//! 番茄钟面板：状态机 + 1s 心跳 + 右下角浮层卡片。
//!
//! 形态见 `docs/requirements/REQ-0003-pomodoro-floating.md`：
//! - 默认不展开（`expanded = false`），窗口底部不再有常驻条；
//! - 展开后由 app 壳定位到右下角，卡片自身不关心挂载位置；
//! - 标题栏的入口文案由 [`PomodoroPanel::titlebar_label`] 提供；
//! - 时长全部来自 `core::settings::PomodoroConfig`，改设置后 `apply_config` 热生效。

use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::*;
use ticktick_core::model::{new_id, now_ts};
use ticktick_core::plugin::AUMID;
use ticktick_core::{Database, PomodoroConfig, PomodoroKind, PomodoroSession, repo};

const COLOR_FOCUS: u32 = 0xE5484D;
const COLOR_BREAK: u32 = 0x30A46C;
const COLOR_TEXT: u32 = 0x1F2328;
const COLOR_MUTED: u32 = 0x6B7280;
const COLOR_TRACK: u32 = 0xE5E7EB;
const COLOR_BTN: u32 = 0xEFF1F4;
const COLOR_BORDER: u32 = 0xE5E7EB;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Idle,
    Focus,
    ShortBreak,
    LongBreak,
}

impl Phase {
    fn label(self) -> &'static str {
        match self {
            Phase::Idle => "就绪",
            Phase::Focus => "专注中",
            Phase::ShortBreak => "短休息",
            Phase::LongBreak => "长休息",
        }
    }
}

/// 插件向 app 壳发出的事件。
#[derive(Debug, Clone)]
pub enum PomodoroEvent {
    /// 一个专注 session 结束（完成或放弃），app 应刷新任务专注统计。
    FocusSessionEnded {
        task_id: Option<String>,
        finished: bool,
    },
    /// 阶段变化（休息结束等）。
    PhaseChanged,
}

pub struct PomodoroPanel {
    db: Database,
    /// 浮层是否展开（默认收起，只留标题栏图标）
    expanded: bool,
    phase: Phase,
    running: bool,
    remaining_secs: i64,
    planned_secs: i64,
    /// 已连续完成的专注轮数（用于长休判定）
    cycles: u32,
    /// 关联任务 (id, title)
    linked_task: Option<(String, String)>,
    session_id: Option<String>,
    session_started_at: i64,
    config: PomodoroConfig,
    _tick: Option<Task<()>>,
}

impl EventEmitter<PomodoroEvent> for PomodoroPanel {}

impl PomodoroPanel {
    pub fn new(_cx: &mut Context<Self>, db: Database) -> Self {
        let config = Self::load_config(&db);
        Self {
            db,
            expanded: false,
            phase: Phase::Idle,
            running: false,
            remaining_secs: config.focus_secs(),
            planned_secs: config.focus_secs(),
            cycles: 0,
            linked_task: None,
            session_id: None,
            session_started_at: 0,
            config,
            _tick: None,
        }
    }

    fn load_config(db: &Database) -> PomodoroConfig {
        db.connect()
            .ok()
            .and_then(|conn| ticktick_core::AppSettings::load(&conn).ok())
            .map(|s| s.pomodoro)
            .unwrap_or_default()
            .clamped()
    }

    /// 设置窗口改了时长后由 app 壳调用。
    ///
    /// Idle 态顺带刷新剩余时间（让"改成 1 分钟"立刻可见）；
    /// 运行中只换参数，不打断当前这一轮计时。
    pub fn apply_config(&mut self, config: PomodoroConfig, cx: &mut Context<Self>) {
        self.config = config.clamped();
        if self.phase == Phase::Idle && !self.running {
            self.planned_secs = self.config.focus_secs();
            self.remaining_secs = self.planned_secs;
        }
        cx.notify();
    }

    pub fn is_expanded(&self) -> bool {
        self.expanded
    }

    pub fn toggle_expanded(&mut self, cx: &mut Context<Self>) {
        self.expanded = !self.expanded;
        cx.notify();
    }

    /// 标题栏入口文案：Idle 显示图标，运行/暂停时显示等宽倒计时。
    pub fn titlebar_label(&self) -> (String, bool) {
        match self.phase {
            Phase::Idle => ("🍅".to_string(), false),
            _ => (self.time_text(), true),
        }
    }

    /// 从任务行发起的入口：关联任务并开始专注。
    pub fn start_for_task(&mut self, task_id: String, title: String, cx: &mut Context<Self>) {
        self.linked_task = Some((task_id, title));
        self.expanded = true;
        self.start_phase(Phase::Focus, cx);
    }

    pub fn is_active(&self) -> bool {
        self.running
    }

    fn time_text(&self) -> String {
        format!(
            "{:02}:{:02}",
            self.remaining_secs / 60,
            self.remaining_secs % 60
        )
    }

    fn phase_secs(&self, phase: Phase) -> i64 {
        match phase {
            Phase::Focus | Phase::Idle => self.config.focus_secs(),
            Phase::ShortBreak => self.config.short_break_secs(),
            Phase::LongBreak => self.config.long_break_secs(),
        }
    }

    fn start_phase(&mut self, phase: Phase, cx: &mut Context<Self>) {
        // 若之前有未完成的 session，先按放弃落库
        self.abandon_pending_session();
        self.phase = phase;
        self.planned_secs = self.phase_secs(phase);
        self.remaining_secs = self.planned_secs;
        self.running = true;
        if phase != Phase::Idle {
            let id = new_id();
            self.session_started_at = now_ts();
            self.session_id = Some(id.clone());
            let kind = match phase {
                Phase::Focus => PomodoroKind::Focus,
                Phase::LongBreak => PomodoroKind::LongBreak,
                _ => PomodoroKind::ShortBreak,
            };
            let session = PomodoroSession {
                id,
                task_id: if kind == PomodoroKind::Focus {
                    self.linked_task.as_ref().map(|t| t.0.clone())
                } else {
                    None
                },
                kind,
                planned_secs: self.planned_secs,
                actual_secs: 0,
                started_at: self.session_started_at,
                ended_at: None,
                finished: false,
            };
            if let Ok(conn) = self.db.connect() {
                let _ = repo::insert_pomodoro_session(&conn, &session);
            }
        }
        self.ensure_tick(cx);
        cx.emit(PomodoroEvent::PhaseChanged);
        cx.notify();
    }

    fn abandon_pending_session(&mut self) {
        if let Some(id) = self.session_id.take() {
            let actual = (now_ts() - self.session_started_at).max(0);
            if let Ok(conn) = self.db.connect() {
                let _ = repo::finish_pomodoro_session(&conn, &id, actual, now_ts(), false);
            }
        }
    }

    fn ensure_tick(&mut self, cx: &mut Context<Self>) {
        if self._tick.is_some() {
            return;
        }
        self._tick = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                if this.update(cx, |this, cx| this.tick(cx)).is_err() {
                    break;
                }
            }
        }));
    }

    fn tick(&mut self, cx: &mut Context<Self>) {
        if !self.running {
            return;
        }
        if self.remaining_secs > 0 {
            self.remaining_secs -= 1;
        }
        if self.remaining_secs <= 0 {
            self.complete_phase(cx);
        }
        cx.notify();
    }

    fn complete_phase(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.session_id.take() {
            if let Ok(conn) = self.db.connect() {
                let _ =
                    repo::finish_pomodoro_session(&conn, &id, self.planned_secs, now_ts(), true);
            }
        }
        self.running = false;
        match self.phase {
            Phase::Focus => {
                self.cycles += 1;
                let task_id = self.linked_task.as_ref().map(|t| t.0.clone());
                cx.emit(PomodoroEvent::FocusSessionEnded {
                    task_id,
                    finished: true,
                });
                self.notify(cx, "专注完成", "休息一下吧，下一轮更高效");
                let next = if self.cycles % self.config.rounds_per_long_break.max(1) == 0 {
                    Phase::LongBreak
                } else {
                    Phase::ShortBreak
                };
                self.start_phase(next, cx);
            }
            Phase::ShortBreak | Phase::LongBreak => {
                self.notify(cx, "休息结束", "开始下一个专注吧");
                self.phase = Phase::Idle;
                self.planned_secs = self.config.focus_secs();
                self.remaining_secs = self.planned_secs;
                cx.emit(PomodoroEvent::PhaseChanged);
                cx.notify();
            }
            Phase::Idle => {}
        }
    }

    fn toggle_pause(&mut self, cx: &mut Context<Self>) {
        if self.phase == Phase::Idle {
            return;
        }
        self.running = !self.running;
        cx.notify();
    }

    fn give_up(&mut self, cx: &mut Context<Self>) {
        let was_focus = self.phase == Phase::Focus;
        let task_id = self.linked_task.as_ref().map(|t| t.0.clone());
        self.abandon_pending_session();
        self.phase = Phase::Idle;
        self.running = false;
        self.planned_secs = self.config.focus_secs();
        self.remaining_secs = self.planned_secs;
        if was_focus {
            cx.emit(PomodoroEvent::FocusSessionEnded {
                task_id,
                finished: false,
            });
        }
        cx.notify();
    }

    fn skip_break(&mut self, cx: &mut Context<Self>) {
        if matches!(self.phase, Phase::ShortBreak | Phase::LongBreak) {
            self.give_up(cx);
        }
    }

    fn progress(&self) -> f32 {
        if self.planned_secs <= 0 {
            return 0.0;
        }
        (1.0 - self.remaining_secs as f32 / self.planned_secs as f32).clamp(0.0, 1.0)
    }

    /// 发系统通知。开关关闭时不创建后台任务；失败只 warn，不影响计时流转。
    fn notify(&self, cx: &mut Context<Self>, title: &str, body: &str) {
        if !self.config.notify {
            return;
        }
        let (summary, content) = (title.to_string(), body.to_string());
        cx.background_executor()
            .spawn(async move {
                let result = notify_rust::Notification::new()
                    .app_id(AUMID)
                    .appname("滴答 Lite")
                    .summary(&summary)
                    .body(&content)
                    .show();
                if let Err(e) = result {
                    log::warn!("系统通知发送失败: {e}");
                }
            })
            .detach();
    }
}

impl Render for PomodoroPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut card = div().w_full().flex().flex_col().gap_2();
        if !self.expanded {
            // 收起态不占空间：入口在标题栏
            return card;
        }

        let bar_color = match self.phase {
            Phase::ShortBreak | Phase::LongBreak => COLOR_BREAK,
            _ => COLOR_FOCUS,
        };
        let percent = (self.progress() * 100.0).round() as i32;

        card = card
            .p_3()
            .rounded_lg()
            .border_1()
            .border_color(rgb(COLOR_BORDER))
            .bg(rgb(0xFFFFFF))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(COLOR_MUTED))
                            .child(self.phase.label().to_string()),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_xl()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(if self.phase == Phase::Idle {
                                        COLOR_MUTED
                                    } else {
                                        bar_color
                                    }))
                                    .child(self.time_text()),
                            )
                            .child(
                                div()
                                    .id("pomo-collapse")
                                    .cursor_pointer()
                                    .px_1()
                                    .text_xs()
                                    .text_color(rgb(COLOR_MUTED))
                                    .hover(|s| s.text_color(rgb(COLOR_TEXT)))
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.toggle_expanded(cx)),
                                    )
                                    .child("▾"),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .h_1()
                            .rounded_full()
                            .bg(rgb(COLOR_TRACK))
                            .child(
                                div()
                                    .h_full()
                                    .w(relative(self.progress()))
                                    .rounded_full()
                                    .bg(rgb(bar_color)),
                            ),
                    )
                    .child(
                        div()
                            .w(px(34.0))
                            .text_xs()
                            .text_color(rgb(COLOR_MUTED))
                            .child(format!("{percent}%")),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_xs()
                            .truncate()
                            .text_color(rgb(COLOR_MUTED))
                            .child(match &self.linked_task {
                                Some((_, title)) => format!("关联任务：{title}"),
                                None => "未关联任务".to_string(),
                            }),
                    )
                    .when(self.linked_task.is_some(), |el| {
                        el.child(
                            small_button("pomo-unlink", "×", false).on_click(cx.listener(
                                |this, _, _, cx| {
                                    this.linked_task = None;
                                    cx.notify();
                                },
                            )),
                        )
                    }),
            );

        let controls = div().flex().flex_row().gap_2();
        let controls = match (self.phase, self.running) {
            (Phase::Idle, _) => controls.child(
                small_button("pomo-start", "开始专注", true)
                    .on_click(cx.listener(|this, _, _, cx| this.start_phase(Phase::Focus, cx))),
            ),
            (_, true) => controls
                .child(
                    small_button("pomo-pause", "暂停", false)
                        .on_click(cx.listener(|this, _, _, cx| this.toggle_pause(cx))),
                )
                .child(
                    if matches!(self.phase, Phase::ShortBreak | Phase::LongBreak) {
                        small_button("pomo-skip", "跳过休息", false)
                            .on_click(cx.listener(|this, _, _, cx| this.skip_break(cx)))
                    } else {
                        small_button("pomo-giveup", "放弃", false)
                            .on_click(cx.listener(|this, _, _, cx| this.give_up(cx)))
                    },
                ),
            (_, false) => controls
                .child(
                    small_button("pomo-resume", "继续", true)
                        .on_click(cx.listener(|this, _, _, cx| this.toggle_pause(cx))),
                )
                .child(
                    small_button("pomo-giveup", "放弃", false)
                        .on_click(cx.listener(|this, _, _, cx| this.give_up(cx))),
                ),
        };

        card.child(controls)
    }
}

fn small_button(id: &'static str, label: &'static str, primary: bool) -> Stateful<Div> {
    let base = div()
        .id(id)
        .cursor_pointer()
        .px_2()
        .py_1()
        .rounded_md()
        .text_xs();
    if primary {
        base.bg(rgb(COLOR_FOCUS))
            .text_color(rgb(0xFFFFFF))
            .hover(|s| s.bg(rgb(0xD13B40)))
            .child(label)
    } else {
        base.bg(rgb(COLOR_BTN))
            .text_color(rgb(COLOR_TEXT))
            .hover(|s| s.bg(rgb(0xE2E5E9)))
            .child(label)
    }
}
