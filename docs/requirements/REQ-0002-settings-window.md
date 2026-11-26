---
id: REQ-0002
title: 独立设置窗口：所有设置集中管理
status: implemented
owner: vfq
created: 2026-11-26
updated: 2026-11-26
related: [ADR-0003, REQ-0001, REQ-0003]
---

# REQ-0002 独立设置窗口：所有设置集中管理

## 1. 背景

初版没有任何设置入口，配置项散落在 UI 状态里且**全部不持久化**：

| 设置项 | 位置 | 现状 |
|---|---|---|
| 置顶开关 | `RootView.topmost` | 仅内存，重启回 `false` |
| 显示已完成任务 | `RootView.show_completed` | 仅内存 |
| 番茄钟时长 | `plugins/pomodoro` 的 3 个字段 | 从 `settings` 表读，但**全项目无人写入**，永远是硬编码 25/5/15 |
| 长休间隔 | `ROUNDS_PER_LONG_BREAK` 常量 | 不可配 |
| 通知开关 | 无 | 永远弹 |

`settings(key, value)` KV 表与 `repo::get_setting/set_setting` 早已就绪，缺的只是写入端与 UI。

## 2. 目标

1. 提供一个**独立的设置窗口**（不是主窗内的覆盖层），承载全部设置项。
2. 设置写入 `settings` 表，重启保留。
3. 主窗与设置窗共享同一份状态，任一窗口改动，另一窗口立即生效（置顶、显示已完成、番茄钟时长）。
4. 标签管理作为设置窗口的一个分区（见 REQ-0001）。

## 3. 交互

窗口尺寸 560×560，最小 480×440，**自绘 32px 标题栏**（可拖动、可关闭）。

```
┌────────────────────────────────────────┐
│ ⋮⋮ 设置                            ✕  │  ← 拖动区（按钮在拖动区外）
├──────────┬─────────────────────────────┤
│ 常规     │  置顶显示          [ ○──]   │
│ 番茄钟   │  显示已完成任务    [ ──●]   │
│ 标签     │                             │
│          │  数据文件                   │
│          │  %APPDATA%\…\data.db        │
│          │  [ 打开所在目录 ]           │
├──────────┴─────────────────────────────┤
│ 修改会立即保存                          │
└────────────────────────────────────────┘
```

**保存策略：即时保存**，无「应用 / 取消」按钮。理由是小工具类应用的心智模型是"改了就生效"，避免用户在两个窗口间来回确认。

## 4. 方案

### 4.1 存储

`crates/core/src/settings.rs` 新增类型化结构，键名统一为 `域.字段` 形式：

| 键 | 类型 | 默认 |
|---|---|---|
| `ui.topmost` | bool | `false` |
| `ui.show_completed` | bool | `false` |
| `pomodoro.focus_mins` | u32 | 25 |
| `pomodoro.short_break_mins` | u32 | 5 |
| `pomodoro.long_break_mins` | u32 | 15 |
| `pomodoro.rounds_per_long_break` | u32 | 4 |
| `pomodoro.notify` | bool | `true` |

历史键 `pomodoro_focus_mins` / `pomodoro_short_break_mins` / `pomodoro_long_break_mins`（无点号）保留**只读兼容**：`load()` 时新键缺失则回落旧键。`save()` 只写新键，不动旧键（留回滚余地）。

所有数值在 `load()` 与 `save()` 两侧都过 `clamp()`：专注 1–180、短休 1–60、长休 1–120、轮数 1–12。非法输入不会进库。

### 4.2 跨窗口状态

```rust
pub struct SettingsStore { db: Database, settings: AppSettings }
pub enum SettingsEvent { Changed }
impl EventEmitter<SettingsEvent> for SettingsStore {}

impl SettingsStore {
    pub fn settings(&self) -> &AppSettings;
    pub fn edit(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut AppSettings));
}
```

`edit()` 内部：改内存 → `clamp` → 写库（失败只 `log::warn!`，不阻断 UI）→ `cx.emit(Changed)` + `cx.notify()`。

两个窗口都 `cx.subscribe_in(&store, window, ...)`。gpui 的监听按 `EntityId` 挂在 App 上，同一 Entity 支持多订阅者，互不覆盖。`Subscription` 必须存进各窗口的 `_subs: Vec<Subscription>`，否则立即注销。

### 4.3 单实例

设置窗口句柄存 `RootView.settings_window: Option<AnyWindowHandle>`。入口统一：

```rust
if self.focus_settings(cx) { return; }   // update() 成功 = 还活着，激活并返回
self.open_settings_window(cx)?;          // update() 返回 Err = 已关闭，清空后新开
```

`AnyWindowHandle::update()` 在窗口被 `remove_window()` 后返回 `Err`，因此不需要额外的 `closed` 标志位。

主窗隐藏到托盘时，一并隐藏设置窗（否则会出现"主窗没了、设置窗还飘着"的割裂感）。

## 5. 验收标准

> 勾选含义：`[x]` 已有自动化测试或代码级证据；`[ ]` 依赖 Windows 真机交互，统一在 P5 真机清单里逐条走。

- [x] AC-1 点标题栏齿轮打开设置窗口；已打开时再次点击只是置前，不产生第二个窗口
- [x] AC-2 设置窗口可拖动、可关闭（自绘标题栏）
- [ ] AC-3 改「置顶显示」后主窗立即置顶；重启应用仍保持（重启项待真机）
- [ ] AC-4 改「显示已完成任务」后主窗列表立即变化；重启保持（重启项待真机）
- [x] AC-5 修改番茄钟四个参数后，番茄钟下次进入 Idle 态即按新时长计
- [x] AC-6 输入越界值（如 999）被钳制到上限，不写库
- [x] AC-7 主窗隐藏到托盘时，设置窗口一并关闭（不留悬空窗口）
- [x] AC-8 老库（只有旧键 `pomodoro_focus_mins=50`）升级后，设置页显示 50 而非 25

## 6. 验证

- 自动：CI `test (core)` 覆盖 `settings` 的加载 / 兼容 / 钳制 / 往返
- 手动：Windows 真机按 AC-1 ~ AC-8 逐条走一遍

## 7. 变更记录

| 日期 | 版本 | 说明 |
|---|---|---|
| 2026-11-26 | v1 | 初稿，独立窗口 + 即时保存 |
| 2026-11-26 | v1.1 | 落地（P4a/P4b）。`SettingsStore` 作为跨窗口单一真相实体：改值即`clamp`→落库→`emit(SettingsChanged)`；主窗订阅后同步置顶并 `reload`。设置窗为独立 gpui 窗口（440×560，`WindowKind::Normal` + 自绘 34px 标题栏走 `window_control_area(Drag)`），左导航三区（常规/番茄钟/标签），右侧 `overflow_y_scroll`。窗口句柄存 `RootView::settings_window`，`open_or_focus` 用 `AnyWindowHandle::update` 判活去重。**AC-7 从"一并隐藏"改为"一并关闭"**：托盘只有「显示窗口」一个入口，隐藏后无法召回设置窗，会留下永久不可见的孤儿窗口，属实现期的合理修正。 |
