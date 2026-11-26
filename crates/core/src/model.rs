use serde::{Deserialize, Serialize};

/// 任务优先级（用颜色标记重要程度）：高=红 / 中=橙 / 低=蓝 / 无=灰
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    None,
    Low,
    Medium,
    High,
}

impl Priority {
    pub fn as_i64(self) -> i64 {
        match self {
            Priority::None => 0,
            Priority::Low => 1,
            Priority::Medium => 2,
            Priority::High => 3,
        }
    }

    pub fn from_i64(v: i64) -> Self {
        match v {
            1 => Priority::Low,
            2 => Priority::Medium,
            3 => Priority::High,
            _ => Priority::None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Priority::None => "none",
            Priority::Low => "low",
            Priority::Medium => "medium",
            Priority::High => "high",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Priority::None => "无",
            Priority::Low => "低",
            Priority::Medium => "中",
            Priority::High => "高",
        }
    }

    /// 优先级展示色（hex），高=红 / 中=橙 / 低=蓝 / 无=灰
    pub fn color_hex(self) -> u32 {
        match self {
            Priority::High => 0xE5484D,
            Priority::Medium => 0xF76B15,
            Priority::Low => 0x0091FF,
            Priority::None => 0x8B8D98,
        }
    }

    pub fn all() -> [Priority; 4] {
        [
            Priority::High,
            Priority::Medium,
            Priority::Low,
            Priority::None,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Active,
    Completed,
}

impl TaskStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            TaskStatus::Active => "active",
            TaskStatus::Completed => "completed",
        }
    }

    /// 宽松解析数据库中的状态文本；无法识别时回落到 `Active`（兼容历史脏数据）。
    ///
    /// 命名不用 `from_str`：那是 `std::str::FromStr` 的约定名，固有方法同名会被
    /// clippy 判为 `should_implement_trait`，而本函数语义是可失败的宽松回退，
    /// 与 `FromStr` 必须报错的契约不一致。
    pub fn from_db_str(s: &str) -> Self {
        match s {
            "completed" => TaskStatus::Completed,
            _ => TaskStatus::Active,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vault {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
    pub sort_order: i64,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub vault_id: String,
    pub title: String,
    pub note: Option<String>,
    pub priority: Priority,
    pub status: TaskStatus,
    pub due_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub sort_order: i64,
    pub created_at: i64,
    pub updated_at: i64,
    /// 关联标签（join 后填充，不直接映射到单一列）
    #[serde(default)]
    pub tags: Vec<Tag>,
}

impl Task {
    pub fn is_completed(&self) -> bool {
        self.status == TaskStatus::Completed
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PomodoroKind {
    Focus,
    ShortBreak,
    LongBreak,
}

impl PomodoroKind {
    pub fn as_str(self) -> &'static str {
        match self {
            PomodoroKind::Focus => "focus",
            PomodoroKind::ShortBreak => "short_break",
            PomodoroKind::LongBreak => "long_break",
        }
    }

    /// 宽松解析数据库中的阶段文本；无法识别时回落到 `Focus`（兼容历史脏数据）。
    /// 命名理由同 [`TaskStatus::from_db_str`]。
    pub fn from_db_str(s: &str) -> Self {
        match s {
            "short_break" => PomodoroKind::ShortBreak,
            "long_break" => PomodoroKind::LongBreak,
            _ => PomodoroKind::Focus,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PomodoroSession {
    pub id: String,
    pub task_id: Option<String>,
    pub kind: PomodoroKind,
    pub planned_secs: i64,
    pub actual_secs: i64,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    /// 完整完成（true）vs 中途放弃（false）
    pub finished: bool,
}

/// 当前 unix 秒时间戳
pub fn now_ts() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}

/// 生成 UUID v7 字符串主键
pub fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}
