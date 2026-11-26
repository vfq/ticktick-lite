---
id: GUIDE-0001
title: 构建与运行
status: active
updated: 2026-11-26
---

# 构建与运行

## 前置条件

- **Windows 10/11**（本项目仅支持 Windows：`windows` crate、`tray-icon`、`gpui-pre-windows` 均为平台专属）
- **Visual Studio Build Tools**，勾选 "Desktop development with C++" + Windows SDK
  （`rusqlite` 的 `bundled` 特性需要 C 编译器编译 SQLite）
- Rust 工具链由 `rust-toolchain.toml` 自动锁定为 **1.100**，并自动安装 `rustfmt`

## 本机允许的命令

本机**只做格式校验**，理由见 [ADR-0002](../adr/ADR-0002-ci-self-hosted-qc.md)。

```bash
cargo fmt --all              # 格式化（提交前必做）
cargo fmt --all -- --check   # 检查是否已格式化（CI 同款）
```

编译与测试一律走 GitHub Actions，本机不跑 `cargo build` / `test` / `clippy`。

**唯一例外**：阶段验收时允许人工执行一次

```bash
cargo check --workspace --all-targets
```

只 Check、不产出二进制。这与 `cargo build` 的区别是跳过 codegen 与链接，快且不占磁盘。

## 通过 CI 获取构建产物

1. 推送后打开仓库的 Actions 页面，确认 `CI` workflow 通过。
2. 需要 Windows 二进制时，手动触发 `windows-check` workflow：
   ```bash
   gh workflow run windows-check.yml -f full_build=true
   gh run watch
   ```
3. 构建产物从该 run 的 Artifacts 下载。

> 若 GitHub 托管 runner 因计费不可分配，`windows-check` 会一直 queued。
> 此时退化方案：在本机跑一次 `cargo build --release`，并在 PR 中记录。

## 数据文件位置

```
%APPDATA%\ticktick-lite\TickTickLite\data\data.db
```

- 使用 WAL 模式，因此同目录会有 `data.db-wal` / `data.db-shm`，三者需一起备份
- 该目录已加入 `.gitignore`，不要提交
- schema 版本记录在 `schema_meta` 表，启动时自动迁移

## 常见问题

**Q: `cargo fmt` 报 `'cargo-fmt.exe' is not installed`**
A: `rust-toolchain.toml` 已声明 `components = ["rustfmt"]`，执行任意 `cargo` 命令时会自动安装。
若仍未装，手动执行 `rustup component add rustfmt`。

**Q: 编译报「拒绝访问 (os error 5)」**
A: 有另一个 cargo 进程正在使用同一个 `target/`。关掉它再重试。
**同一时刻只允许一个 cargo 进程**，这是本项目的硬约束。

**Q: 首次编译很慢**
A: `gpui-kit` 的依赖链（`gpui-pre` + Windows 图形栈）首次编译约 10–45 分钟，属正常。
CI 上用 `CARGO_TARGET_DIR` 固定缓存目录复用增量。
