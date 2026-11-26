# 文档索引

本目录是项目的**单一事实来源**：任何需求、缺陷、技术决策都必须在此留档。
代码可以重写，结论必须可追溯。

## 索引

| ID | 类型 | 标题 | 状态 | 关联代码 |
|---|---|---|---|---|
| [REQ-0001](requirements/REQ-0001-tag-system.md) | 需求 | 标签系统（选择 / 管理 / 筛选 / 导出） | implemented | `core/{model,repo,db,export}.rs`、`app/view/{overlays,task_list,settings_window}.rs` |
| [REQ-0002](requirements/REQ-0002-settings-window.md) | 需求 | 独立设置窗口 | implemented | `app/view/settings_window.rs`、`app/settings_store.rs`、`core/settings.rs` |
| [REQ-0003](requirements/REQ-0003-pomodoro-floating.md) | 需求 | 番茄钟浮层化 + 时长可配 + 系统通知 | implemented | `plugins/pomodoro/panel.rs`、`app/view/{mod,titlebar}.rs` |
| [BUG-0001](bugs/BUG-0001-window-not-draggable.md) | 缺陷 | 主窗自绘标题栏无法拖动 | fixed | `app/view/titlebar.rs` |
| [BUG-0002](bugs/BUG-0002-vault-chip-clipped.md) | 缺陷 | 清单 chip 被压窄、溢出被遮挡 | fixed | `app/view/sidebar.rs` |
| [ADR-0001](adr/ADR-0001-rust-2024.md) | 决策 | 迁移到 Rust 2024 edition | accepted | `Cargo.toml`、`rust-toolchain.toml` |
| [ADR-0002](adr/ADR-0002-ci-self-hosted-qc.md) | 决策 | CI 使用自建 runner `qc`，本机只做格式校验 | accepted | `.github/workflows/`、`AGENTS.md` |
| [ADR-0003](adr/ADR-0003-settings-storage.md) | 决策 | 设置统一存 `settings` KV 表 + 类型化 `AppSettings` | accepted | `core/settings.rs`、`app/settings_store.rs` |
| [ADR-0004](adr/ADR-0004-tag-color-and-filter.md) | 决策 | 标签颜色派生与筛选语义 | accepted | `core/model.rs`、`core/repo.rs` |
| [ADR-0005](adr/ADR-0005-toast-notification.md) | 决策 | Windows 系统通知走 AUMID 注册 + notify-rust | accepted | `app/platform.rs`、`plugins/pomodoro/panel.rs` |

## 命名规则

| 类型 | 目录 | 文件名 | 序号 |
|---|---|---|---|
| 需求 | `requirements/` | `REQ-四位序号-短横线英文名.md` | 全局单调递增 |
| 缺陷 | `bugs/` | `BUG-四位序号-短横线英文名.md` | 全局单调递增 |
| 决策 | `adr/` | `ADR-四位序号-短横线英文名.md` | 全局单调递增 |
| 指南 | `guides/` | `主题.md` | 无序号 |

序号**不复用**。文件改名必须同步改 front matter 的 `id` 与本文索引表。

## Front matter 规范

每个文档开头必须是 YAML front matter，CI 会校验以下四个字段存在：

```yaml
---
id: REQ-0001
title: 标签系统（选择 / 管理 / 筛选 / 导出）
status: in-progress
updated: 2026-11-26
---
```

`status` 取值：
- 需求：`draft` / `accepted` / `in-progress` / `implemented` / `superseded`
- 缺陷：`open` / `investigating` / `fixed` / `wontfix`
- 决策：`proposed` / `accepted` / `superseded`

## 何时必须写什么

| 场景 | 必须新增 / 更新 |
|---|---|
| 新功能、行为变更 | `requirements/REQ-xxxx.md`（含验收标准 AC） |
| 缺陷修复 | `bugs/BUG-xxxx.md`（**必须**写「根因」与「回归测试」） |
| 引入依赖、跨模块技术选择 | `adr/ADR-xxxx.md` |
| 每次改动 | 本索引表对应行 + 文档 `updated` 字段 |

## 关联指南

- [构建与运行](guides/build-and-run.md)
- [CI 与 runner](guides/ci-and-runner.md)
- [发布检查清单](guides/release-checklist.md)
