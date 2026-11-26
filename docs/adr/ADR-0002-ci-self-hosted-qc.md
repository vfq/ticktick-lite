---
id: ADR-0002
title: CI 使用自建 runner qc，本机只做格式校验
status: accepted
date: 2026-11-26
updated: 2026-11-26
deciders: [vfq]
related: [ADR-0001]
---

# ADR-0002 CI 使用自建 runner `qc`，本机只做格式校验

## 背景

两条约束同时存在：

1. **项目是 Windows-only**：`windows` crate 的 `SetWindowPos` / `ShowWindow`、`tray-icon`、
   `gpui-pre-windows` 都是 Windows 专属；沙箱内的 `ticktick-app` 无法在 Linux 上产出可执行文件。
2. **GitHub 托管 runner 在该账户上因计费问题无法分配**，因此只能用 self-hosted runner。

`qc-4c4g` 是一台国内腾讯云 Linux 主机（4C4G），已用于其他仓库的重编译任务。
它**不能**构建本项目的 app/plugin，但完全能跑与平台无关的部分。

## 决策

### 1. CI 只跑可跨平台部分

`.github/workflows/ci.yml` 四个 job，全部 `runs-on: [self-hosted, qc]`：

| job | 命令 | 作用 |
|---|---|---|
| `fmt` | `cargo fmt --all -- --check` | 格式门禁（唯一在本机也能跑的一项） |
| `clippy (core)` | `cargo clippy -p ticktick-core --all-targets --locked -- -D warnings` | core 的静态检查 |
| `test (core)` | `cargo test -p ticktick-core --locked` | core 的行为验证 |
| `docs` | 校验 `docs/**/*.md` 的 front matter 四字段 | 强制文档规范落地 |

选择 `ticktick-core` 而非 `--workspace`，是因为 app/plugin 依赖 `gpui-kit`，
其平台后端在 Linux 上虽然能编译，但我们的业务代码里 `RawWindowHandle::Win32` 等
分支会被 `cfg` 掉，产出的是"能编译但不代表 Windows 结果"的假信号——比不跑更糟。

### 2. Windows 侧用独立的手动 workflow

`.github/workflows/windows-check.yml`，`on: workflow_dispatch`，`runs-on: windows-latest`。
不进入 PR 门禁，仅在需要时手动触发，拿到 app/plugin 的真实编译结果。

若托管 runner 仍不可分配，退化路径是**阶段验收时由人工在本机跑一次 `cargo check`**
（只 Check、不产出二进制、不跑 release），这是唯一能证明代码可编译的途径。

### 3. 本机只允许 `cargo fmt`

`AGENTS.md` 明文规定：

- ✅ `cargo fmt --all` / `--check`
- ⚠️ `cargo check` —— 仅阶段验收，人工触发一次
- ❌ `cargo build` / `run` / `test` / `clippy`

第二条理由与技术洁癖无关：本项目 `target/` 上**并发运行两个 cargo 进程会互锁 `.o` 文件**，
Windows 报「拒绝访问 (os error 5)」，导致构建莫名失败。此坑已在开发中实际踩到一次。

## 后果

**正面**
- 本地与 CI 结果单一来源，避免"本机能跑、CI 挂"。
- 格式门禁零成本，且能拦住绝大多数低级合并冲突。
- 文档规范由 CI 强制，不依赖人的自觉。

**负面 / 风险**
- **app/plugin 存在验证盲区**：CI 不覆盖，只能靠阶段验收的 `cargo check` + 真机点验。
  缓解：所有改动只使用"仓库中已出现过的 API"或已在 ADR/REQ 中核实并标注行号的 API。
- runner 是单点：qc-4c4g 掉线则 CI 全部 queued。
- 公开仓库 + self-hosted 有安全风险：fork PR 可在你的机器上执行任意代码。

## 安全加固（必须保留）

1. 每个 job 加 `if`：`github.event_name != 'pull_request' || head.repo.full_name == github.repository`，拒绝 fork PR。
2. 不使用 `pull_request_target`。
3. `permissions: contents: read`；`checkout` 带 `persist-credentials: false`。
4. runner 跑在专用低权用户下，不放任何 secret / deploy key。
5. `CARGO_TARGET_DIR` 按 job 分目录，避免同机并发锁。
6. 仓库设置开启「Require approval for all outside collaborators」。
