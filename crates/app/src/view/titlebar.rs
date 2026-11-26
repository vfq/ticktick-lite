//! 自绘标题栏：拖动区 + 置顶 / 最小化到托盘 / 关闭（隐藏到托盘）。

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::platform;
use crate::theme_ext as tx;

use super::RootView;

pub fn render_titlebar(view: &RootView, _window: &mut Window, cx: &mut Context<RootView>) -> Div {
    let pin_color = if view.topmost {
        tx::ACCENT
    } else {
        tx::TEXT_FAINT
    };

    div()
        .h(px(tx::TITLEBAR_H))
        .w_full()
        .flex()
        .flex_row()
        .items_center()
        .border_b_1()
        .border_color(tx::rgba(tx::BORDER))
        .bg(tx::rgba(tx::BG_SUB))
        .child(
            // 拖动区：用 window_control_area 走平台的 WM_NCHITTEST → HTCAPTION。
            // 不能用 window.start_window_move()——gpui-pre 在 Windows 后端把该方法
            // 实现成了空函数（BUG-0001）。
            div()
                .id("titlebar-drag")
                .flex_1()
                .h_full()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .pl_3()
                .window_control_area(WindowControlArea::Drag)
                .child(div().w_2().h_2().rounded_full().bg(tx::rgba(tx::FOCUS)))
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(tx::rgba(tx::TEXT))
                        .child("滴答 Lite"),
                ),
        )
        .child(
            titlebar_button("btn-pin", "置顶", pin_color).on_click(cx.listener(
                |this, _, window, cx| {
                    this.topmost = !this.topmost;
                    platform::set_topmost(window, this.topmost);
                    cx.notify();
                },
            )),
        )
        .child(
            titlebar_button("btn-hide", "—", tx::TEXT_MUTED).on_click(cx.listener(
                |_this, _, window, _cx| {
                    platform::hide_window(window);
                },
            )),
        )
        .child(
            titlebar_button("btn-close", "✕", tx::TEXT_MUTED).on_click(cx.listener(
                |_this, _, window, _cx| {
                    // 关闭按钮 = 最小化到托盘（退出走托盘菜单）
                    platform::hide_window(window);
                },
            )),
        )
}

fn titlebar_button(id: &'static str, label: &'static str, color: u32) -> Stateful<Div> {
    div()
        .id(id)
        .cursor_pointer()
        .px_2()
        .h_full()
        .flex()
        .items_center()
        .text_xs()
        .text_color(rgb(color))
        .hover(|s| s.bg(tx::rgba(tx::BG_HOVER)).text_color(tx::rgba(tx::TEXT)))
        .child(label)
}
