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

/// 标签调色板：8 色，人工可区分且不需要滚动选择。
///
/// 前 4 色复用 [`Priority`] 的语义色系，保证优先级与标签处在同一视觉语言里。
pub const TAG_PALETTE: [u32; 8] = [
    0x2563EB, // 蓝
    0x30A46C, // 绿
    0xE5484D, // 红
    0xF76B15, // 橙
    0x8E4EC6, // 紫
    0x0091FF, // 亮蓝
    0xD6405F, // 玫红
    0x6B7280, // 灰
];

/// 颜色解析与派生全部失败时的最后兜底（中性灰）。
pub const TAG_COLOR_FALLBACK: u32 = 0x6B7280;

/// 解析 `#RRGGBB` 或 `RRGGBB`（大小写均可）为 `0xRRGGBB`。
///
/// 只接受恰好 6 位十六进制，不做 `#RGB` 之类的简写展开——数据库里只写规范形式，
/// 出现简写说明是脏数据，交给调用方回落默认色。
pub fn parse_hex_color(s: &str) -> Option<u32> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(s, 16).ok()
}

/// 格式化为 `#RRGGBB`（大写），只取低 24 位。
pub fn color_to_hex(c: u32) -> String {
    format!("#{:06X}", c & 0x00FF_FFFF)
}

/// 由标签名派生默认色：FNV-1a 哈希落调色板，**保证同名同色**。
///
/// 幂等且与机器无关，因此两台独立迁移的数据库对同一标签名会得到同一颜色。
pub fn default_tag_color(name: &str) -> String {
    let mut h: u32 = 0x811C_9DC5;
    for b in name.as_bytes() {
        h ^= u32::from(*b);
        h = h.wrapping_mul(0x0100_0193);
    }
    color_to_hex(TAG_PALETTE[(h as usize) % TAG_PALETTE.len()])
}

/// 把外部传入的颜色规范化为 `#RRGGBB`；缺失或非法时回落到名称派生色。
///
/// 写入路径统一走这里，因此数据库里不会出现 `null` / `red` / `#abc` 这类值。
pub fn tag_color_or_default(color: Option<&str>, name: &str) -> String {
    color
        .and_then(parse_hex_color)
        .map(color_to_hex)
        .unwrap_or_else(|| default_tag_color(name))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
}

impl Tag {
    /// 展示用颜色：优先解析存储值，失败回落名称派生色，再失败回落中性灰。
    ///
    /// 展示层不允许因为一条脏数据而渲染异常或 panic——非法值降级，不报错。
    pub fn color_hex(&self) -> u32 {
        self.color
            .as_deref()
            .and_then(parse_hex_color)
            .or_else(|| parse_hex_color(&default_tag_color(&self.name)))
            .unwrap_or(TAG_COLOR_FALLBACK)
    }

    /// 展示用颜色（`#RRGGBB`），用于导出与跨层传递。
    pub fn color_hex_string(&self) -> String {
        color_to_hex(self.color_hex())
    }
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
