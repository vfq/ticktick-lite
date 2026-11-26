# 滴答 Lite（TickTick Lite）

仿 TickTick 的 Windows 桌面悬浮小窗 Todo 应用，使用 **Rust 1.100 + gpui + GPUI Kit（gpui-component）** 构建，数据存储在本地 SQLite。

## 功能

| 功能 | 说明 |
|---|---|
| 桌面悬浮小窗 | 380×620 无边框小窗，自绘标题栏可拖动；「置顶」按钮切换始终置顶；关闭按钮最小化到系统托盘 |
| 多清单（vault） | 顶部横向 chip 行切换清单（全部 / 各清单 / 新建），chip 上显示该清单进行中任务数，右侧提供重命名与删除 |
| 任务标签 | 任务可带任意多个标签：编辑器里 chip 多选（也可直接输入新标签回车即建即选）；每个标签有稳定颜色（新建时按名字派生，可在设置里改）；点任务行标签即按该标签筛选，顶部出现可移除的筛选条 |
| 任务列表 | 按优先级分组（高 / 中 / 低 / 无），组头带计数；行内左侧色条 + 圆点双编码优先级，右侧显示番茄数与累计专注时长 |
| 设置窗口 | 独立窗口（440×540），三个分区：常规（置顶 / 显示已完成）、番茄钟（专注 / 短休 / 长休 / 长休间隔 / 系统通知）、标签（新建 / 改名 / 改色 / 删除）。改动即时保存并同步到主窗，无「保存」按钮 |
| JSON 导出 | 将指定 vault 的全部**已完成**任务导出为 JSON（`format_version: 2`，标签带颜色；含时间戳、关联番茄钟记录与累计专注时长） |
| 番茄钟插件 | 默认收起的浮层，点标题栏 🍅 展开为右下角卡片；专注 / 短休 / 长休时长与长休间隔均可在设置里改；可从任务行「▶」关联任务发起专注；完成/放弃均落库，阶段结束弹 Windows 系统通知 |
| 系统托盘 | 托盘菜单：显示窗口 / 退出；单击托盘图标还原窗口 |

## 构建与运行

前置条件：Windows + Visual Studio Build Tools（C++ 工作负载）+ Windows SDK。Rust 工具链由 `rust-toolchain.toml` 锁定为 **1.100**。

```bash
cargo run -p ticktick-app          # 调试运行（首次编译 gpui 约需 10+ 分钟）
cargo build --release              # 发布构建
cargo test -p ticktick-core        # 数据层单元测试（不依赖 gpui，秒级）
cargo check --workspace            # 全工作区编译检查（含 app / 插件）
cargo check -p ticktick-app --no-default-features   # 无番茄钟插件构建
```

> 本仓库的 CI 只覆盖**跨平台**部分（`cargo fmt` / core 的 clippy 与测试 / docs 校验），
> app 与插件依赖 gpui 的 Windows 后端，只能在 Windows 上编译。原因与取舍见
> `docs/adr/ADR-0002-ci-self-hosted-qc.md`。

数据文件位置：`%APPDATA%\ticktick-lite\TickTickLite\data\data.db`

## 导出 JSON 格式

```json
{
  "app": "ticktick-lite",
  "format_version": 2,
  "exported_at": "2026-11-02T09:30:00+08:00",
  "vault": { "id": "019a1c2e-…", "name": "工作" },
  "task_count": 1,
  "tasks": [
    {
      "id": "019a1c2f-…",
      "title": "写季度汇报",
      "note": "附 Q3 数据",
      "priority": "high",
      "tags": [
        { "name": "汇报", "color": "#2563EB" },
        { "name": "文档", "color": "#8E4EC6" }
      ],
      "created_at": "2026-10-28T10:00:00+08:00",
      "completed_at": "2026-11-01T16:40:00+08:00",
      "due_at": null,
      "pomodoros": [
        { "started_at": "2026-11-01T15:00:00+08:00", "planned_secs": 1500, "actual_secs": 1500, "finished": true }
      ],
      "total_focus_secs": 1500
    }
  ]
}
```

`format_version` 演进：`1` = `tags` 为字符串数组、无颜色；`2` = `tags` 为 `{name, color}` 对象数组（`#RRGGBB`）。
读取方按版本分支解析。

## 项目结构

```
crates/
  core/      # 纯 Rust 核心：数据模型、SQLite schema、repo CRUD、JSON 导出、插件契约（可独立单测）
  app/       # gpui 应用壳：窗口、UI、托盘、置顶、覆盖层；通过 feature `pomodoro` 装配插件
plugins/
  pomodoro/  # 番茄钟插件：状态机 + 面板 UI + session 落库（--no-default-features 可整体移除）
```

## 设计说明

- **依赖策略**：单一依赖 `gpui-kit = "0.8.1"`（内部精确锁定 gpui-pre 快照），规避 gpui git rev 对齐问题
- **置顶实现**：gpui 无原生 always-on-top，经 raw-window-handle 取 HWND 后 `SetWindowPos(HWND_TOPMOST)` 实现
- **设置单一真相**：`SettingsStore` 是一个 `Entity`，主窗与设置窗共享同一实例；任何一侧 `edit()` 都走「内存生效 → 钳制 → 落库 → `emit`」，另一侧订阅后立即同步
- **界面规范**：`design/ui-spec.html`（可直接在浏览器打开）——含窗口信息层级、设计令牌（色板 / 字阶 / 间距 / 圆角）、交互态与可访问性要求，以及设置窗口与番茄钟浮层的形态；代码侧令牌集中在 `crates/app/src/theme_ext.rs`

### 界面设计要点

- **380px 窄窗的取舍**：放弃左侧栏（会挤占任务区），改为顶部横向 chip 行切换清单，任务区获得完整宽度
- **优先级双编码**：色条（位置 + 颜色）之外再加组头文字与圆点，避免仅靠颜色区分（色觉障碍可用）
- **间距 / 圆角**：4px 基准刻度（4/8/12/16/24），控件 6px、卡片 12px、chip 全圆角
- **番茄钟收口**：不占常驻区域——默认收起，点标题栏 🍅 才以右下角 320px 浮层展开；收起不打断计时，倒计时改在标题栏显示
- **配置外置**：低频配置（置顶、显示已完成、番茄钟四项时长、标签管理）不挤占小窗，全部收进独立设置窗口
- **托盘实现**：`tray-icon` crate + gpui 前台 executor 轮询桥（托盘代码隔离在 `tray.rs`）
- **DB 访问**：`Database` 为路径包装，按需新建连接（WAL），可安全跨线程克隆
