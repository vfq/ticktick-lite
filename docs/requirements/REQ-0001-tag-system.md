---
id: REQ-0001
title: 标签系统（选择 / 管理 / 筛选 / 导出）
status: in-progress
owner: vfq
created: 2026-11-26
updated: 2026-11-26
related: [ADR-0004, REQ-0002]
---

# REQ-0001 标签系统（选择 / 管理 / 筛选 / 导出）

## 1. 背景

数据库层在项目初版就已具备标签能力（`tags` / `task_tags` 表 + 多对多关联），但产品层面只做了最浅的一层：

- 任务编辑器里是一个**逗号分隔的纯文本框**，用户要记住标签名才能复用；
- `tags.color` 列存在，但 `repo::ensure_tags` 写入时恒为 `NULL`，任务行上的标签 chip 颜色**写死灰色**；
- `repo::list_tags` 已实现但**零调用点**，没有任何标签管理入口；
- `TaskFilter` 只有 `vault_id` / `status`，无法按标签筛选；
- 导出 JSON 的 `tags` 是 `["汇报", "文档"]` 这样的字符串数组，颜色信息丢失。

结果是「标签」在 UI 上退化为「给任务打几个灰字」，无法承担分类与检索职责。

## 2. 目标与非目标

**目标（四层全做）**
1. **选择器**：任务编辑器改为「已选 chip（可移除）+ 全部标签网格（点击切换）+ 输入框回车新建」。
2. **管理**：设置窗口「标签」分区支持列表展示（含任务计数）、改名、配色（8 色调色板）、删除。
3. **筛选**：点击任务行上的标签 chip 即按该标签过滤，顶部出现可关闭的筛选条。
4. **导出**：`format_version` 升到 2，`tags` 变为 `{ name, color }` 对象数组。

**非目标**
- 标签层级 / 嵌套标签
- 标签合并
- 跨 vault 的标签权限

## 3. 交互

任务编辑器（覆盖层，380px 窗宽）：

```
标签
┌ 已选 ────────────────────────────┐
│ [● 汇报 ✕] [● 文档 ✕]            │
└──────────────────────────────────┘
┌ 可选 ────────────────────────────┐
│ [● 工作] [● 紧急] [● 家庭]        │
│ [● 学习] [● 周会]                 │
└──────────────────────────────────┘
[ 新建或搜索标签…              ] ⏎
```

任务列表行内标签 chip：胶囊形，底色为标签色的 14% 透明度，文字与边框用标签色本身。点击 → 按该标签筛选。

## 4. 方案

### 4.1 数据模型

- `tags.color` 统一存 `#RRGGBB` 字符串。
- 历史 `NULL` 值由 schema v2 迁移**回填**：按标签名做 FNV-1a 哈希，落到 8 色调色板中的一个，保证同一名字永远同一颜色（迁移幂等，只处理 `NULL` 或空串）。
- 读取侧再做一次兜底：`Tag::color_hex()` 解析失败时同样走名称派生色，因此 UI 永远不会拿到非法颜色。
- `TaskFilter` 增加 `tag_ids: Vec<String>`，语义为 **OR（并集）**：命中任一选中标签即入选。SQL 用 `EXISTS` 子查询，天然避免一个任务多标签时重复出现。
- 新增 `idx_task_tags_tag_id`，让"按标签筛任务"不必全表扫 `task_tags`。

### 4.2 接口

```rust
// core/model.rs
pub const TAG_PALETTE: [u32; 8];
pub fn parse_hex_color(s: &str) -> Option<u32>;
pub fn color_to_hex(c: u32) -> String;
pub fn default_tag_color(name: &str) -> String;
impl Tag { pub fn color_hex(&self) -> u32; pub fn color_hex_string(&self) -> String; }

// core/repo.rs
pub fn create_tag(conn, name: &str, color: Option<&str>) -> Result<Tag>;
pub fn rename_tag(conn, id: &str, name: &str) -> Result<()>;
pub fn set_tag_color(conn, id: &str, color: Option<&str>) -> Result<()>;
pub fn delete_tag(conn, id: &str) -> Result<()>;
pub struct TagCount { pub tag: Tag, pub active: i64, pub total: i64 }
pub fn list_tags_with_counts(conn) -> Result<Vec<TagCount>>;
```

`delete_tag` 依赖 `task_tags` 的外键 `ON DELETE CASCADE`（连接已开 `PRAGMA foreign_keys`），任务本身不受影响。

### 4.3 UI

- `Overlay::TaskEditor` 的 `tags: Entity<InputState>` 换成 `tag_input: Entity<InputState>` + `selected_tags: Vec<String>`（存标签名，与 `repo::set_task_tags` 的入参一致）。
- 保存时不再 `split(',')`，直接用 `selected_tags`。
- 任务行 chip 由 `theme_ext::tag_chip_colors(hex)` 派生三色（底 / 文字 / 边框）。

## 5. 验收标准

- [ ] AC-1 任务编辑器可选多个标签，保存后重新打开正确回显
- [ ] AC-2 在设置窗口改名后，任务行标签立即显示新名
- [ ] AC-3 改色后任务行 chip 与设置页色点同步变化
- [ ] AC-4 删除标签后，任务不再带该标签，且任务本身保留
- [ ] AC-5 点击任务行标签 chip 过滤生效，顶部出现可关闭的筛选条
- [ ] AC-6 导出 JSON `format_version == 2`，`tags` 为 `[{name, color}]`，颜色为 `#RRGGBB`
- [ ] AC-7 新增同名标签时报错「标签「x」已存在」，不产生重复行
- [ ] AC-8 `cargo test -p ticktick-core` 全绿（含标签 CRUD、筛选去重、迁移幂等）

## 6. 验证

- 自动：CI `clippy (core)` + `test (core)`（`.github/workflows/ci.yml`）
- 手动（Windows 真机）：建 3 个标签 → 给任务打 2 个 → 点 chip 筛选 → 设置页改名改色 → 删除其中一个 → 导出 JSON 核对

## 7. 变更记录

| 日期 | 版本 | 说明 |
|---|---|---|
| 2026-11-26 | v1 | 初稿，四层全量 |
| 2026-11-26 | v1.1 | 数据层落地：schema v2（`idx_task_tags_tag_id` + 颜色回填）、`TAG_PALETTE`/`parse_hex_color`/`default_tag_color`/`Tag::color_hex`、标签 CRUD 与 `list_tags_with_counts`、`TaskFilter.tag_ids`（OR 语义）、导出 `format_version: 2`。`update_task_details` 收敛为 `TaskEdit` + `update_task`（顺带清掉 `too_many_arguments` 的 allow）。对应测试 17 项见 `crates/core/tests/core_flow.rs`。 |
