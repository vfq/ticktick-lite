---
id: BUG-0001
title: 主窗自绘标题栏无法拖动窗口
status: open
severity: major
owner: vfq
created: 2026-11-26
updated: 2026-11-26
affects: 0.1.0
related: [ADR-0001]
---

# BUG-0001 主窗自绘标题栏无法拖动窗口

## 1. 现象

按住主窗口顶部标题栏的空白区域拖动，窗口没有任何反应。
标题栏右侧的「置顶 / 最小化 / 关闭」三个按钮工作正常，因此不是整块标题栏失去交互。

## 2. 复现

1. 启动应用（`target/debug/ticktick.exe`）。
2. 鼠标左键按住标题栏左侧「● 滴答 Lite」附近区域并移动。

- 期望：窗口跟随鼠标移动
- 实际：窗口不动，鼠标指针保持普通箭头，无任何视觉反馈

必现，与是否置顶无关。

## 3. 根因

`crates/app/src/view/titlebar.rs` 的拖动区只绑定了：

```rust
.on_mouse_down(MouseButton::Left, |_, window, _| {
    window.start_window_move()
})
```

而 `Window::start_window_move()` 在 **Windows 平台是空实现**：

- 上层转发：`gpui-pre-0.3.10/src/window.rs:3014-3025`，文档注释写明
  *"Handle window movement for Linux and macOS"*；
- 平台默认实现：`gpui-pre-0.3.10/src/platform.rs:1356` 只有 `fn start_window_move(&self) {}`；
- Windows 后端 `gpui-pre-windows-0.3.10` 全 crate **没有覆写该方法**（零命中）。

即：点击拖动区只是触发了一个什么都不做的回调。

**放大因素**：主窗 `WindowOptions { titlebar: None }`，在 Windows 端等价于
`hide_title_bar = true`（`gpui-pre-windows-0.3.10/src/window.rs:478-482`），
客户区被 `WM_NCCALCSIZE` 拉满整窗，因此也**不存在系统标题栏作为兜底拖动区**。

## 4. 修复

改用 gpui 的窗口控制区命中测试通道：

```rust
div()
    .id("titlebar-drag")
    .flex_1()
    .h_full()
    .window_control_area(WindowControlArea::Drag)   // ← 关键
    // 删除 on_mouse_down + start_window_move
```

- `window_control_area` 定义在 `InteractiveElement`：`gpui-pre-0.3.10/src/elements/div.rs:1302-1308`；
- 命中框落地：`window.rs:5329-5335` → 每帧收集 `window_control_hitboxes`；
- Windows 端在 `WM_NCHITTEST` 里翻译成 `HTCAPTION`：`gpui-pre-windows-0.3.10/src/events.rs:955-971`，
  之后由 `DefWindowProc` 接管拖动。

生效前提：`hide_title_bar == true`（主窗已满足）、`is_movable == true`（默认 `true`）。

**约束**：拖动区会吞掉子元素的点击（`HTCAPTION` 优先），因此标题栏按钮必须放在拖动区**之外**——
现有结构已是 `[拖动区 flex_1][按钮组]`，满足要求。

## 5. 回归测试

- [ ] 按住标题栏可拖动，松开不跳位、不粘连
- [ ] 双击标题栏最大化 / 还原（系统默认行为，见下）
- [ ] 「置顶 / 最小化 / 关闭」三个按钮点击仍生效
- [ ] 设置窗口（`settings_window.rs`）用同一套做法，同样可拖动

**已知行为**：因为 `WS_MAXIMIZEBOX` 存在，双击标题栏会最大化窗口。这是 Windows 对
`HTCAPTION` 的标准处理，本期按系统默认交付。若后续要禁止，需在 `platform.rs` 里拦截
`WM_NCLBUTTONDBLCLK`。

## 6. 变更记录

| 日期 | 版本 | 说明 |
|---|---|---|
| 2026-11-26 | v1 | 记录根因（`start_window_move` 在 Windows 为空实现）与修复方案 |
