//! 番茄钟插件：25/5 分钟循环（每 4 轮后 15 分钟长休），可关联任务，
//! 专注时段完成后写入 `pomodoro_sessions` 并通知 app 刷新统计。
//!
//! 作为独立 crate 由 app 通过 feature `pomodoro` 装配；
//! `cargo build --no-default-features` 时整体移除。

mod panel;

pub use panel::{PomodoroEvent, PomodoroPanel};

use ticktick_core::Plugin;

/// UI 无关的插件契约实现（核心事件钩子入口）。
pub struct PomodoroPlugin;

impl Plugin for PomodoroPlugin {
    fn id(&self) -> &'static str {
        "pomodoro"
    }

    fn display_name(&self) -> &str {
        "番茄钟"
    }
}
