//! 插件契约（UI 无关部分）。
//!
//! 插件（如番茄钟）实现本 trait 以接入核心事件；UI 装配由 app 壳通过
//! feature flag 完成（`--no-default-features` 构建时可整体移除插件）。

use crate::model::Task;

pub trait Plugin: Send + Sync {
    /// 插件唯一标识，如 "pomodoro"
    fn id(&self) -> &'static str;

    /// 展示名称，如 "番茄钟"
    fn display_name(&self) -> &str;

    /// 任务完成时回调（可用于统计等）
    fn on_task_completed(&self, _task: &Task) {}

    /// 任务创建时回调
    fn on_task_created(&self, _task: &Task) {}
}
