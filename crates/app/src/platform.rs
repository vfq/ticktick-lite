//! Windows 平台能力：窗口置顶、隐藏/显示（托盘还原）、通知用的 AUMID 注册。

use gpui_kit::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey,
    RegCreateKeyExW, RegSetValueExW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    HWND_NOTOPMOST, HWND_TOPMOST, SW_HIDE, SW_SHOW, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SetForegroundWindow, SetWindowPos, ShowWindow,
};
use windows::core::PCWSTR;

/// 应用的 AppUserModelID。非 MSIX 打包的应用必须注册它，否则系统通知要么
/// 显示成「Windows PowerShell」（未设 app_id），要么直接发送失败（app_id 未注册）。
pub const AUMID: &str = "vfq.TickTickLite.Desktop";

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

/// 把 UTF-16 字符串转成带 NUL 结尾的缓冲，供 `RegSetValueExW` 使用。
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 在 `HKCU\SOFTWARE\Classes\AppUserModelId\<AUMID>` 注册应用标识。
///
/// 只需成功一次，之后所有通知复用；失败只 `warn`——通知会退化成
/// 「能弹但显示成 PowerShell」，不影响计时器主流程。
pub fn register_aumid() {
    let subkey = wide(&format!("SOFTWARE\\Classes\\AppUserModelId\\{AUMID}"));
    let mut key = HKEY::default();
    unsafe {
        let status = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR::from_raw(subkey.as_ptr()),
            0,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut key,
            None,
        );
        if status.0 != 0 {
            log::warn!(
                "注册 AUMID 失败（0x{:X}），系统通知可能显示为 PowerShell",
                status.0
            );
            return;
        }
        let name = wide("DisplayName");
        let value = wide("滴答 Lite");
        let bytes: Vec<u8> = value.iter().flat_map(|u| u.to_le_bytes()).collect();
        let set = RegSetValueExW(
            key,
            PCWSTR::from_raw(name.as_ptr()),
            0,
            REG_SZ,
            Some(&bytes),
        );
        if set.0 != 0 {
            log::warn!("写入 DisplayName 失败（0x{:X}）", set.0);
        }
        let _ = RegCloseKey(key);
    }
}
