//! TickTick Lite 核心库：数据模型、SQLite 持久化、JSON 导出与插件契约。
//!
//! 本 crate 不依赖 gpui，可独立、快速地进行单元测试。

pub mod db;
pub mod error;
pub mod export;
pub mod model;
pub mod plugin;
pub mod repo;

pub use db::Database;
pub use error::{Error, Result};
pub use model::{PomodoroKind, PomodoroSession, Priority, Tag, Task, TaskStatus, Vault};
pub use plugin::Plugin;
