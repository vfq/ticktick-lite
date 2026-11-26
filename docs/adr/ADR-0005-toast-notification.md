---
id: ADR-0005
title: Windows 系统通知走 AUMID 注册 + notify-rust
status: accepted
date: 2026-11-26
updated: 2026-11-26
deciders: [vfq]
related: [REQ-0003]
---

# ADR-0005 Windows 系统通知走 AUMID 注册 + notify-rust

## 背景

番茄钟到点需要弹系统通知。初版用 `notify-rust` 的 `Notification::show()`，
但**未设置 AppUserModelID**，Windows 会把通知显示成「Windows PowerShell」——
因为 `notify-rust` 在未指定 app_id 时会回落到 `Toast::POWERSHELL_APP_ID`。

## 决策

### 1. 保留 `notify-rust`，不换 crate

已经核实：`notify-rust 4.18.2` 在 Windows 上就是 `tauri-winrt-notification` 的**薄封装**：

```toml
# notify-rust-4.18.2/Cargo.toml:162
[target.'cfg(target_os="windows")'.dependencies.winrt-notification]
version = "0.8"
package = "tauri-winrt-notification"
```

`tauri-winrt-notification` 本来就在依赖树里（`Cargo.lock` 0.8.1）。直接换成它只是省掉一层壳，
不改变任何行为，反而要多写一份调用代码。**收益为零，故不动。**

同时把版本钉死为 `=4.18`：crates.io 已发布 4.19.0，caret 范围会在下次构建时静默升级，
而 Windows 通知路径是必须真机验证的部分——不允许无人值守地改变。

### 2. 必须注册 AUMID

在 `HKCU\SOFTWARE\Classes\AppUserModelId\<AUMID>` 写入：

| 值 | 内容 |
|---|---|
| `DisplayName` | `滴答 Lite` |
| `IconBackgroundColor` | `0`（透明） |
| `IconUri` | 可选，暂缺省 → 用系统默认图标 |

AUMID 常量：`vfq.TickTickLite.Desktop`（≤129 字符、无空格）。

理由（依据 `tauri-winrt-notification` 的 `unpackaged_app` 示例）：
非 MSIX 打包的应用**必须**注册 AUMID，否则：

- 不设 `app_id` → 通知能弹，但显示成「Windows PowerShell」（借用 PowerShell 已注册的 AUMID）；
- 设了未注册的 `app_id` → `Show()` 直接失败（`0x80070490` / `0x803E0105`）。

两种情况都不可接受：前者品牌错误，后者功能失效。

### 3. 发送放到后台线程

WinRT 的 toast 调用在未初始化 apartment 的线程上会自动加入 MTA，直接调用可行，
但内部有约 10ms 的同步等待。放在前台帧里会造成可感知的卡顿，因此在
`cx.background_executor().spawn(...)` 里发送。

### 4. 失败必回落，不静默

```rust
if let Err(e) = n.show() {
    log::warn!("系统通知发送失败: {e}");
}
```

同时保留应用内 toast（窗口顶部 3 秒提示）作为兜底。
通知属于"增强体验"而非"业务必需"，失败不应该影响计时器的状态流转。

`sender` 受 `pomodoro.notify` 开关控制；关闭时连后台任务都不创建。

## 后果

**正面**
- 通知显示正确的应用名与图标背景，不再是 PowerShell。
- 无桌面会话（如 CI）或通知被系统禁用时，只有一条 `warn` 日志，不影响主流程。
- 应用启动时写一次注册表，之后所有通知复用。

**负面 / 风险**
- 写注册表需要权限：`HKCU` 是当前用户范围，普通权限即可，但企业策略可能禁止。
  失败只 `warn`，通知会退化为"能弹但显示成 PowerShell"。
- 注册表残留：卸载应用不会自动清理 `HKCU\...\AppUserModelId\vfq.TickTickLite.Desktop`。
  体积极小，可接受；若将来做安装包，在卸载脚本里删一次即可。
- **CI 无法验证**：通知需要真实桌面会话，`windows-check` workflow 覆盖不到。
  这是明确接受的人工验证项，已写入 REQ-0003 的验收清单。

## 替代方案

1. **改用 `tauri-winrt-notification` 直连** —— 省一层壳，但要重写调用代码并重新验证，
   收益为零，否决。
2. **用 `windows` crate 手写 `ToastNotificationManager`** —— 需要大量 COM 样板，
   且 `windows 0.58` 的 WinRT 支持不如 `tauri-winrt-notification` 成熟，否决。
3. **不做系统通知，只弹应用内提示** —— 番茄钟在小窗最小化到托盘后用户看不到应用内提示，
   通知是刚需，否决。
