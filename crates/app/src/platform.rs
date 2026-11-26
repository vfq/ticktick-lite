//! Windows 平台能力：窗口置顶、隐藏/显示（托盘还原）。

use gpui_kit::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    HWND_NOTOPMOST, HWND_TOPMOST, SW_HIDE, SW_SHOW, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SetForegroundWindow, SetWindowPos, ShowWindow,
};

/// 从 gpui 窗口取原生 HWND。
///
/// 注意必须使用完全限定语法：`Window::window_handle()` 这个 inherent 方法
/// 返回的是 gpui 的 `AnyWindowHandle`，而非原生句柄。
pub fn hwnd_of(window: &Window) -> Option<HWND> {
    let handle = HasWindowHandle::window_handle(window).ok()?;
    match handle.as_raw() {
        RawWindowHandle::Win32(h) => Some(HWND(h.hwnd.get() as *mut core::ffi::c_void)),
        _ => None,
    }
}

/// 切换窗口置顶（始终悬浮于其他窗口之上）。
pub fn set_topmost(window: &Window, topmost: bool) {
    if let Some(hwnd) = hwnd_of(window) {
        let z = if topmost {
            HWND_TOPMOST
        } else {
            HWND_NOTOPMOST
        };
        unsafe {
            let _ = SetWindowPos(
                hwnd,
                z,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }
}

/// 隐藏窗口到托盘。
pub fn hide_window(window: &Window) {
    if let Some(hwnd) = hwnd_of(window) {
        unsafe {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
    }
}

/// 从托盘还原并前置窗口。
pub fn show_window(window: &Window) {
    if let Some(hwnd) = hwnd_of(window) {
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetForegroundWindow(hwnd);
        }
    }
}
