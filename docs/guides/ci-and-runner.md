---
id: GUIDE-0002
title: CI 与自建 runner
status: active
updated: 2026-11-26
---

# CI 与自建 runner

配套决策见 [ADR-0002](../adr/ADR-0002-ci-self-hosted-qc.md)。

## workflow 一览

| 文件 | 触发 | runner | 作用 |
|---|---|---|---|
| `.github/workflows/ci.yml` | push `main` / PR / 手动 | `[self-hosted, qc]` | `fmt`、`clippy (core)`、`test (core)`、`docs` |
| `.github/workflows/windows-check.yml` | 仅手动 | `windows-latest` | app/plugin 的真实编译验证 |

## 为什么 app/plugin 不在 `ci.yml` 里

`qc` runner 是 Linux，而本项目是 Windows-only。在 Linux 上 `cargo check` 虽然能过，
但业务代码里的 `RawWindowHandle::Win32` 分支会被 `cfg` 掉——那是**假信号**，比不跑更糟。

因此 Windows 侧拆到独立的手动 workflow，不进入 PR 门禁。

## 本次部署实况（as-built）

| 项 | 值 |
|---|---|
| 主机 | `qc-4c4g.2no.dev`（SSH 端口 `23233`），腾讯云 4C / 3.6G / x86_64 |
| 运行账户 | `feature`（uid 1002，gid `maintainer`），**非 root** |
| runner 目录 | `/opt/actions-runner-ticktick` |
| systemd 服务 | `actions.runner.vfq-ticktick-lite.qc-ticktick.service` |
| runner 名称 | `qc-ticktick`（与同机 `qc-4c4g`、`qc-music` 区分） |
| 标签 | `self-hosted, Linux, X64, qc`（`qc` 为自定义标签，workflow 靠它匹配） |
| runner 版本 | `2.338.2`，复用同机 `/opt/actions-runner` 的二进制，未重新下载 |
| Rust | `/home/feature/.cargo` 下预装 `1.100.0`（含 `rustfmt`/`clippy`）与 `stable` |
| crates 镜像 | `/home/feature/.cargo/config.toml` 已配 USTC sparse 源 |
| 工具链镜像 | workflow 顶层 `RUSTUP_DIST_SERVER` / `RUSTUP_UPDATE_ROOT` 指向 USTC |
| 缓存目录 | `/var/cache/ticktick/target-{clippy,test}`，属主 `feature` |

**本机约定：一台机器上每个仓库一个 runner 目录 + 一个服务**。同机已有
`/opt/actions-runner`（`vfq/dqqxfw`）与 `/opt/actions-runner-music`（`vfq/music-catalog`），
新增仓库时**另建目录**，绝不复用已有实例的 `.runner` / `.credentials`。

关键注意点：runner 服务的 PATH **不含** `~/.cargo/bin`，所以 `ci.yml` 每个 Rust job
都要显式 `echo "$HOME/.cargo/bin" >> "$GITHUB_PATH"`，否则 `rustup` / `cargo` 全部 command not found。

## 首次部署 runner

### 1. 取注册令牌

```bash
gh api -X POST repos/<owner>/<repo>/actions/runners/registration-token --jq .token
```

令牌有效期 1 小时，立即使用。

### 2. 建专用低权用户

```bash
sudo useradd -m -s /bin/bash gh-runner
sudo apt-get update && sudo apt-get install -y build-essential curl git
```

**不要用 root，不要把 runner 装在持有 secret 的账户下。**

### 3. 装 Rust（不装默认 toolchain）

```bash
sudo -u gh-runner bash -c 'curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs \
  | sh -s -- -y --profile minimal --default-toolchain none'
```

`--default-toolchain none` 是刻意的：版本由仓库的 `rust-toolchain.toml` 决定，
runner 自己不该持有版本偏好。

> 国内机器访问 `static.rust-lang.org` 很慢，可先设置
> `RUSTUP_DIST_SERVER=https://mirrors.ustc.edu.cn/rust-static`。

### 4. 安装并注册 runner

```bash
sudo -u gh-runner bash -c '
  set -e
  mkdir -p ~/actions-runner && cd ~/actions-runner
  VER=$(curl -s https://api.github.com/repos/actions/runner/releases/latest \
        | grep -o "\"tag_name\": \"v[^\"]*" | cut -d\" -f4 | sed "s/^v//")
  curl -o runner.tar.gz -L \
    "https://github.com/actions/runner/releases/download/v${VER}/actions-runner-linux-x64-${VER}.tar.gz"
  tar xzf runner.tar.gz
  ./config.sh --url https://github.com/<owner>/<repo> --token <TOKEN> \
              --labels qc --name qc-4c4g --unattended --replace
  sudo ./svc.sh install gh-runner && sudo ./svc.sh start
'
```

`--labels qc` 是自定义标签，workflow 用它精确匹配；`--replace` 允许同名 runner 重装。

### 5. 验证

```bash
gh api repos/<owner>/<repo>/actions/runners --jq '.runners[] | {name, status, labels: [.labels[].name]}'
```

应看到 `name: qc-4c4g`、`status: online`、labels 含 `qc`。

## 安全加固（公开仓库必须做）

1. **job 级过滤 fork PR**：
   ```yaml
   if: >-
     github.event_name != 'pull_request' ||
     github.event.pull_request.head.repo.full_name == github.repository
   ```
   没有这一条，任何人 fork 后提 PR 就能在你的机器上执行任意代码。
2. 不使用 `pull_request_target`。
3. `permissions: contents: read` + `checkout` 的 `persist-credentials: false`。
4. 仓库设置 → Actions → 勾选「Require approval for all outside collaborators」。
5. runner 主机不放 deploy key / PAT，不挂载敏感目录。

## 缓存与并发

- `CARGO_TARGET_DIR` 按 job 分目录（`target-clippy` / `target-test`），
  避免同机多个 job 抢同一个 `target/` 导致「拒绝访问 (os error 5)」。
- workflow 里不要开 `cancel-in-progress: true` 对 `test`——中断的 cargo 会留下锁文件。

## 故障排查

| 现象 | 原因 | 处理 |
|---|---|---|
| job 一直 queued | 没有 `qc` 标签的在线 runner | 查 `gh api .../actions/runners`，重启服务 `sudo ./svc.sh restart` |
| runner 刚启动时显示 `offline` | GitHub 侧状态上报有 30~60 秒滞后，日志已出现 `Listening for Jobs` 即为正常 | 等一分钟再查，不要急着重启 |
| `cargo-fmt` 找不到 | runner 的 toolchain 没装 rustfmt | 仓库 `rust-toolchain.toml` 已声明 `components`，确认 runner 用的是仓库内 toolchain |
| 拉取代码私有子模块失败 | 用了 `persist-credentials: false` 且没配 SSH | 改用 deploy key 或改走 https + token（仅限可信仓库） |
| `--locked` 失败 | 本地改了 `Cargo.toml` 但没提交 `Cargo.lock` | 本机跑 `cargo metadata --offline` 刷新 lock 后一起提交 |
