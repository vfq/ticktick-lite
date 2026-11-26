//! 根视图：标题栏 + 侧栏 + 任务区 + 番茄钟插件区 + 覆盖层。

mod overlays;
mod sidebar;
mod task_list;
mod titlebar;

use std::collections::HashMap;
use std::time::Duration;

use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::*;
use ticktick_core::model::{now_ts, Priority, TaskStatus};
use ticktick_core::repo::{self, NewTask, TaskFilter};
use ticktick_core::{Database, Task as CoreTask, Vault};

use crate::theme_ext as tx;

#[cfg(feature = "pomodoro")]
use ticktick_plugin_pomodoro::{PomodoroEvent, PomodoroPanel};

pub use overlays::Overlay;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    #[allow(dead_code)] // 无插件构建下暂未使用，保留供后续提示类型扩展
    Info,
    Error,
}

pub struct RootView {
    pub(crate) db: Database,
    pub(crate) vaults: Vec<Vault>,
    pub(crate) tasks: Vec<CoreTask>,
    pub(crate) focus_totals: HashMap<String, i64>,
    pub(crate) vault_counts: HashMap<String, i64>,
    pub(crate) total_active: i64,
    pub(crate) filter_vault: Option<String>,
    pub(crate) show_completed: bool,
    pub(crate) topmost: bool,
    pub(crate) quick_add: Entity<InputState>,
    pub(crate) overlay: Option<Overlay>,
    pub(crate) toast: Option<(String, ToastKind)>,
    _toast_task: Option<Task<()>>,
    #[cfg(feature = "pomodoro")]
    pub(crate) pomodoro: Entity<PomodoroPanel>,
    _subs: Vec<Subscription>,
}

impl RootView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>, db: Database) -> Self {
        let quick_add =
            cx.new(|cx| InputState::new(window, cx).placeholder("添加任务，回车快速创建"));

        let mut subs: Vec<Subscription> = Vec::new();
        subs.push(
            cx.subscribe_in(&quick_add, window, |this, _input, event, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.quick_add_task(window, cx);
                }
            }),
        );

        #[cfg(feature = "pomodoro")]
        let pomodoro = {
            let panel = cx.new(|cx| PomodoroPanel::new(cx, db.clone()));
            subs.push(cx.subscribe_in(
                &panel,
                window,
                |this, _panel, event, _window, cx| match event {
                    PomodoroEvent::FocusSessionEnded { finished, .. } => {
                        this.reload(cx);
                        if *finished {
                            this.show_toast(
                                "一个专注时段已完成 🍅".to_string(),
                                ToastKind::Info,
                                cx,
                            );
                        }
                    }
                    PomodoroEvent::PhaseChanged => {}
                },
            ));
            panel
        };

        let mut view = Self {
            db,
            vaults: Vec::new(),
            tasks: Vec::new(),
            focus_totals: HashMap::new(),
            vault_counts: HashMap::new(),
            total_active: 0,
            filter_vault: None,
            show_completed: false,
            topmost: false,
            quick_add,
            overlay: None,
            toast: None,
            _toast_task: None,
            #[cfg(feature = "pomodoro")]
            pomodoro,
            _subs: subs,
        };
        view.reload(cx);
        view
    }

    /// 从数据库重新加载 vault / 任务 / 专注统计。
    pub(crate) fn reload(&mut self, cx: &mut Context<Self>) {
        let Ok(conn) = self.db.connect() else { return };
        self.vaults = repo::list_vaults(&conn).unwrap_or_default();
        if let Some(fid) = &self.filter_vault {
            if !self.vaults.iter().any(|v| &v.id == fid) {
                self.filter_vault = None;
            }
        }
        self.vault_counts = repo::active_counts_by_vault(&conn).unwrap_or_default();
        self.total_active = repo::total_active_count(&conn).unwrap_or(0);
        let status = if self.show_completed {
            None
        } else {
            Some(TaskStatus::Active)
        };
        self.tasks = repo::list_tasks(
            &conn,
            &TaskFilter {
                vault_id: self.filter_vault.clone(),
                status,
            },
        )
        .unwrap_or_default();
        let ids: Vec<String> = self.tasks.iter().map(|t| t.id.clone()).collect();
        self.focus_totals = repo::focus_totals(&conn, &ids).unwrap_or_default();
        cx.notify();
    }

    fn quick_add_task(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let title = self.quick_add.read(cx).value().to_string();
        let title = title.trim().to_string();
        if title.is_empty() {
            return;
        }
        let vault_id = self
            .filter_vault
            .clone()
            .or_else(|| self.vaults.first().map(|v| v.id.clone()));
        let Some(vault_id) = vault_id else { return };
        if let Ok(conn) = self.db.connect() {
            let _ = repo::create_task(
                &conn,
                NewTask {
                    vault_id,
                    title,
                    note: None,
                    priority: Priority::None,
                    due_at: None,
                    tag_names: vec![],
                },
            );
        }
        self.quick_add
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.reload(cx);
    }

    pub(crate) fn show_toast(&mut self, msg: String, kind: ToastKind, cx: &mut Context<Self>) {
        self.toast = Some((msg, kind));
        cx.notify();
        self._toast_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(3)).await;
            let _ = this.update(cx, |this, cx| {
                this.toast = None;
                cx.notify();
            });
        }));
    }

    pub(crate) fn close_overlay(&mut self, cx: &mut Context<Self>) {
        self.overlay = None;
        cx.notify();
    }

    // ------------------------------------------------------------------
    // 覆盖层打开入口
    // ------------------------------------------------------------------

    pub(crate) fn open_task_editor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        task_id: Option<String>,
    ) {
        let existing = task_id.as_ref().and_then(|id| {
            self.db
                .connect()
                .ok()
                .and_then(|conn| repo::get_task(&conn, id).ok())
        });
        let (title_v, note_v, tags_v, due_v, priority, vault_id) = match &existing {
            Some(t) => (
                t.title.clone(),
                t.note.clone().unwrap_or_default(),
                t.tags
                    .iter()
                    .map(|g| g.name.clone())
                    .collect::<Vec<_>>()
                    .join(", "),
                t.due_at.map(format_date).unwrap_or_default(),
                t.priority,
                t.vault_id.clone(),
            ),
            None => (
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                Priority::None,
                self.filter_vault
                    .clone()
                    .or_else(|| self.vaults.first().map(|v| v.id.clone()))
                    .unwrap_or_default(),
            ),
        };
        let title = cx.new(|cx| InputState::new(window, cx).placeholder("任务标题"));
        title.update(cx, |s, cx| s.set_value(title_v, window, cx));
        let note = cx.new(|cx| InputState::new(window, cx).placeholder("备注（可选）"));
        note.update(cx, |s, cx| s.set_value(note_v, window, cx));
        let tags = cx.new(|cx| InputState::new(window, cx).placeholder("标签，用逗号分隔（可选）"));
        tags.update(cx, |s, cx| s.set_value(tags_v, window, cx));
        let due =
            cx.new(|cx| InputState::new(window, cx).placeholder("截止日期 YYYY-MM-DD（可选）"));
        due.update(cx, |s, cx| s.set_value(due_v, window, cx));

        self.overlay = Some(Overlay::TaskEditor {
            task_id,
            title,
            note,
            tags,
            due,
            priority,
            vault_id,
        });
        cx.notify();
    }

    pub(crate) fn open_vault_editor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        vault_id: Option<String>,
    ) {
        let name_v = vault_id
            .as_ref()
            .and_then(|id| self.vaults.iter().find(|v| &v.id == id))
            .map(|v| v.name.clone())
            .unwrap_or_default();
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("清单名称"));
        name.update(cx, |s, cx| s.set_value(name_v, window, cx));
        self.overlay = Some(Overlay::VaultEditor { vault_id, name });
        cx.notify();
    }

    pub(crate) fn open_export(&mut self, cx: &mut Context<Self>) {
        let vault_id = self
            .filter_vault
            .clone()
            .or_else(|| self.vaults.first().map(|v| v.id.clone()));
        self.overlay = Some(Overlay::Export {
            vault_id,
            message: None,
        });
        cx.notify();
    }

    pub(crate) fn open_confirm_delete_task(
        &mut self,
        cx: &mut Context<Self>,
        id: String,
        title: String,
    ) {
        self.overlay = Some(Overlay::ConfirmDeleteTask { id, title });
        cx.notify();
    }

    pub(crate) fn open_confirm_delete_vault(
        &mut self,
        cx: &mut Context<Self>,
        id: String,
        name: String,
    ) {
        self.overlay = Some(Overlay::ConfirmDeleteVault { id, name });
        cx.notify();
    }

    // ------------------------------------------------------------------
    // 覆盖层动作
    // ------------------------------------------------------------------

    pub(crate) fn save_task_editor(&mut self, cx: &mut Context<Self>) {
        let Some(Overlay::TaskEditor {
            task_id,
            title,
            note,
            tags,
            due,
            priority,
            vault_id,
        }) = &self.overlay
        else {
            return;
        };
        let title_text = title.read(cx).value().to_string().trim().to_string();
        let note_text = note.read(cx).value().to_string().trim().to_string();
        let tags_text = tags.read(cx).value().to_string();
        let due_text = due.read(cx).value().to_string();
        let priority = *priority;
        let vault_id = vault_id.clone();
        let task_id = task_id.clone();

        if title_text.is_empty() {
            self.show_toast("标题不能为空".to_string(), ToastKind::Error, cx);
            return;
        }
        let due_at = match parse_due(&due_text) {
            Ok(v) => v,
            Err(msg) => {
                self.show_toast(msg, ToastKind::Error, cx);
                return;
            }
        };
        let tag_names: Vec<String> = tags_text
            .split([',', '，'])
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let Ok(conn) = self.db.connect() else { return };
        let result = match task_id {
            Some(id) => repo::update_task_details(
                &conn,
                &id,
                &title_text,
                if note_text.is_empty() {
                    None
                } else {
                    Some(note_text.as_str())
                },
                priority,
                due_at,
                &vault_id,
                &tag_names,
            )
            .map(|_| ()),
            None => repo::create_task(
                &conn,
                NewTask {
                    vault_id,
                    title: title_text,
                    note: if note_text.is_empty() {
                        None
                    } else {
                        Some(note_text)
                    },
                    priority,
                    due_at,
                    tag_names,
                },
            )
            .map(|_| ()),
        };
        match result {
            Ok(()) => {
                self.overlay = None;
                self.reload(cx);
            }
            Err(e) => self.show_toast(format!("保存失败: {e}"), ToastKind::Error, cx),
        }
    }

    pub(crate) fn save_vault_editor(&mut self, cx: &mut Context<Self>) {
        let Some(Overlay::VaultEditor { vault_id, name }) = &self.overlay else {
            return;
        };
        let name_text = name.read(cx).value().to_string().trim().to_string();
        let vault_id = vault_id.clone();
        if name_text.is_empty() {
            self.show_toast("名称不能为空".to_string(), ToastKind::Error, cx);
            return;
        }
        let Ok(conn) = self.db.connect() else { return };
        let result = match &vault_id {
            Some(id) => repo::rename_vault(&conn, id, &name_text),
            None => repo::create_vault(&conn, &name_text, None).map(|_| ()),
        };
        match result {
            Ok(()) => {
                self.overlay = None;
                self.reload(cx);
            }
            Err(e) => self.show_toast(format!("保存失败: {e}"), ToastKind::Error, cx),
        }
    }

    pub(crate) fn do_export(&mut self, cx: &mut Context<Self>) {
        let Some(Overlay::Export { vault_id, .. }) = &self.overlay else {
            return;
        };
        let Some(vault_id) = vault_id.clone() else {
            self.set_export_message("请先选择要导出的清单".to_string(), true, cx);
            return;
        };
        let vault_name = self
            .vaults
            .iter()
            .find(|v| v.id == vault_id)
            .map(|v| v.name.clone())
            .unwrap_or_else(|| "export".to_string());
        let default_name = format!("ticktick-{}-已完成.json", vault_name);
        let path = rfd::FileDialog::new()
            .set_title("导出已完成任务为 JSON")
            .set_file_name(&default_name)
            .add_filter("JSON", &["json"])
            .save_file();
        let Some(path) = path else { return };
        let result =
            self.db
                .connect()
                .map_err(|e| e.to_string())
                .and_then(|conn| -> Result<(), String> {
                    let json = ticktick_core::export::export_vault_completed_json(&conn, &vault_id)
                        .map_err(|e| e.to_string())?;
                    std::fs::write(&path, json).map_err(|e| e.to_string())
                });
        match result {
            Ok(()) => self.set_export_message(format!("已导出到 {}", path.display()), false, cx),
            Err(e) => self.set_export_message(format!("导出失败: {e}"), true, cx),
        }
    }

    fn set_export_message(&mut self, msg: String, is_error: bool, cx: &mut Context<Self>) {
        if let Some(Overlay::Export { message, .. }) = &mut self.overlay {
            *message = Some((msg, is_error));
        }
        cx.notify();
    }

    pub(crate) fn confirm_delete_task(&mut self, cx: &mut Context<Self>) {
        let Some(Overlay::ConfirmDeleteTask { id, .. }) = &self.overlay else {
            return;
        };
        let id = id.clone();
        if let Ok(conn) = self.db.connect() {
            let _ = repo::delete_task(&conn, &id);
        }
        self.overlay = None;
        self.reload(cx);
    }

    pub(crate) fn confirm_delete_vault(&mut self, cx: &mut Context<Self>) {
        let Some(Overlay::ConfirmDeleteVault { id, .. }) = &self.overlay else {
            return;
        };
        let id = id.clone();
        if self.vaults.len() <= 1 {
            self.overlay = None;
            self.show_toast("至少保留一个清单".to_string(), ToastKind::Error, cx);
            return;
        }
        if let Ok(conn) = self.db.connect() {
            let _ = repo::delete_vault(&conn, &id);
        }
        if self.filter_vault.as_deref() == Some(id.as_str()) {
            self.filter_vault = None;
        }
        self.overlay = None;
        self.reload(cx);
    }

    pub(crate) fn set_overlay_priority(&mut self, p: Priority, cx: &mut Context<Self>) {
        if let Some(Overlay::TaskEditor { priority, .. }) = &mut self.overlay {
            *priority = p;
        }
        cx.notify();
    }

    pub(crate) fn set_overlay_vault(&mut self, vault_id: String, cx: &mut Context<Self>) {
        match &mut self.overlay {
            Some(Overlay::TaskEditor { vault_id: v, .. }) => *v = vault_id,
            Some(Overlay::Export { vault_id: v, .. }) => *v = Some(vault_id),
            _ => {}
        }
        cx.notify();
    }

    // ------------------------------------------------------------------
    // 主区域
    // ------------------------------------------------------------------

    fn render_main(&mut self, cx: &mut Context<Self>) -> Div {
        let total = self.tasks.len();
        let toggle_label = if self.show_completed {
            "隐藏已完成"
        } else {
            "显示已完成"
        };

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .child(
                div()
                    .p_2()
                    .border_b_1()
                    .border_color(tx::rgba(tx::BORDER))
                    .child(Input::new(&self.quick_add).w_full()),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .px_3()
                    .py_1()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .text_xs()
                            .text_color(tx::rgba(tx::TEXT_MUTED))
                            .child(format!("{total} 项任务")),
                    )
                    .child(
                        div()
                            .id("toggle-completed")
                            .cursor_pointer()
                            .text_xs()
                            .text_color(tx::rgba(tx::ACCENT))
                            .hover(|s| s.text_color(tx::rgba(tx::ACCENT_HOVER)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_completed = !this.show_completed;
                                this.reload(cx);
                            }))
                            .child(toggle_label),
                    )
                    .child(
                        div()
                            .id("open-export")
                            .cursor_pointer()
                            .text_xs()
                            .text_color(tx::rgba(tx::ACCENT))
                            .hover(|s| s.text_color(tx::rgba(tx::ACCENT_HOVER)))
                            .on_click(cx.listener(|this, _, _, cx| this.open_export(cx)))
                            .child("导出 JSON"),
                    ),
            )
            .child(task_list::render_task_list(self, cx))
    }
}

impl Render for RootView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 清单切换栏在上，任务区在下（小窗宽度有限，不用侧栏）
        let body = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(sidebar::render_vault_bar(self, cx))
            .child(self.render_main(cx));

        let mut root = div()
            .size_full()
            .flex()
            .flex_col()
            .relative()
            .bg(tx::rgba(tx::BG))
            .text_color(tx::rgba(tx::TEXT))
            .child(titlebar::render_titlebar(self, window, cx));

        if let Some((msg, kind)) = &self.toast {
            let color = match kind {
                ToastKind::Info => tx::SUCCESS,
                ToastKind::Error => tx::DANGER,
            };
            root = root.child(
                div()
                    .px_3()
                    .py_1()
                    .bg(tx::rgba(color))
                    .text_xs()
                    .text_color(rgb(0xFFFFFF))
                    .child(msg.clone()),
            );
        }

        root = root.child(body);

        #[cfg(feature = "pomodoro")]
        {
            root = root.child(self.pomodoro.clone());
        }

        if self.overlay.is_some() {
            root = root.child(overlays::render_overlay(self, window, cx));
        }

        root
    }
}

// ---------------------------------------------------------------------------
// 日期工具
// ---------------------------------------------------------------------------

pub(crate) fn format_date(ts: i64) -> String {
    let utc =
        time::OffsetDateTime::from_unix_timestamp(ts).unwrap_or(time::OffsetDateTime::UNIX_EPOCH);
    let local = time::UtcOffset::current_local_offset()
        .map(|off| utc.to_offset(off))
        .unwrap_or(utc);
    format!(
        "{:04}-{:02}-{:02}",
        local.year(),
        local.month() as u8,
        local.day()
    )
}

pub(crate) fn format_due_short(ts: i64) -> String {
    let utc =
        time::OffsetDateTime::from_unix_timestamp(ts).unwrap_or(time::OffsetDateTime::UNIX_EPOCH);
    let local = time::UtcOffset::current_local_offset()
        .map(|off| utc.to_offset(off))
        .unwrap_or(utc);
    format!("{:02}-{:02}", local.month() as u8, local.day())
}

pub(crate) fn is_overdue(due_at: i64) -> bool {
    due_at < now_ts()
}

fn parse_due(s: &str) -> Result<Option<i64>, String> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(None);
    }
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 3 {
        return Err("日期格式应为 YYYY-MM-DD".to_string());
    }
    let year: i32 = parts[0]
        .trim()
        .parse()
        .map_err(|_| "年份无效".to_string())?;
    let month: u8 = parts[1]
        .trim()
        .parse()
        .map_err(|_| "月份无效".to_string())?;
    let day: u8 = parts[2]
        .trim()
        .parse()
        .map_err(|_| "日期无效".to_string())?;
    let month = time::Month::try_from(month).map_err(|_| "月份应在 1-12 之间".to_string())?;
    let date =
        time::Date::from_calendar_date(year, month, day).map_err(|_| "日期不存在".to_string())?;
    let offset = time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC);
    let dt = date
        .with_hms(23, 59, 59)
        .map_err(|e| e.to_string())?
        .assume_offset(offset);
    Ok(Some(dt.unix_timestamp()))
}
