---
id: BUG-0002
title: 清单 chip 行显示不全、文字溢出被任务区遮挡
status: open
severity: major
owner: vfq
created: 2026-11-26
updated: 2026-11-26
affects: 0.1.0
related: [REQ-0001]
---

# BUG-0002 清单 chip 行显示不全、文字溢出被遮挡

## 1. 现象

顶部「全部 / 各清单」的 chip 行在清单数量稍多（≥ 4 个）或清单名较长时：

- chip 被压窄，名字显示成残缺的半个字；
- 长清单名会折成两行，第二行超出 chip 行下边界，压到任务列表上方，看起来像被"切掉一半"；
- 横向滚动条不出现，用户无法通过滚动看到被压掉的 chip。

## 2. 复现

1. 新建 5 个清单，其中至少一个名字 ≥ 6 个汉字（如「季度汇报材料」）。
2. 观察顶部 chip 行。

- 期望：chip 保持自身宽度，超出部分横向滚动；名字过长以省略号截断，始终单行
- 实际：chip 被压缩、文字折行、折行部分被下方任务区覆盖

窗宽 380px 时必现。

## 3. 根因

三个因素叠加，缺一不可：

**根因 A —— chip 被 flex 压缩，而不是触发横向滚动。**

- 容器：`sidebar.rs` 用 `flex_1() + min_w_0() + overflow_x_scroll()`，主动放弃了最小宽度约束；
- 子项：`vault_chip` **未设置 `flex_shrink_0()`**，而 gpui 的 `Style::default()` 里
  `flex_shrink: 1.0`（`gpui-pre-0.3.10/src/style.rs:802`）。

于是空间不足时，taffy 选择压缩每个 chip，而不是让内容撑宽去触发滚动。

**根因 B —— chip 内文本允许换行。**

标签节点只加了 `.overflow_hidden()`，缺 `whitespace_nowrap()`。被压窄后中文正常折行，
`py_1 + 两行行高` 直接超过容器的固定高度 `h(36.0)`。

**根因 C —— 横向滚动的裁剪掩码不裁纵向。**

`overflow_x_scroll()` 只把 `overflow.x` 设为 `Scroll`，`overflow.y` 仍为 `Visible`。
`Style::overflow_mask()`（`gpui-pre-0.3.10/src/style.rs:638-687`）在
`(x: false, y: true)` 分支下返回的掩码**只限制 x 范围**，纵向不裁剪。
溢出的第二行因此被绘制到 chip 行之外，而任务区在 `render` 顺序上更靠后，
后绘制覆盖先绘制 → 视觉上就是"被遮挡"。

结论：不是容器高度不够导致裁切（纵向压根没裁），也不是滚动机制没生效
（`id="vault-bar"` 存在，滚动状态可正常建立）。

## 4. 修复

```rust
// 1) 容器：补纵向裁剪，外层禁止被压扁
let mut chips = div()
    .id("vault-bar")
    // …
    .overflow_x_scroll()
    .overflow_y_hidden();        // 根因 C

// 2) 每个 chip：禁止压缩
div().id(id)
    .flex_shrink_0()             // 根因 A（关键一行）
    .h(px(24.0))
    // …

// 3) chip 文本：单行 + 上限 + 省略号
div().text_xs()
    .max_w(px(140.0))
    .truncate()                  // 根因 B
```

`truncate()` 等价于 `overflow_hidden() + whitespace_nowrap() + text_ellipsis()`
（`gpui-pre-0.3.10/src/styled.rs:139-141`）；`flex_shrink_0()` 见同文件 `:250-255`。

外层 bar 容器同时补 `.flex_shrink_0()`，避免 `body` 纵向空间紧张时整行被压扁。

## 5. 回归测试

- [ ] 5 个清单（含长名）时，每个 chip 宽度正常，长名以「…」截断且为单行
- [ ] chip 行可横向滚动（滚轮在 chip 行上滚动 / 拖拽），能看到全部清单
- [ ] chip 行下方边界干净，无文字溢出到任务列表
- [ ] 窗口缩到最小宽度 340px 时行为一致
- [ ] 选中态（蓝色边框）与被压缩修复后视觉一致

## 6. 变更记录

| 日期 | 版本 | 说明 |
|---|---|---|
| 2026-11-26 | v1 | 记录三重根因（flex 压缩 / 文本换行 / x-only 掩码不裁 y） |
