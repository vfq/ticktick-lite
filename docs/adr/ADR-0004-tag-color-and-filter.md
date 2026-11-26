---
id: ADR-0004
title: 标签颜色派生策略与筛选语义
status: accepted
date: 2026-11-26
updated: 2026-11-26
deciders: [vfq]
related: [REQ-0001]
---

# ADR-0004 标签颜色派生策略与筛选语义

## 背景

`tags.color` 列从建表起就存在，但 `repo::ensure_tags` 插入时恒写 `NULL`，
UI 也从没读过该字段——任务行的标签 chip 用固定的 `BG_CHIP`（`#F1F3F6`）灰色。
结果是所有标签长得完全一样，标签在视觉上无法承担分类职责。

同时需要决定：多选标签筛选时，是「并集（OR）」还是「交集（AND）」。

## 决策

### 1. 颜色存 `#RRGGBB`，历史 NULL 按名称哈希回填

`tags.color` 统一存 `#RRGGBB` 字符串（大写十六进制，带 `#`）。

迁移时对 `color IS NULL OR color = ''` 的行回填：

```rust
pub fn default_tag_color(name: &str) -> String {
    let mut h: u32 = 0x811C_9DC5;
    for b in name.as_bytes() {
        h ^= u32::from(*b);
        h = h.wrapping_mul(0x0100_0193);   // FNV-1a
    }
    color_to_hex(TAG_PALETTE[(h as usize) % TAG_PALETTE.len()])
}
```

关键性质：**同名永远同色**。即使两台机器的数据库独立迁移，同一个标签名也会落到同一颜色，
不会出现"我在公司电脑上「工作」是蓝色、在家是红色"的割裂。

回填写在 schema v2 迁移里，且只处理 NULL/空串 → 幂等，重复执行不改变已有颜色。

### 2. 读取侧再兜底一次

`Tag::color_hex()`：解析 `self.color` 失败时，回落到 `default_tag_color(self.name)`，
再失败回落 `0x6B7280`（中性灰）。

理由：数据库可能被手工改过、可能是旧版本写入的非法值。UI 不应该因为一条脏数据渲染异常，
更不应该 panic。**非法数据降级为默认值，而不是报错**——这是展示层的基本原则。

### 3. 调色板 8 色

```rust
pub const TAG_PALETTE: [u32; 8] = [
    0x2563EB, // 蓝
    0x30A46C, // 绿
    0xE5484D, // 红
    0xF76B15, // 橙
    0x8E4EC6, // 紫
    0x0091FF, // 亮蓝
    0xD6405F, // 玫红
    0x6B7280, // 灰
];
```

8 色是"人工可区分"与"不需要滚动选择"的折中。前 4 色复用 `Priority` 的语义色系，
保证优先级与标签在同一视觉语言里。

### 4. chip 三色由底色派生，而不是硬编码

```rust
pub fn tag_chip_colors(hex: u32) -> (Hsla, Hsla, Hsla) {
    let c: Hsla = rgb(hex).into();
    (c.opacity(0.14), c, c.opacity(0.35))   // 底 / 文字 / 边框
}
```

不用"给每个标签写死三色"的方式，因为调色板可扩展（将来加第 9 色），
派生方式能自动适配。

### 5. 筛选语义取 OR（并集）

`TaskFilter.tag_ids: Vec<String>`，SQL：

```sql
AND EXISTS (
  SELECT 1 FROM task_tags tt
  WHERE tt.task_id = tasks.id AND tt.tag_id IN (?, ?)
)
```

选 OR 的理由：
- 用户点击多个标签时的心理模型是"我想看这几类"，而不是"同时属于这几类的"；
- AND 在数据稀疏时几乎总是空结果（一个任务很难同时带 3 个标签），体验差；
- `EXISTS` 子查询天然去重——一个任务命中两个选中标签也只返回一次，不需要 `DISTINCT`。

**代价**：AND 语义无法表达。若将来需要，改成：

```sql
AND tasks.id IN (
  SELECT tt.task_id FROM task_tags tt
  WHERE tt.tag_id IN (...) GROUP BY tt.task_id HAVING COUNT(DISTINCT tt.tag_id) = N
)
```

改动局限在 `repo::list_tasks` 一处，不影响其他模块——这也是把 SQL 收在 repo 层的好处。

### 6. 新增反向索引

```sql
CREATE INDEX IF NOT EXISTS idx_task_tags_tag_id ON task_tags(tag_id);
```

`task_tags` 的主键是 `(task_id, tag_id)`，按 `tag_id` 反查任务用不上主键前缀，
只能全表扫。标签筛选是高频操作，这个索引是必需的。

## 后果

**正面**
- 标签第一次有了视觉区分度，且无需用户逐个配颜色（默认色即可用）。
- 颜色非法/缺失不会导致 UI 异常。
- 筛选 SQL 简单、无重复，可走索引。

**负面**
- 8 色哈希必然产生碰撞：标签数超过 8 时会出现同色。接受——颜色只是辅助编码，
  文字标签仍然是主要识别手段（不依赖颜色单通道，对色觉障碍友好）。
- `format_version` 升到 2 破坏了导出的向后兼容。已在 REQ-0001 记录，消费方需按版本分支。

## 替代方案

1. **每个标签随机取色** —— 同一标签在不同机器上颜色不同，且不可复现，否决。
2. **只用一种主题色 + 不同灰度** —— 区分度太低，等于没做，否决。
3. **让用户必须手动配色** —— 新建标签的摩擦太大，默认色应该开箱可用，否决。
