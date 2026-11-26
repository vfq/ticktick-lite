---
id: REQ-0003
title: 番茄钟浮层化：可展开图标、时长可配、系统通知
status: implemented
owner: vfq
created: 2026-11-26
updated: 2026-11-26
related: [REQ-0002, ADR-0005]
---

# REQ-0003 番茄钟浮层化

## 1. 背景

初版番茄钟是**常驻窗口底部**的一块面板（`view/mod.rs` 里作为 `root` 的普通子元素追加），
即使处于 Idle（就绪）态也占掉约 150–170px 高度。在 380×620 的小窗里，这相当于永久牺牲
四分之一的任务可视区域——而番茄钟在多数时间并不需要被看见。

同时存在三个硬编码问题：

- 时长 `25 / 5 / 15` 是常量，虽已写从 `settings` 读取的逻辑，但**全项目没有任何写入点**，因此永远走默认值；
- 长休间隔 `ROUNDS_PER_LONG_BREAK = 4` 是编译期常量，不可配；
- 系统通知用 `notify-rust` 但**没有设置 AUMID**，Windows 会把通知显示成「Windows PowerShell」。

## 2. 目标

1. **可展开图标**：Idle 态只在标题栏显示一个 `🍅` 图标，默认不展开；运行时图标位置显示 `MM:SS` 倒计时。
2. **浮层而非常驻**：展开后以右下角浮层卡片呈现，覆盖在任务列表之上，不挤压布局；收起后完全不占空间。
3. **时长可配**：专注 / 短休 / 长休 / 长休间隔四项均可在设置窗口调整，改动后热生效。
4. **系统通知**：到点弹 Windows toast，标题与应用名正确（不显示成 PowerShell）；发送失败回落应用内提示。

## 3. 交互

标题栏：

```
[● 滴答 Lite]                     🍅  📌  —  ✕      ← Idle
[● 滴答 Lite]                    12:34  📌  —  ✕    ← 运行中（等宽，不跳动）
```

展开后右下角浮层（320×自动，圆角 12px，带阴影）：

```
┌──────────────────────────────┐
│ 专注中            12:34      │
│ ████████████░░░░░░░░  41%    │
│ 关联任务：写季度汇报          │
│ [ 暂停 ]  [ 放弃 ]           │
└──────────────────────────────┘
```

## 4. 方案

### 4.1 状态

`PomodoroPanel` 的 `collapsed: bool` 改为 `expanded: bool`（**默认 `false`**），
三个 `*_mins` 字段合并为 `config: PomodoroConfig`（来自 `core::settings`）。

新增方法：

```rust
pub fn is_expanded(&self) -> bool;
pub fn toggle_expanded(&mut self, cx: &mut Context<Self>);
pub fn titlebar_label(&self) -> (SharedString, bool);   // (文本, 是否高亮)
pub fn apply_config(&mut self, config: PomodoroConfig, cx: &mut Context<Self>);
```

`apply_config` 在 Idle 态同时刷新 `planned_secs` / `remaining_secs`，运行中只改后续阶段用的参数（不打断当前计时）。

### 4.2 挂载

由 `view/mod.rs` 负责定位，面板自身只管卡片：

```rust
if self.pomodoro.read(cx).is_expanded() {
    root = root.child(
        div().absolute().right(px(12.0)).bottom(px(12.0)).w(px(320.0))
            .child(self.pomodoro.clone())
    );
}
```

### 4.3 通知

见 [ADR-0005](../adr/ADR-0005-toast-notification.md)。要点：

- 启动时在 `HKCU\SOFTWARE\Classes\AppUserModelId\vfq.TickTickLite.Desktop` 写入 `DisplayName`；
- 发送时 `.app_id(APP_AUMID)`；
- `show()` 返回 `Err` 时 `log::warn!`，并保留应用内 toast 作为兜底；
- 发送放到 `background_executor`，避免 WinRT 初始化阻塞前台帧（约 10ms）。

## 5. 验收标准

> 勾选含义：`[x]` 已有代码级证据；`[ ]` 依赖 Windows 真机交互，统一在 P5 真机清单里逐条走。

- [ ] AC-1 启动后番茄钟**默认不展开**，窗口底部无常驻条，任务列表拿回完整高度（待真机看布局）
- [ ] AC-2 点标题栏 `🍅` 展开浮层；再点收起；收起后布局完全复原（待真机看布局）
- [x] AC-3 运行中标题栏显示 `MM:SS` 倒计时，浮层收起时不中断计时
- [x] AC-4 设置窗口把专注改为 1 分钟，番茄钟进入 Idle 后按 1 分钟计
- [ ] AC-5 专注结束弹出 Windows 通知，应用名为「滴答 Lite」，非「Windows PowerShell」（待真机）
- [x] AC-6 关闭 `pomodoro.notify` 后不再弹系统通知
- [x] AC-7 通知发送失败时，应用内出现提示且不 panic
- [x] AC-8 从任务行 `▶` 仍可直接对某任务发起专注

## 6. 验证

- 自动：CI 只覆盖 core（`PomodoroConfig` 的钳制与默认值）
- 手动：真机走 AC-1 ~ AC-8；系统通知必须在真实桌面会话下验证（CI 无桌面）

## 7. 变更记录

| 日期 | 版本 | 说明 |
|---|---|---|
| 2026-11-26 | v1 | 初稿：常驻条 → 可展开浮层 + 四项可配 + AUMID 通知 |
| 2026-11-26 | v1.1 | 落地（P4c）。`PomodoroPanel` 的 `collapsed` 改为 `expanded`（默认 `false`），`render()` 在收起态返回空卡片；由 app 壳在 `RootView::render` 里以 `absolute().right(12).bottom(12).w(320)` 挂载，彻底移出主布局（原为底部普通子元素）。时长来源改为 `PomodoroConfig`，`RootView` 订阅 `SettingsChanged` 后调 `apply_config` 热生效（Idle 态顺带刷新剩余时间，运行中不打断本轮）。标题栏新增 `🍅` 入口，文案由 `titlebar_label()` 提供（Idle 显示图标，计时中显示 `MM:SS` 并以 `DANGER` 色高亮）。通知走 `notify_rust::Notification` 的 `.app_id(core::plugin::AUMID)` + `.appname("滴答 Lite")`，在 `background_executor` 里发送、失败只 `warn`。 |
