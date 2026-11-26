# AGENTS.md

本文件是**给自动化代理与协作者的硬约束**，优先级高于个人习惯。

## 1. 本机只做格式校验

- ✅ 允许：`cargo fmt --all`、`cargo fmt --all -- --check`
- ⚠️ 受限：`cargo check` —— **仅在阶段验收时执行一次**，由人工确认，不产出二进制
- ❌ 禁止：`cargo build`、`cargo run`、`cargo test`、`cargo clippy`（这些一律走 CI）

理由见 [ADR-0002](docs/adr/ADR-0002-ci-self-hosted-qc.md)。动机有两条：
1. 保持本地与 CI 的结果一致，避免"本机能跑、CI 挂"；
2. 本项目 `target/` 上的 cargo 并发会互相锁死 `.o` 文件（Windows 报「拒绝访问 os error 5」），
   所以同一时刻只允许一个 cargo 进程。

## 2. 改动必须同时落文档

提交前逐项确认：

| 你做了什么 | 必须提交 |
|---|---|
| 新增功能 / 改变已有行为 | `docs/requirements/REQ-xxxx-*.md` |
| 修复缺陷 | `docs/bugs/BUG-xxxx-*.md`（含「根因」「回归测试」） |
| 新增依赖 / 跨模块技术选型 | `docs/adr/ADR-xxxx-*.md` |
| 以上任意一项 | `docs/README.md` 索引表对应行 + 文档 `updated` 字段 |

文档骨架见 `docs/README.md`。CI 的 `docs` job 会校验 front matter 的四字段。

## 3. 提交规范

- 分支：`main` 受保护，一律走 PR
- 标题：`feat(tags): ...` / `fix(titlebar): ...` / `chore(ci): ...`
- 正文第一行引用文档 id，例如 `Refs: BUG-0001`

## 4. 依赖与版本

- `gpui-kit` 与 `notify-rust` 使用**精确版本**（`=0.8.1` / `=4.18`）。
  `gpui-kit` 的 caret 范围会在次版本引入 `gpui-pre` 升级，属于破坏性变更。
- 升级 `gpui-kit` 必须：跑一次 `windows-check` workflow + 真机点验 + 写 ADR。

## 5. 平台事实（易踩坑，已核实）

- `window.start_window_move()` 在 Windows 是**空实现**，窗口拖动只能用 `window_control_area(WindowControlArea::Drag)`。
- `WindowKind::Normal` 在 Windows **没有** `WS_CAPTION`，`appears_transparent: false` 不等于"原生标题栏"。自绘标题栏是唯一可行做法。
- `WindowKind::AnchoredPopup` 在 Windows 直接返回不支持，任何下拉都必须走窗内联展开。
- 拖动区（`window_control_area`）会吞掉子元素的点击，按钮必须放在拖动区**外**。
