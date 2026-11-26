//! 清单切换栏：横向 chip 行（全部 + 各清单 + 新建），右侧为当前清单的编辑/删除。
//!
//! 小窗宽度仅 380px，侧栏会挤占任务区，因此改为顶部横向 chip 行，
//! 与 TickTick 小部件的心智模型一致；清单增删走覆盖层，不进入二级页面。

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::theme_ext as tx;

use super::RootView;

pub fn render_vault_bar(view: &RootView, cx: &mut Context<RootView>) -> Div {
    // 「全部」
    let all_selected = view.filter_vault.is_none();
    let mut chips = div()
        .id("vault-bar")
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .px_3()
        .h(px(tx::CHIP_ROW_H))
        .flex_1()
        .min_w_0()
        .overflow_x_scroll()
        .child(vault_chip(
            "vault-all",
            "全部",
            Some(view.total_active),
            all_selected,
            cx.listener(|this, _, _, cx| {
                this.filter_vault = None;
                this.reload(cx);
            }),
        ));

    for vault in &view.vaults {
        let id = vault.id.clone();
        let selected = view.filter_vault.as_deref() == Some(vault.id.as_str());
        let count = view.vault_counts.get(&vault.id).copied().unwrap_or(0);
        chips = chips.child(vault_chip(
            SharedString::from(format!("vault-{}", vault.id)),
            &vault.name,
            Some(count),
            selected,
            cx.listener(move |this: &mut RootView, _, _, cx| {
                this.filter_vault = Some(id.clone());
                this.reload(cx);
            }),
        ));
    }

    // 新建清单
    let add = div()
        .id("vault-new")
        .cursor_pointer()
        .px_2()
        .py_1()
        .rounded_full()
        .text_xs()
        .text_color(tx::rgba(tx::TEXT_MUTED))
        .hover(|s| s.bg(tx::rgba(tx::BG_HOVER)).text_color(tx::rgba(tx::TEXT)))
        .on_click(cx.listener(|this, _, window, cx| {
            this.open_vault_editor(window, cx, None);
        }))
        .child("＋");

    // 当前清单的编辑 / 删除
    let actions = match view.filter_vault.clone() {
        Some(id) => {
            let edit_id = id.clone();
            let del_id = id.clone();
            let del_name = view
                .vaults
                .iter()
                .find(|v| v.id == id)
                .map(|v| v.name.clone())
                .unwrap_or_default();
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .id("vault-edit")
                        .cursor_pointer()
                        .px_1()
                        .text_xs()
                        .text_color(tx::rgba(tx::TEXT_FAINT))
                        .hover(|s| s.text_color(tx::rgba(tx::ACCENT)))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_vault_editor(window, cx, Some(edit_id.clone()));
                        }))
                        .child("✎"),
                )
                .child(
                    div()
                        .id("vault-delete")
                        .cursor_pointer()
                        .px_1()
                        .text_xs()
                        .text_color(tx::rgba(tx::TEXT_FAINT))
                        .hover(|s| s.text_color(tx::rgba(tx::DANGER)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open_confirm_delete_vault(cx, del_id.clone(), del_name.clone());
                        }))
                        .child("✕"),
                )
        }
        None => div(),
    };

    div()
        .w_full()
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .px_3()
        .py_1()
        .border_b_1()
        .border_color(tx::rgba(tx::BORDER))
        .bg(tx::rgba(tx::BG_SUB))
        .child(chips)
        .child(add)
        .child(actions)
}

fn vault_chip(
    id: impl Into<ElementId>,
    label: &str,
    count: Option<i64>,
    selected: bool,
    on_click: impl Fn(&gpui_kit::ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let (bg, fg, border) = if selected {
        (tx::BG_ACTIVE, tx::ACCENT_HOVER, 0x378ADD)
    } else {
        (tx::BG_SUB, tx::TEXT_MUTED, tx::BORDER)
    };
    let mut chip = div()
        .id(id)
        .cursor_pointer()
        .px_2()
        .py_1()
        .rounded_full()
        .border_1()
        .border_color(rgb(border))
        .bg(rgb(bg))
        .when(selected, |el| el.border_color(rgb(0x85B7EB)))
        .hover(|s| s.bg(tx::rgba(tx::BG_HOVER)))
        .on_click(on_click)
        .child(
            div()
                .text_xs()
                .text_color(tx::rgba(fg))
                .overflow_hidden()
                .child(label.to_string()),
        );
    if let Some(count) = count {
        chip = chip.child(
            div()
                .text_xs()
                .text_color(tx::rgba(if selected { tx::ACCENT } else { tx::TEXT_FAINT }))
                .child(format!(" {count}")),
        );
    }
    chip
}
