//! 独立设置窗口：常规 / 番茄钟 / 标签三个分区。
//!
//! 形态为**另一个 gpui 窗口**（用户已选定），而非主窗覆盖层。
//! 窗口自绘 32px 标题栏：gpui 的 Windows 后端只在 `WindowKind::Dialog` 时设
//! `WS_CAPTION`，`WindowKind::Normal` 即便 `titlebar: Some(..)` 也拿不到系统标题栏
//! （见 `gpui-pre-windows-0.3.10/src/window.rs` 的 `dw_style` 组装），
//! 因此与主窗用同一套 `window_control_area(Drag)` 做法。

use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::*;
use ticktick_core::Database;
use ticktick_core::model::{TAG_PALETTE, color_to_hex};
use ticktick_core::repo::{self, TagCount};

use crate::settings_store::{SettingsChanged, SettingsStore};
use crate::theme_ext as tx;

const WIN_W: f32 = 440.0;
const WIN_H: f32 = 560.0;
const NAV_W: f32 = 104.0;
const HEADER_H: f32 = 34.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    General,
    Pomodoro,
    Tags,
}

impl Section {
    const ALL: [Section; 3] = [Section::General, Section::Pomodoro, Section::Tags];

    fn label(self) -> &'static str {
        match self {
            Section::General => "常规",
            Section::Pomodoro => "番茄钟",
            Section::Tags => "标签",
        }
    }

    fn id(self) -> &'static str {
        match self {
            Section::General => "nav-general",
            Section::Pomodoro => "nav-pomodoro",
            Section::Tags => "nav-tags",
        }
    }
}

/// 打开设置窗口；已存在则把它提到前台，避免开出多个副本。
///
/// `handle` 由主窗持有。判活靠 `AnyWindowHandle::update`——窗口关闭后它会返回 Err。
pub fn open_or_focus(
    handle: &mut Option<AnyWindowHandle>,
    cx: &mut App,
    db: Database,
    store: Entity<SettingsStore>,
) {
    if let Some(existing) = *handle {
        if existing
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
        {
            return;
        }
        *handle = None;
    }

    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(WIN_W), px(WIN_H)),
            cx,
        ))),
        titlebar: None,
        window_min_size: Some(size(px(WIN_W), px(420.0))),
        ..Default::default()
    };

    let opened = gpui_kit::open_window(options, cx, move |window, cx| {
        cx.new(|cx| SettingsWindow::new(window, cx, db, store))
    });

    match opened {
        Ok((new_handle, _view)) => *handle = Some(new_handle),
        Err(e) => log::error!("打开设置窗口失败: {e}"),
    }
}

pub struct SettingsWindow {
    db: Database,
    store: Entity<SettingsStore>,
    section: Section,
    tags: Vec<TagCount>,
    /// 新建标签输入框
    new_tag: Entity<InputState>,
    /// 正在改名的标签：`(tag_id, 输入框)`
    rename: Option<(String, Entity<InputState>)>,
    /// 展开调色板的标签
    palette_for: Option<String>,
    /// 等待二次确认删除的标签
    delete_confirm: Option<String>,
    error: Option<String>,
    _subs: Vec<Subscription>,
}

impl SettingsWindow {
    pub fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        db: Database,
        store: Entity<SettingsStore>,
    ) -> Self {
        let new_tag = cx.new(|cx| InputState::new(window, cx).placeholder("新标签名称，回车创建"));

        let mut subs = Vec::new();
        subs.push(
            cx.subscribe_in(&new_tag, window, |this, _input, event, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.create_tag(window, cx);
                }
            }),
        );
        // 另一侧（如主窗）改了设置也要立刻反映到本窗口
        subs.push(
            cx.subscribe(&store, |this, _store, _event: &SettingsChanged, cx| {
                this.reload_tags(cx);
                cx.notify();
            }),
        );

        let mut me = Self {
            db,
            store,
            section: Section::General,
            tags: Vec::new(),
            new_tag,
            rename: None,
            palette_for: None,
            delete_confirm: None,
            error: None,
            _subs: subs,
        };
        me.reload_tags(cx);
        me
    }

    fn reload_tags(&mut self, cx: &mut Context<Self>) {
        let Ok(conn) = self.db.connect() else { return };
        self.tags = repo::list_tags_with_counts(&conn).unwrap_or_default();
        let _ = cx;
    }

    fn create_tag(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.new_tag.read(cx).value().to_string();
        let name = name.trim().to_string();
        if name.is_empty() {
            return;
        }
        let Ok(conn) = self.db.connect() else { return };
        match repo::create_tag(&conn, &name, None) {
            Ok(_) => {
                self.error = None;
                self.new_tag
                    .update(cx, |state, cx| state.set_value("", window, cx));
            }
            // 重名等冲突把原因显示在页脚，不弹窗打断
            Err(e) => self.error = Some(e.to_string()),
        }
        self.reload_tags(cx);
        cx.notify();
    }

    fn begin_rename(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        tag_id: &str,
        name: &str,
    ) {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("标签名"));
        input.update(cx, |state, cx| state.set_value(name, window, cx));
        self.rename = Some((tag_id.to_string(), input));
        self.palette_for = None;
        self.delete_confirm = None;
        cx.notify();
    }

    fn commit_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((tag_id, input)) = self.rename.clone() else {
            return;
        };
        let name = input.read(cx).value().to_string();
        let name = name.trim().to_string();
        if name.is_empty() {
            self.error = Some("标签名不能为空".to_string());
            cx.notify();
            return;
        }
        let Ok(conn) = self.db.connect() else { return };
        match repo::rename_tag(&conn, &tag_id, &name) {
            Ok(()) => {
                self.rename = None;
                self.error = None;
                self.reload_tags(cx);
                // 改名不影响任务数据，但主窗的标签 chip 要跟着变，广播一次
                self.store.update(cx, |store, cx| store.notify_changed(cx));
                let _ = window;
            }
            Err(e) => self.error = Some(e.to_string()),
        }
        cx.notify();
    }

    fn set_tag_color(&mut self, cx: &mut Context<Self>, tag_id: &str, hex: u32) {
        let Ok(conn) = self.db.connect() else { return };
        if let Err(e) = repo::set_tag_color(&conn, tag_id, Some(&color_to_hex(hex))) {
            self.error = Some(e.to_string());
        } else {
            self.palette_for = None;
            self.error = None;
            self.reload_tags(cx);
        }
        cx.notify();
    }

    fn delete_tag(&mut self, cx: &mut Context<Self>, tag_id: &str) {
        let Ok(conn) = self.db.connect() else { return };
        if let Err(e) = repo::delete_tag(&conn, tag_id) {
            self.error = Some(e.to_string());
        } else {
            self.delete_confirm = None;
            self.error = None;
            self.reload_tags(cx);
        }
        cx.notify();
    }

    // ------------------------------------------------------------------
    // 渲染
    // ------------------------------------------------------------------

    fn render_header(&self, cx: &mut Context<Self>) -> Div {
        div()
            .h(px(HEADER_H))
            .w_full()
            .flex_shrink_0()
            .flex()
            .flex_row()
            .items_center()
            .border_b_1()
            .border_color(tx::rgba(tx::BORDER))
            .bg(tx::rgba(tx::BG_SUB))
            .child(
                div()
                    .id("settings-drag")
                    .flex_1()
                    .h_full()
                    .flex()
                    .items_center()
                    .pl_3()
                    .window_control_area(WindowControlArea::Drag)
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(tx::rgba(tx::TEXT_MUTED))
                            .child("设置"),
                    ),
            )
            .child(
                div()
                    .id("settings-close")
                    .cursor_pointer()
                    .px_3()
                    .h_full()
                    .flex()
                    .items_center()
                    .text_sm()
                    .text_color(tx::rgba(tx::TEXT_MUTED))
                    .hover(|s| s.bg(tx::rgba(tx::BG_HOVER)).text_color(tx::rgba(tx::TEXT)))
                    .on_click(cx.listener(|_this, _, window, _cx| window.remove_window()))
                    .child("✕"),
            )
    }

    fn render_nav(&self, cx: &mut Context<Self>) -> Div {
        let mut nav = div()
            .w(px(NAV_W))
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .gap_1()
            .p_2()
            .bg(tx::rgba(tx::BG_SUB));

        for section in Section::ALL {
            let selected = self.section == section;
            nav = nav.child(
                div()
                    .id(section.id())
                    .cursor_pointer()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .text_sm()
                    .when(selected, |d| {
                        d.bg(tx::rgba(tx::BG_ACTIVE))
                            .text_color(tx::rgba(tx::ACCENT))
                    })
                    .when(!selected, |d| d.text_color(tx::rgba(tx::TEXT_MUTED)))
                    .hover(|s| s.bg(tx::rgba(tx::BG_HOVER)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.section = section;
                        cx.notify();
                    }))
                    .child(section.label()),
            );
        }
        nav
    }

    fn render_content(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let body = match self.section {
            Section::General => self.render_general(cx),
            Section::Pomodoro => self.render_pomodoro(cx),
            Section::Tags => self.render_tags(cx),
        };

        let mut wrap = div()
            .id("settings-content")
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_y_scroll()
            .p_4()
            .flex()
            .flex_col()
            .gap_2()
            .child(body);

        if let Some(err) = &self.error {
            wrap = wrap.child(
                div()
                    .mt_2()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .bg(tx::rgba(tx::DANGER).opacity(0.08))
                    .text_xs()
                    .text_color(tx::rgba(tx::DANGER))
                    .child(err.clone()),
            );
        }
        wrap
    }

    fn render_general(&self, cx: &mut Context<Self>) -> Div {
        let s = self.store.read(cx).get();
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(section_title("窗口"))
            .child(setting_row(
                "始终置顶",
                "让窗口浮在其他应用之上",
                switch(
                    "switch-topmost",
                    s.topmost,
                    cx.listener(|this, _, _, cx| {
                        this.store
                            .update(cx, |store, cx| store.edit(cx, |s| s.topmost = !s.topmost));
                    }),
                ),
            ))
            .child(setting_row(
                "显示已完成",
                "任务列表包含已勾选的条目",
                switch(
                    "switch-completed",
                    s.show_completed,
                    cx.listener(|this, _, _, cx| {
                        this.store.update(cx, |store, cx| {
                            store.edit(cx, |s| s.show_completed = !s.show_completed)
                        });
                    }),
                ),
            ))
    }

    fn render_pomodoro(&self, cx: &mut Context<Self>) -> Div {
        let s = self.store.read(cx).get();
        let p = s.pomodoro;

        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(section_title("时长（分钟）"))
            .child(setting_row(
                "专注",
                "1 – 180",
                stepper(
                    "step-focus",
                    p.focus_mins,
                    "分",
                    cx,
                    |this, delta, _, cx| {
                        this.store.update(cx, |store, cx| {
                            store.edit(cx, |s| {
                                s.pomodoro.focus_mins = bump(s.pomodoro.focus_mins, delta, 1, 180)
                            })
                        });
                    },
                ),
            ))
            .child(setting_row(
                "短休息",
                "1 – 60",
                stepper(
                    "step-short",
                    p.short_break_mins,
                    "分",
                    cx,
                    |this, delta, _, cx| {
                        this.store.update(cx, |store, cx| {
                            store.edit(cx, |s| {
                                s.pomodoro.short_break_mins =
                                    bump(s.pomodoro.short_break_mins, delta, 1, 60)
                            })
                        });
                    },
                ),
            ))
            .child(setting_row(
                "长休息",
                "1 – 120",
                stepper(
                    "step-long",
                    p.long_break_mins,
                    "分",
                    cx,
                    |this, delta, _, cx| {
                        this.store.update(cx, |store, cx| {
                            store.edit(cx, |s| {
                                s.pomodoro.long_break_mins =
                                    bump(s.pomodoro.long_break_mins, delta, 1, 120)
                            })
                        });
                    },
                ),
            ))
            .child(setting_row(
                "长休间隔",
                "连续几轮专注后长休息",
                stepper(
                    "step-rounds",
                    p.rounds_per_long_break,
                    "轮",
                    cx,
                    |this, delta, _, cx| {
                        this.store.update(cx, |store, cx| {
                            store.edit(cx, |s| {
                                s.pomodoro.rounds_per_long_break =
                                    bump(s.pomodoro.rounds_per_long_break, delta, 1, 12)
                            })
                        });
                    },
                ),
            ))
            .child(section_title("提醒"))
            .child(setting_row(
                "系统通知",
                "每个阶段结束时弹桌面通知",
                switch(
                    "switch-notify",
                    p.notify,
                    cx.listener(|this, _, _, cx| {
                        this.store.update(cx, |store, cx| {
                            store.edit(cx, |s| s.pomodoro.notify = !s.pomodoro.notify)
                        });
                    }),
                ),
            ))
    }

    fn render_tags(&mut self, cx: &mut Context<Self>) -> Div {
        let tags = self.tags.clone();
        let rename_id = self.rename.as_ref().map(|(id, _)| id.clone());
        let palette_for = self.palette_for.clone();
        let delete_confirm = self.delete_confirm.clone();

        let mut list = div().flex().flex_col().gap_1();

        if tags.is_empty() {
            list = list.child(
                div()
                    .py_4()
                    .text_xs()
                    .text_color(tx::rgba(tx::TEXT_FAINT))
                    .child("还没有标签。给任务打标签，或直接在下面新建。"),
            );
        }

        for item in &tags {
            let tag = &item.tag;
            let id = tag.id.clone();
            let hex = tag.color_hex();

            if delete_confirm.as_deref() == Some(tag.id.as_str()) {
                let del_id = id.clone();
                let cancel = cx.listener(|this, _, _, cx| {
                    this.delete_confirm = None;
                    cx.notify();
                });
                list = list.child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .bg(tx::rgba(tx::DANGER).opacity(0.06))
                        .child(
                            div()
                                .flex_1()
                                .text_xs()
                                .text_color(tx::rgba(tx::DANGER))
                                .child(format!("删除「{}」？任务本身不会被删除", tag.name)),
                        )
                        .child(small_button(
                            SharedString::from(format!("tag-del-cancel-{}", tag.id)),
                            "取消",
                            tx::TEXT_MUTED,
                            cancel,
                        ))
                        .child(small_button(
                            SharedString::from(format!("tag-del-confirm-{}", tag.id)),
                            "删除",
                            tx::DANGER,
                            cx.listener(move |this, _, _, cx| {
                                this.delete_tag(cx, &del_id);
                            }),
                        )),
                );
                continue;
            }

            let mut row = div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .px_2()
                .py_1()
                .rounded_md()
                .hover(|s| s.bg(tx::rgba(tx::BG_HOVER)))
                // 色点：点击展开调色板
                .child(
                    div()
                        .id(SharedString::from(format!("tag-dot-{}", tag.id)))
                        .cursor_pointer()
                        .size(px(14.0))
                        .flex_shrink_0()
                        .rounded_full()
                        .bg(rgb(hex))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.palette_for = if this.palette_for.as_deref() == Some(&id) {
                                None
                            } else {
                                Some(id.clone())
                            };
                            cx.notify();
                        })),
                );

            // 名称 / 改名输入
            if rename_id.as_deref() == Some(tag.id.as_str()) {
                let input = self.rename.as_ref().map(|(_, i)| i.clone()).unwrap();
                row = row
                    .child(div().flex_1().min_w_0().child(Input::new(&input)))
                    .child(small_button(
                        SharedString::from(format!("tag-rename-save-{}", tag.id)),
                        "保存",
                        tx::ACCENT,
                        cx.listener(|this, _, window, cx| this.commit_rename(window, cx)),
                    ))
                    .child(small_button(
                        SharedString::from(format!("tag-rename-cancel-{}", tag.id)),
                        "取消",
                        tx::TEXT_MUTED,
                        cx.listener(|this, _, _, cx| {
                            this.rename = None;
                            cx.notify();
                        }),
                    ));
            } else {
                let name = tag.name.clone();
                let for_rename = tag.id.clone();
                let del_id = tag.id.clone();
                row = row
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .truncate()
                            .text_color(tx::rgba(tx::TEXT))
                            .child(name.clone()),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(tx::rgba(tx::TEXT_FAINT))
                            .child(format!("{} 进行中", item.active)),
                    )
                    .child(small_button(
                        SharedString::from(format!("tag-rename-{}", tag.id)),
                        "✎",
                        tx::TEXT_FAINT,
                        cx.listener(move |this, _, window, cx| {
                            this.begin_rename(window, cx, &for_rename, &name);
                        }),
                    ))
                    .child(small_button(
                        SharedString::from(format!("tag-delete-{}", tag.id)),
                        "✕",
                        tx::TEXT_FAINT,
                        cx.listener(move |this, _, _, cx| {
                            this.delete_confirm = Some(del_id.clone());
                            this.palette_for = None;
                            cx.notify();
                        }),
                    ));
            }

            list = list.child(row);

            if palette_for.as_deref() == Some(tag.id.as_str()) {
                let mut palette = div().flex().flex_row().items_center().gap_2().px_2().pb_2();
                for hex in TAG_PALETTE {
                    let tid = tag.id.clone();
                    palette = palette.child(
                        div()
                            .id(SharedString::from(format!("pal-{}-{hex:06X}", tag.id)))
                            .cursor_pointer()
                            .size(px(18.0))
                            .rounded_full()
                            .bg(rgb(hex))
                            .border_1()
                            .border_color(tx::rgba(if hex == tag.color_hex() {
                                tx::TEXT
                            } else {
                                tx::BORDER
                            }))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.set_tag_color(cx, &tid, hex);
                            })),
                    );
                }
                list = list.child(palette);
            }
        }

        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(section_title("全部标签"))
            .child(list)
            .child(
                div()
                    .mt_2()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .text_color(tx::rgba(tx::TEXT_FAINT))
                            .child("新建标签"),
                    )
                    .child(Input::new(&self.new_tag)),
            )
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let header = self.render_header(cx);
        let nav = self.render_nav(cx);
        let content = self.render_content(cx);

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(tx::rgba(tx::BG))
            .text_color(tx::rgba(tx::TEXT))
            .child(header)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_h_0()
                    .child(nav)
                    .child(content),
            )
    }
}

// ---------------------------------------------------------------------------
// 小构件
// ---------------------------------------------------------------------------

fn bump(value: u32, delta: i32, min: u32, max: u32) -> u32 {
    let next = i64::from(value) + i64::from(delta);
    (next.clamp(i64::from(min), i64::from(max))) as u32
}

fn section_title(text: &'static str) -> Div {
    div()
        .mt_2()
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(tx::rgba(tx::TEXT_FAINT))
        .child(text)
}

fn setting_row(label: &'static str, hint: &'static str, control: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap_3()
        .py_2()
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().text_sm().child(label))
                .child(
                    div()
                        .text_xs()
                        .text_color(tx::rgba(tx::TEXT_FAINT))
                        .child(hint),
                ),
        )
        .child(control)
}

fn switch(
    id: &'static str,
    on: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex_shrink_0()
        .cursor_pointer()
        .w(px(34.0))
        .h(px(18.0))
        .px(px(2.0))
        .rounded_full()
        .bg(rgb(if on { tx::ACCENT } else { 0xD9DCE1 }))
        .flex()
        .items_center()
        .when(on, |d| d.justify_end())
        .when(!on, |d| d.justify_start())
        .on_click(on_click)
        .child(div().size(px(14.0)).rounded_full().bg(rgb(0xFFFFFF)))
}

fn stepper<F>(
    id: &'static str,
    value: u32,
    unit: &'static str,
    cx: &mut Context<SettingsWindow>,
    on_delta: F,
) -> Div
where
    F: Fn(&mut SettingsWindow, i32, &mut Window, &mut Context<SettingsWindow>) + Clone + 'static,
{
    let minus = on_delta.clone();
    div()
        .flex_shrink_0()
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .child(step_button(
            SharedString::from(format!("{id}-minus")),
            "−",
            cx.listener(move |this, _, window, cx| minus(this, -1, window, cx)),
        ))
        .child(
            div()
                .w(px(52.0))
                .text_sm()
                .text_color(tx::rgba(tx::TEXT))
                .child(format!("{value} {unit}")),
        )
        .child(step_button(
            SharedString::from(format!("{id}-plus")),
            "+",
            cx.listener(move |this, _, window, cx| on_delta(this, 1, window, cx)),
        ))
}

fn step_button(
    id: impl Into<ElementId>,
    label: &'static str,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .cursor_pointer()
        .size(px(22.0))
        .rounded_md()
        .border_1()
        .border_color(tx::rgba(tx::BORDER))
        .bg(tx::rgba(tx::BG_BTN))
        .flex()
        .items_center()
        .justify_center()
        .text_sm()
        .text_color(tx::rgba(tx::TEXT_MUTED))
        .hover(|s| s.bg(tx::rgba(tx::BG_BTN_HOVER)))
        .on_click(on_click)
        .child(label)
}

fn small_button(
    id: impl Into<ElementId>,
    label: &'static str,
    color: u32,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .flex_shrink_0()
        .cursor_pointer()
        .px_2()
        .py_1()
        .rounded_md()
        .text_xs()
        .text_color(tx::rgba(color))
        .hover(|s| s.bg(tx::rgba(tx::BG_BTN_HOVER)))
        .on_click(on_click)
        .child(label)
}
