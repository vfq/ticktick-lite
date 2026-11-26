//! 系统托盘：显示/隐藏窗口、退出。
//!
//! gpui 无内置托盘，这里使用 `tray-icon` crate + 轮询桥：
//! 托盘事件经 crossbeam channel 取出，在 gpui 前台 executor 中处理。

use std::time::Duration;

use gpui_kit::*;
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

use crate::platform;

const MENU_ID_SHOW: &str = "tray-show";
const MENU_ID_QUIT: &str = "tray-quit";

/// 生成 32x32 番茄红圆点托盘图标（RGBA）。
fn make_icon() -> Option<Icon> {
    let size = 32u32;
    let center = (size as f32 - 1.0) / 2.0;
    let radius = center - 1.0;
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist <= radius {
                // 番茄红，边缘 1px 抗锯齿
                let alpha = ((radius - dist).clamp(0.0, 1.0) * 255.0) as u8;
                rgba.extend_from_slice(&[0xE5, 0x48, 0x4D, alpha]);
            } else {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
    }
    Icon::from_rgba(rgba, size, size).ok()
}

/// 安装托盘并启动事件轮询。窗口句柄用于“显示窗口”时还原。
pub fn spawn_tray(cx: &mut App, handle: AnyWindowHandle) {
    let menu = Menu::new();
    let show_item = MenuItem::with_id(MENU_ID_SHOW, "显示窗口", true, None);
    let quit_item = MenuItem::with_id(MENU_ID_QUIT, "退出", true, None);
    let _ = menu.append_items(&[&show_item, &quit_item]);

    let Some(icon) = make_icon() else {
        log::warn!("无法创建托盘图标，跳过托盘");
        return;
    };

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("滴答 Lite")
        .with_icon(icon)
        .build();

    let Ok(tray) = tray else {
        log::warn!("托盘创建失败: {:?}", tray.err());
        return;
    };

    cx.spawn(async move |cx| {
        // 托盘对象必须保活
        let _tray = tray;
        loop {
            cx.background_executor()
                .timer(Duration::from_millis(80))
                .await;

            let mut show = false;
            let mut quit = false;

            while let Ok(event) = TrayIconEvent::receiver().try_recv() {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Down,
                    ..
                } = event
                {
                    show = true;
                }
            }
            while let Ok(event) = MenuEvent::receiver().try_recv() {
                match event.id().0.as_str() {
                    MENU_ID_SHOW => show = true,
                    MENU_ID_QUIT => quit = true,
                    _ => {}
                }
            }

            if show {
                let _ = cx.update(|app| {
                    let _ = handle.update(app, |_view, window, _app| {
                        platform::show_window(window);
                    });
                });
            }
            if quit {
                let _ = cx.update(|cx| {
                    cx.quit();
                });
                break;
            }
        }
    })
    .detach();
}
