//! 任务列表：勾选完成、优先级色条、标签、截止日期、专注时长、行内操作。

use gpui_kit::prelude::*;
use gpui_kit::*;
use ticktick_core::{repo, Task as CoreTask, TaskStatus};

use crate::theme_ext as tx;

use super::{format_due_short, is_overdue, RootView};

pub fn render_task_list(view: &RootView, cx: &mut Context<RootView>) -> Stateful<Div> {
    if view.tasks.is_empty() {
        return div()
            .id("task-scroll")
            .flex_1()
            .min_h_0()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .text_sm()
                    .text_color(tx::rgba(tx::TEXT_FAINT))
                    .child("暂无任务，在上方输入框回车快速添加"),
            );
    }

    let mut list = div()
        .id("task-scroll")
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .p_2()
        .gap_1();

    // 分组标题：优先级分组在前，已完成在后（与优先级色条形成双编码）
    let mut current_group: Option<String> = None;
    for task in &view.tasks {
        let group = if task.is_completed() {
            "已完成".to_string()
        } else {
            format!("{}优先级", task.priority.label())
        };
        if current_group.as_deref() != Some(group.as_str()) {
            list = list.child(
                div()
                    .pt_1()
                    .pb(px(2.0))
                    .text_xs()
                    .text_color(tx::rgba(tx::TEXT_FAINT))
                    .child(group.clone()),
            );
            current_group = Some(group);
        }
        let focus = view.focus_totals.get(&task.id).copied().unwrap_or(0);
        list = list.child(render_task_row(task, focus, cx));
    }
    list
}

fn render_task_row(task: &CoreTask, focus_secs: i64, cx: &mut Context<RootView>) -> Stateful<Div> {
    let done = task.is_completed();

    // ---- 勾选框 ----
    let toggle_id = task.id.clone();
    let checkbox = div()
        .id(SharedString::from(format!("check-{}", task.id)))
        .w_4()
        .h_4()
        .rounded_sm()
        .flex()
        .items_center()
        .justify_center()
        .border_1()
        .border_color(tx::rgba(if done { tx::ACCENT } else { tx::BORDER }))
        .cursor_pointer()
        .when(done, |el| {
            el.bg(tx::rgba(tx::ACCENT))
                .child(div().text_xs().text_color(rgb(0xFFFFFF)).child("✓"))
        })
        .on_click(cx.listener(move |this, _, _, cx| {
            let new_status = if done {
                TaskStatus::Active
            } else {
                TaskStatus::Completed
            };
            if let Ok(conn) = this.db.connect() {
                let _ = repo::set_task_status(&conn, &toggle_id, new_status);
            }
            this.reload(cx);
        }));

    // ---- 标题 ----
    let title_color = if done { tx::TEXT_FAINT } else { tx::TEXT };
    let mut title_el = div()
        .flex_1()
        .text_sm()
        .text_color(tx::rgba(title_color))
        .overflow_hidden()
        .child(task.title.clone());
    if done {
        title_el = title_el.line_through();
    }

    // ---- 优先级圆点 ----
    let prio = task.priority;
    let prio_dot = div()
        .w_2()
        .h_2()
        .rounded_full()
        .bg(tx::priority_color(prio));

    // ---- 行内操作 ----
    let edit_id = task.id.clone();
    let del_id = task.id.clone();
    let del_title = task.title.clone();
    let mut actions = div().flex().flex_row().items_center().gap_1();

    #[cfg(feature = "pomodoro")]
    if !done {
        let pid = task.id.clone();
        let ptitle = task.title.clone();
        actions = actions.child(
            action_button(SharedString::from(format!("focus-{}", task.id)), "▶").on_click(
                cx.listener(move |this, _, _, cx| {
                    this.pomodoro.update(cx, |p, cx| {
                        p.start_for_task(pid.clone(), ptitle.clone(), cx)
                    });
                    cx.notify();
                }),
            ),
        );
    }

    actions = actions
        .child(
            action_button(SharedString::from(format!("edit-{}", task.id)), "✎").on_click(
                cx.listener(move |this, _, window, cx| {
                    this.open_task_editor(window, cx, Some(edit_id.clone()));
                }),
            ),
        )
        .child(
            action_button(SharedString::from(format!("del-{}", task.id)), "✕").on_click(
                cx.listener(move |this, _, _, cx| {
                    this.open_confirm_delete_task(cx, del_id.clone(), del_title.clone());
                }),
            ),
        );

    // ---- 元信息行（标签 / 截止 / 专注） ----
    let mut meta = div().flex().flex_row().items_center().gap_1().flex_wrap();
    let mut has_meta = false;
    for tag in &task.tags {
        has_meta = true;
        meta = meta.child(
            div()
                .px_1()
                .rounded_sm()
                .bg(tx::tag_chip_color())
                .text_xs()
                .text_color(tx::rgba(tx::TEXT_MUTED))
                .child(tag.name.clone()),
        );
    }
    if let Some(due) = task.due_at {
        has_meta = true;
        let overdue = !done && is_overdue(due);
        meta = meta.child(
            div()
                .text_xs()
                .text_color(tx::rgba(if overdue { tx::OVERDUE } else { tx::TEXT_FAINT }))
                .child(format!("📅 {}", format_due_short(due))),
        );
    }
    if focus_secs > 0 {
        has_meta = true;
        meta = meta.child(
            div()
                .text_xs()
                .text_color(tx::rgba(tx::FOCUS))
                .child(format!("🍅 {}m", focus_secs / 60)),
        );
    }

    let border_color = if prio == ticktick_core::Priority::None {
        tx::rgba(tx::BORDER)
    } else {
        tx::priority_color(prio)
    };

    let mut row = div()
        .id(SharedString::from(format!("task-{}", task.id)))
        .flex()
        .flex_col()
        .gap(px(2.0))
        .px_2()
        .py_1()
        .rounded_md()
        .border_l_2()
        .border_color(border_color)
        .hover(|s| s.bg(tx::rgba(tx::BG_HOVER)))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(checkbox)
                .child(prio_dot)
                .child(title_el)
                .child(actions),
        );
    if has_meta {
        row = row.child(meta.pl_6());
    }
    row
}

fn action_button(id: SharedString, label: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .cursor_pointer()
        .px_1()
        .rounded_sm()
        .text_xs()
        .text_color(tx::rgba(tx::TEXT_FAINT))
        .hover(|s| s.bg(tx::rgba(tx::BG_ACTIVE)).text_color(tx::rgba(tx::TEXT)))
        .child(label)
}
