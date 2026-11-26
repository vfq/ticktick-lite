//! 自绘标题栏：拖动区 + 番茄钟入口 / 设置 / 置顶 / 最小化到托盘 / 关闭（隐藏到托盘）。

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::theme_ext as tx;

use super::RootView;

pub fn render_titlebar(view: &RootView, _window: &mut Window, cx: &mut Context<RootView>) -> Div {
    let topmost = view.settings.read(cx).get().topmost;
    let pin_color = if topmost { tx::ACCENT } else { tx::TEXT_FAINT };

    let bar = div()
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
        );

    // 番茄钟入口：平时只显示 🍅，计时中显示倒计时（REQ-0003）。
    // 用 shadowing 而不是 `mut bar`：关掉 pomodoro feature 时不会有 unused_mut。
    #[cfg(feature = "pomodoro")]
    let bar = {
        let (label, ticking) = view.pomodoro_entry(cx);
        let color = if ticking { tx::DANGER } else { tx::TEXT_MUTED };
        bar.child(
            titlebar_button("btn-pomodoro", label, color)
                .on_click(cx.listener(|this, _, _window, cx| this.toggle_pomodoro(cx))),
        )
    };

    bar.child(
        titlebar_button("btn-settings", "⚙".to_string(), tx::TEXT_FAINT)
            .on_click(cx.listener(|this, _, _window, cx| this.open_settings(cx))),
    )
    .child(
        titlebar_button("btn-pin", "置顶".to_string(), pin_color).on_click(cx.listener(
            |this, _, _window, cx| {
                this.settings
                    .update(cx, |store, cx| store.edit(cx, |s| s.topmost = !s.topmost));
            },
        )),
    )
    .child(
        titlebar_button("btn-hide", "—".to_string(), tx::TEXT_MUTED).on_click(cx.listener(
            |this, _, window, cx| {
                this.hide_to_tray(window, cx);
            },
        )),
    )
    .child(
        titlebar_button("btn-close", "✕".to_string(), tx::TEXT_MUTED).on_click(cx.listener(
            |this, _, window, cx| {
                // 关闭按钮 = 最小化到托盘（退出走托盘菜单）
                this.hide_to_tray(window, cx);
            },
        )),
    )
}

fn titlebar_button(id: &'static str, label: impl Into<SharedString>, color: u32) -> Stateful<Div> {
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
        .child(label.into())
}
