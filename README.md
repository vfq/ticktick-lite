# 滴答 Lite（TickTick Lite）

仿 TickTick 的 Windows 桌面悬浮小窗 Todo 应用，使用 **Rust 1.100 + gpui + GPUI Kit（gpui-component）** 构建，数据存储在本地 SQLite。

## 功能

| 功能 | 说明 |
|---|---|
| 桌面悬浮小窗 | 380×620 无边框小窗，自绘标题栏可拖动；「置顶」按钮切换始终置顶；关闭按钮最小化到系统托盘 |
| 多清单（vault） | 顶部横向 chip 行切换清单（全部 / 各清单 / 新建），chip 上显示该清单进行中任务数，右侧提供重命名与删除 |
| 任务标签 | 任务可带任意多个标签（逗号分隔输入，自动去重创建） |
| 任务列表 | 按优先级分组（高 / 中 / 低 / 无），组头带计数；行内左侧色条 + 圆点双编码优先级，右侧显示番茄数与累计专注时长 |
| JSON 导出 | 将指定 vault 的全部**已完成**任务导出为 JSON（含标签、时间戳、关联番茄钟记录与累计专注时长） |
| 番茄钟插件 | 25 分钟专注 + 5 分钟短休，每 4 轮后 15 分钟长休；可从任务行「▶」关联任务发起专注；完成/放弃均落库，完成时 Windows 通知提醒 |
| 系统托盘 | 托盘菜单：显示窗口 / 退出；单击托盘图标还原窗口 |

## 构建与运行

前置条件：Windows + Visual Studio Build Tools（C++ 工作负载）+ Windows SDK。Rust 工具链由 `rust-toolchain.toml` 锁定为 **1.100**。

```bash
cargo run -p ticktick-app          # 调试运行（首次编译 gpui 约需 10+ 分钟）
cargo build --release              # 发布构建
cargo test -p ticktick-core        # 数据层单元测试（不依赖 gpui，秒级）
cargo check --no-default-features -p ticktick-app   # 无番茄钟插件构建
```

数据文件位置：`%APPDATA%\ticktick-lite\TickTickLite\data\data.db`

## 导出 JSON 格式

```json
{
  "app": "ticktick-lite",
  "format_version": 1,
  "exported_at": "2026-11-02T09:30:00+08:00",
  "vault": { "id": "019a1c2e-…", "name": "工作" },
  "task_count": 1,
  "tasks": [
    {
      "id": "019a1c2f-…",
      "title": "写季度汇报",
      "note": "附 Q3 数据",
      "priority": "high",
      "tags": ["汇报", "文档"],
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
- **界面规范**：`design/ui-spec.html`（可直接在浏览器打开）——含窗口信息层级、设计令牌（色板 / 字阶 / 间距 / 圆角）、交互态与可访问性要求；代码侧令牌集中在 `crates/app/src/theme_ext.rs`

### 界面设计要点

- **380px 窄窗的取舍**：放弃左侧栏（会挤占任务区），改为顶部横向 chip 行切换清单，任务区获得完整宽度
- **优先级双编码**：色条（位置 + 颜色）之外再加组头文字与圆点，避免仅靠颜色区分（色觉障碍可用）
- **间距 / 圆角**：4px 基准刻度（4/8/12/16/24），控件 6px、卡片 12px、chip 全圆角
- **番茄钟收口**：不占常驻区域，从任务行「▶」发起，面板以覆盖层出现，结束后自动落库并通知
- **托盘实现**：`tray-icon` crate + gpui 前台 executor 轮询桥（托盘代码隔离在 `tray.rs`）
- **DB 访问**：`Database` 为路径包装，按需新建连接（WAL），可安全跨线程克隆
