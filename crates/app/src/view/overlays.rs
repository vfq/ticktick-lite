//! 覆盖层：任务编辑器 / 清单编辑 / JSON 导出 / 删除确认。

use gpui_kit::component::input::{Input, InputState};
use gpui_kit::prelude::*;
use gpui_kit::*;
use ticktick_core::Priority;

use crate::theme_ext as tx;

use super::RootView;

pub enum Overlay {
    TaskEditor {
        task_id: Option<String>,
        title: Entity<InputState>,
        note: Entity<InputState>,
        /// 新建标签的输入框（回车加入已选）
        tag_input: Entity<InputState>,
        /// 已选标签名（覆盖式写回任务）
        selected_tags: Vec<String>,
        due: Entity<InputState>,
        priority: Priority,
        vault_id: String,
    },
    VaultEditor {
        vault_id: Option<String>,
        name: Entity<InputState>,
    },
    Export {
        vault_id: Option<String>,
        /// (消息, 是否错误)
        message: Option<(String, bool)>,
    },
    ConfirmDeleteTask {
        id: String,
        title: String,
    },
    ConfirmDeleteVault {
        id: String,
        name: String,
    },
}

pub fn render_overlay(view: &RootView, _window: &mut Window, cx: &mut Context<RootView>) -> Div {
    let content = match &view.overlay {
        Some(Overlay::TaskEditor { .. }) => render_task_editor(view, cx),
        Some(Overlay::VaultEditor { .. }) => render_vault_editor(view, cx),
        Some(Overlay::Export { .. }) => render_export(view, cx),
        Some(Overlay::ConfirmDeleteTask { title, .. }) => render_confirm(
            cx,
            "删除任务",
            format!("确定删除任务「{title}」吗？此操作不可撤销。"),
            true,
        ),
        Some(Overlay::ConfirmDeleteVault { name, .. }) => render_confirm(
            cx,
            "删除清单",
            format!("确定删除清单「{name}」吗？其中的任务将一并删除。"),
            false,
        ),
        None => div(),
    };

    div()
        .absolute()
        .inset_0()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(rgba(0x00000066))
        .child(content)
}

// ---------------------------------------------------------------------------
// 任务编辑器
// ---------------------------------------------------------------------------

fn render_task_editor(view: &RootView, cx: &mut Context<RootView>) -> Div {
    let Some(Overlay::TaskEditor {
        task_id,
        title,
        note,
        due,
        priority,
        vault_id,
        ..
    }) = &view.overlay
    else {
        return div();
    };

    let heading = if task_id.is_some() {
        "编辑任务"
    } else {
        "新建任务"
    };

    // 优先级选择
    let mut prio_row = div().flex().flex_row().gap_1().flex_wrap();
    for p in Priority::all() {
        let selected = *priority == p;
        let chip = div()
            .id(SharedString::from(format!("prio-{}", p.as_str())))
            .cursor_pointer()
            .px_2()
            .py_1()
            .rounded_md()
            .text_xs()
            .border_1()
            .border_color(tx::rgba(tx::BORDER))
            .when(selected, |el| {
                el.bg(tx::priority_color(p))
                    .text_color(rgb(0xFFFFFF))
                    .border_color(tx::priority_color(p))
            })
            .when(!selected, |el| el.text_color(tx::rgba(tx::TEXT_MUTED)))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.set_overlay_priority(p, cx);
            }))
            .child(p.label().to_string());
        prio_row = prio_row.child(chip);
    }

    panel_base(heading)
        .child(field_label("标题"))
        .child(Input::new(title).w_full())
        .child(field_label("备注"))
        .child(Input::new(note).w_full())
        .child(field_label("标签"))
        .child(tag_picker(view, cx))
        .child(field_label("截止日期"))
        .child(Input::new(due).w_full())
        .child(field_label("优先级"))
        .child(prio_row)
        .child(field_label("所属清单"))
        .child(vault_chips(view, Some(vault_id.as_str()), cx))
        .child(footer(
            cx.listener(|this, _, _, cx| this.close_overlay(cx)),
            cx.listener(|this, _, _, cx| this.save_task_editor(cx)),
            "保存",
        ))
}

/// 标签选择器：已选 chip（点击移除）+ 可选 chip（点击加入）+ 新建输入框。
///
/// 用 chip 多选替代原来的「逗号分隔文本框」——用户不必记住标签名，
/// 也不会因为多打一个逗号造出重复标签。
fn tag_picker(view: &RootView, cx: &mut Context<RootView>) -> Div {
    let Some(Overlay::TaskEditor {
        tag_input,
        selected_tags,
        ..
    }) = &view.overlay
    else {
        return div();
    };
    let selected = selected_tags.clone();
    let input = tag_input.clone();

    let mut selected_row = div().flex().flex_row().flex_wrap().gap_1().min_h(px(22.0));
    if selected.is_empty() {
        selected_row = selected_row.child(
            div()
                .text_xs()
                .text_color(tx::rgba(tx::TEXT_FAINT))
                .child("未选择标签"),
        );
    }
    for name in &selected {
        let hex = view
            .all_tags
            .iter()
            .find(|t| &t.name == name)
            .map(|t| t.color_hex())
            .unwrap_or(tx::TEXT_MUTED);
        let (bg, fg, border) = tx::tag_chip_colors(hex);
        let owned = name.clone();
        selected_row = selected_row.child(
            div()
                .id(SharedString::from(format!("sel-tag-{name}")))
                .cursor_pointer()
                .px_2()
                .py_1()
                .rounded_full()
                .bg(bg)
                .border_1()
                .border_color(border)
                .text_xs()
                .text_color(fg)
                .on_click(
                    cx.listener(move |this, _, _, cx| this.toggle_editor_tag(owned.clone(), cx)),
                )
                .child(format!("{name} ✕")),
        );
    }

    let mut avail_row = div().flex().flex_row().flex_wrap().gap_1();
    for tag in &view.all_tags {
        if selected.iter().any(|n| n == &tag.name) {
            continue;
        }
        let (bg, fg, border) = tx::tag_chip_colors(tag.color_hex());
        let owned = tag.name.clone();
        avail_row = avail_row.child(
            div()
                .id(SharedString::from(format!("opt-tag-{}", tag.id)))
                .cursor_pointer()
                .px_2()
                .py_1()
                .rounded_full()
                .bg(tx::rgba(tx::BG_CHIP))
                .border_1()
                .border_color(tx::rgba(tx::BORDER))
                .text_xs()
                .text_color(tx::rgba(tx::TEXT_MUTED))
                .hover(move |s| s.bg(bg).text_color(fg).border_color(border))
                .on_click(
                    cx.listener(move |this, _, _, cx| this.toggle_editor_tag(owned.clone(), cx)),
                )
                .child(tag.name.clone()),
        );
    }

    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(selected_row)
        .child(avail_row)
        .child(Input::new(&input).w_full())
}

// ---------------------------------------------------------------------------
// 清单编辑
// ---------------------------------------------------------------------------

fn render_vault_editor(view: &RootView, cx: &mut Context<RootView>) -> Div {
    let Some(Overlay::VaultEditor { vault_id, name }) = &view.overlay else {
        return div();
    };
    let heading = if vault_id.is_some() {
        "重命名清单"
    } else {
        "新建清单"
    };
    panel_base(heading)
        .child(field_label("名称"))
        .child(Input::new(name).w_full())
        .child(footer(
            cx.listener(|this, _, _, cx| this.close_overlay(cx)),
            cx.listener(|this, _, _, cx| this.save_vault_editor(cx)),
            "保存",
        ))
}

// ---------------------------------------------------------------------------
// JSON 导出
// ---------------------------------------------------------------------------

fn render_export(view: &RootView, cx: &mut Context<RootView>) -> Div {
    let Some(Overlay::Export {
        vault_id, message, ..
    }) = &view.overlay
    else {
        return div();
    };

    let mut panel = panel_base("导出已完成任务")
        .child(
            div()
                .text_xs()
                .text_color(tx::rgba(tx::TEXT_MUTED))
                .child("选择清单，将其全部已完成任务（含标签与番茄钟记录）导出为 JSON。"),
        )
        .child(field_label("选择清单"))
        .child(vault_chips(view, vault_id.as_deref(), cx));

    if let Some((msg, is_error)) = message {
        panel = panel.child(
            div()
                .text_xs()
                .text_color(tx::rgba(if *is_error { tx::DANGER } else { tx::SUCCESS }))
                .child(msg.clone()),
        );
    }

    panel.child(footer(
        cx.listener(|this, _, _, cx| this.close_overlay(cx)),
        cx.listener(|this, _, _, cx| this.do_export(cx)),
        "选择位置并导出",
    ))
}

// ---------------------------------------------------------------------------
// 删除确认
// ---------------------------------------------------------------------------

fn render_confirm(cx: &mut Context<RootView>, title: &str, body: String, is_task: bool) -> Div {
    let on_confirm: Box<dyn Fn(&ClickEvent, &mut Window, &mut App)> = if is_task {
        Box::new(cx.listener(|this, _, _, cx| this.confirm_delete_task(cx)))
    } else {
        Box::new(cx.listener(|this, _, _, cx| this.confirm_delete_vault(cx)))
    };

    panel_base(title)
        .child(
            div()
                .text_sm()
                .text_color(tx::rgba(tx::TEXT_MUTED))
                .child(body),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .justify_end()
                .gap_2()
                .mt_2()
                .child(
                    overlay_button("confirm-cancel", "取消", BtnStyle::Secondary)
                        .on_click(cx.listener(|this, _, _, cx| this.close_overlay(cx))),
                )
                .child(overlay_button("confirm-ok", "删除", BtnStyle::Danger).on_click(on_confirm)),
        )
}

// ---------------------------------------------------------------------------
// 共用构件
// ---------------------------------------------------------------------------

enum BtnStyle {
    Primary,
    Secondary,
    Danger,
}

fn overlay_button(id: &'static str, label: &'static str, style: BtnStyle) -> Stateful<Div> {
    let base = div()
        .id(id)
        .cursor_pointer()
        .px_3()
        .py_1()
        .rounded(px(tx::RADIUS_CONTROL))
        .text_sm();
    match style {
        BtnStyle::Primary => base
            .bg(tx::rgba(tx::ACCENT))
            .text_color(rgb(0xFFFFFF))
            .hover(|s| s.bg(tx::rgba(tx::ACCENT_HOVER)))
            .child(label),
        BtnStyle::Danger => base
            .bg(tx::rgba(tx::DANGER))
            .text_color(rgb(0xFFFFFF))
            .hover(|s| s.bg(tx::rgba(tx::DANGER_HOVER)))
            .child(label),
        BtnStyle::Secondary => base
            .bg(tx::rgba(tx::BG_BTN))
            .text_color(tx::rgba(tx::TEXT))
            .hover(|s| s.bg(tx::rgba(tx::BG_BTN_HOVER)))
            .child(label),
    }
}

fn footer(
    on_cancel: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    on_ok: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ok_label: &'static str,
) -> Div {
    div()
        .flex()
        .flex_row()
        .justify_end()
        .gap_2()
        .mt_2()
        .child(overlay_button("overlay-cancel", "取消", BtnStyle::Secondary).on_click(on_cancel))
        .child(overlay_button("overlay-ok", ok_label, BtnStyle::Primary).on_click(on_ok))
}

fn panel_base(title: &str) -> Div {
    div()
        .w(px(300.0))
        .flex()
        .flex_col()
        .gap_2()
        .p_4()
        .rounded(px(tx::RADIUS_CARD))
        .bg(rgb(tx::BG))
        .border_1()
        .border_color(tx::rgba(tx::BORDER))
        .shadow_lg()
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tx::rgba(tx::TEXT))
                .child(title.to_string()),
        )
}

fn field_label(text: &str) -> Div {
    div()
        .text_xs()
        .text_color(tx::rgba(tx::TEXT_MUTED))
        .child(text.to_string())
}

fn vault_chips(view: &RootView, selected: Option<&str>, cx: &mut Context<RootView>) -> Div {
    let mut row = div().flex().flex_row().gap_1().flex_wrap();
    for vault in &view.vaults {
        let id = vault.id.clone();
        let is_selected = selected == Some(vault.id.as_str());
        row = row.child(
            div()
                .id(SharedString::from(format!("vault-chip-{}", vault.id)))
                .cursor_pointer()
                .px_2()
                .py_1()
                .rounded(px(tx::RADIUS_CONTROL))
                .text_xs()
                .border_1()
                .border_color(tx::rgba(if is_selected { tx::ACCENT } else { tx::BORDER }))
                .when(is_selected, |el| {
                    el.bg(tx::rgba(tx::BG_ACTIVE))
                        .text_color(tx::rgba(tx::TEXT))
                })
                .when(!is_selected, |el| el.text_color(tx::rgba(tx::TEXT_MUTED)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.set_overlay_vault(id.clone(), cx);
                }))
                .child(vault.name.clone()),
        );
    }
    row
}
