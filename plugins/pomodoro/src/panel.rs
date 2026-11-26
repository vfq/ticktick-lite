//! 番茄钟面板：状态机 + 1s 心跳 + gpui 渲染。

use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::*;
use ticktick_core::model::{new_id, now_ts};
use ticktick_core::{repo, Database, PomodoroKind, PomodoroSession};

const DEFAULT_FOCUS_MINS: i64 = 25;
const DEFAULT_SHORT_BREAK_MINS: i64 = 5;
const DEFAULT_LONG_BREAK_MINS: i64 = 15;
const ROUNDS_PER_LONG_BREAK: u32 = 4;

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
    collapsed: bool,
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
    focus_mins: i64,
    short_break_mins: i64,
    long_break_mins: i64,
    _tick: Option<Task<()>>,
}

impl EventEmitter<PomodoroEvent> for PomodoroPanel {}

impl PomodoroPanel {
    pub fn new(_cx: &mut Context<Self>, db: Database) -> Self {
        let (focus, short_b, long_b) = Self::load_settings(&db);
        Self {
            db,
            collapsed: false,
            phase: Phase::Idle,
            running: false,
            remaining_secs: focus * 60,
            planned_secs: focus * 60,
            cycles: 0,
            linked_task: None,
            session_id: None,
            session_started_at: 0,
            focus_mins: focus,
            short_break_mins: short_b,
            long_break_mins: long_b,
            _tick: None,
        }
    }

    fn load_settings(db: &Database) -> (i64, i64, i64) {
        let read = |key: &str, default: i64| -> i64 {
            db.connect()
                .ok()
                .and_then(|conn| repo::get_setting(&conn, key).ok().flatten())
                .and_then(|v| v.parse::<i64>().ok())
                .filter(|v| *v > 0 && *v <= 180)
                .unwrap_or(default)
        };
        (
            read("pomodoro_focus_mins", DEFAULT_FOCUS_MINS),
            read("pomodoro_short_break_mins", DEFAULT_SHORT_BREAK_MINS),
            read("pomodoro_long_break_mins", DEFAULT_LONG_BREAK_MINS),
        )
    }

    /// 从任务行发起的入口：关联任务并开始专注。
    pub fn start_for_task(&mut self, task_id: String, title: String, cx: &mut Context<Self>) {
        self.linked_task = Some((task_id, title));
        self.collapsed = false;
        self.start_phase(Phase::Focus, cx);
    }

    pub fn is_active(&self) -> bool {
        self.running
    }

    fn phase_secs(&self, phase: Phase) -> i64 {
        match phase {
            Phase::Focus => self.focus_mins * 60,
            Phase::ShortBreak => self.short_break_mins * 60,
            Phase::LongBreak => self.long_break_mins * 60,
            Phase::Idle => self.focus_mins * 60,
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
            let was_focus = self.phase == Phase::Focus;
            if let Ok(conn) = self.db.connect() {
                let _ = repo::finish_pomodoro_session(&conn, &id, actual, now_ts(), false);
            }
            if was_focus {
                // 注意：此时不能 emit（没有 cx），由调用方在后续 emit
            }
        }
    }

    fn ensure_tick(&mut self, cx: &mut Context<Self>) {
        if self._tick.is_some() {
            return;
        }
        self._tick = Some(cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            if this.update(cx, |this, cx| this.tick(cx)).is_err() {
                break;
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
                toast("专注完成", "休息一下吧，下一轮更高效");
                let next = if self.cycles % ROUNDS_PER_LONG_BREAK == 0 {
                    Phase::LongBreak
                } else {
                    Phase::ShortBreak
                };
                self.start_phase(next, cx);
            }
            Phase::ShortBreak | Phase::LongBreak => {
                toast("休息结束", "开始下一个专注吧");
                self.phase = Phase::Idle;
                self.planned_secs = self.focus_mins * 60;
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
        self.planned_secs = self.focus_mins * 60;
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
}

impl Render for PomodoroPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let running_or_paused = self.phase != Phase::Idle;
        let time_text = format!(
            "{:02}:{:02}",
            self.remaining_secs / 60,
            self.remaining_secs % 60
        );
        let bar_color = match self.phase {
            Phase::Focus => COLOR_FOCUS,
            Phase::ShortBreak | Phase::LongBreak => COLOR_BREAK,
            Phase::Idle => COLOR_FOCUS,
        };
        let collapse_icon = if self.collapsed { "▸" } else { "▾" };

        let mut body = div().flex().flex_col().gap_2();
        if !self.collapsed {
            // 时间 + 阶段
            body = body.child(
                div()
                    .flex()
                    .flex_row()
                    .items_baseline()
                    .gap_2()
                    .child(
                        div()
                            .text_2xl()
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(if self.phase == Phase::Idle {
                                COLOR_MUTED
                            } else {
                                bar_color
                            }))
                            .child(time_text),
                    )
                    .child(div().text_xs().text_color(rgb(COLOR_MUTED)).child(format!(
                        "{} · 第 {} 轮",
                        self.phase.label(),
                        self.cycles + 1
                    ))),
            );
            // 进度条
            body = body.child(
                div()
                    .w_full()
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
            );
            // 关联任务
            let linked = match &self.linked_task {
                Some((_, title)) => format!("关联任务：{title}"),
                None => "未关联任务（点任务行右侧 ▶ 发起专注）".to_string(),
            };
            body = body.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(COLOR_MUTED))
                            .overflow_hidden()
                            .child(linked),
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
            // 控制按钮
            let controls = div().flex().flex_row().gap_2();
            let controls = match (self.phase, self.running) {
                (Phase::Idle, _) => controls
                    .child(small_button("pomo-start", "开始专注", true).on_click(
                        cx.listener(|this, _, _, cx| this.start_phase(Phase::Focus, cx)),
                    )),
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
            body = body.child(controls);
            let _ = running_or_paused;
        }

        div()
            .flex()
            .flex_col()
            .gap_2()
            .px_3()
            .py_2()
            .border_t_1()
            .border_color(rgb(COLOR_BORDER))
            .bg(rgb(0xFAFBFC))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(COLOR_TEXT))
                            .child("🍅 番茄钟"),
                    )
                    .child(
                        div()
                            .id("pomo-collapse")
                            .cursor_pointer()
                            .px_1()
                            .text_xs()
                            .text_color(rgb(COLOR_MUTED))
                            .hover(|s| s.text_color(rgb(COLOR_TEXT)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.collapsed = !this.collapsed;
                                cx.notify();
                            }))
                            .child(collapse_icon),
                    ),
            )
            .child(body)
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

fn toast(title: &str, body: &str) {
    let _ = notify_rust::Notification::new()
        .appname("TickTick Lite")
        .summary(title)
        .body(body)
        .show();
}
