---
id: ADR-0003
title: 设置统一存 settings KV 表 + 类型化 AppSettings
status: accepted
date: 2026-11-26
updated: 2026-11-26
deciders: [vfq]
related: [REQ-0002]
---

# ADR-0003 设置统一存 `settings` KV 表 + 类型化 `AppSettings`

## 背景

`settings(key, value)` 表在初版就已建好，`repo::get_setting` / `set_setting` 也已实现，
但**没有任何生产代码调用 `set_setting`**——番茄钟读 `pomodoro_focus_mins` 等三个键，
永远读不到值，永远走硬编码 25/5/15。

同时 `topmost`、`show_completed`、`filter_vault` 是 `RootView` 的内存字段，重启即丢。

现在要把所有设置收进独立设置窗口，需要一个统一的读写模型。

## 决策

### 1. 继续用 KV 表，不建强类型表

理由：
- 设置项数量少（当前 7 项）且**结构扁平**，建一张 `settings(id, bool_value, int_value, text_value, …)`
  的宽表是把简单问题复杂化；
- KV 表天然支持"未来加一项配置不改 schema"，而设置项的增长频率高于业务表；
- 迁移成本为零：表已存在，无需 `ALTER TABLE`，也就绕开了 SQLite 的列迁移痛点。

### 2. 在 core 里包一层类型化结构

`crates/core/src/settings.rs`：

```rust
pub struct AppSettings {
    pub topmost: bool,
    pub show_completed: bool,
    pub pomodoro: PomodoroConfig,
}

pub struct PomodoroConfig {
    pub focus_mins: u32,
    pub short_break_mins: u32,
    pub long_break_mins: u32,
    pub rounds_per_long_break: u32,
    pub notify: bool,
}
```

**键名规范**：`域.字段`（如 `pomodoro.focus_mins`），替代初版的下划线平铺
（`pomodoro_focus_mins`）。点号分层便于 `LIKE 'pomodoro.%'` 批量读取与清理。

### 3. 旧键只读兼容，不做破坏性迁移

`load()` 的读取顺序：新键 → 旧键 → 默认值。
`save()` **只写新键**，不删除旧键。

理由：
- 旧键从未被写过，实际上绝大多数用户库里根本没有这几行，兼容成本只是多一次 `SELECT`；
- 保留旧键使回滚到旧版本时设置不丢；
- 真正的迁移（删除旧键）留到确认新版本稳定后再做，届时是一次 `DELETE FROM settings WHERE key NOT LIKE '%.%'`。

### 4. 非法值在边界处钳制

`PomodoroConfig::clamp()` 在 `load()` 与 `save()` 两侧都调用：
专注 1–180、短休 1–60、长休 1–120、轮数 1–12。

不信任任何来源（包括自己写的 UI）：手改数据库、旧版本残留、并发写入都可能产生越界值。
钳制放在 `core` 而不是 UI，保证"无论从哪个入口进来都安全"。

### 5. 跨窗口共享用 `Entity<SettingsStore>` + `EventEmitter`

```rust
pub struct SettingsStore { db: Database, settings: AppSettings }
pub enum SettingsEvent { Changed }
impl EventEmitter<SettingsEvent> for SettingsStore {}
```

- 主窗与设置窗各自 `cx.subscribe_in(&store, window, …)`；
- gpui 的监听按 `EntityId` 挂在 App 上，**同一 Entity 支持多订阅者**，互不覆盖；
- `edit()` 内部完成"改内存 → clamp → 落库 → `emit`"，UI 不需要知道持久化细节。

**单一真相**：删除 `RootView.topmost` / `RootView.show_completed` 字段。
保留两份状态必然产生不同步，这是小项目里最常见的隐性 bug 源。

## 后果

**正面**
- 加一项设置的成本 = 加一个字段 + 一个键常量，不碰 schema。
- 设置窗与主窗天然一致，不存在"设置里开了但主窗没变"。

**负面**
- KV 表没有类型约束，读出来永远是 `String`，解析失败只能静默回落默认值
  （`get_setting` 本身用 `.ok()` 吞掉了所有错误，这一点在 `load()` 里补了日志）。
- 设置项之间有耦合时（如"长休间隔 ≤ 总轮数"）KV 无法表达，只能靠 `clamp()` 兜。

## 替代方案

1. **用 `serde_json` 把整个 `AppSettings` 序列化存进单行** —— 更简洁，但无法做单项原子更新，
   且未来若要从外部工具改某一项，得反序列化整个 blob。对 7 个字段来说不划算。
2. **每个设置项建独立表** —— 过度设计，否决。
3. **只在内存 + 退出时写盘** —— 崩溃丢配置，且跨窗口同步仍需一个共享实体，收益为零。
