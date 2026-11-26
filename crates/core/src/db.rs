use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};

use crate::error::Result;
use crate::model::default_tag_color;

/// 当前数据库结构版本。
///
/// - v1：初版（vaults / tags / tasks / task_tags / pomodoro_sessions / settings）
/// - v2：新增 `idx_task_tags_tag_id`（按标签反查任务不再全表扫）+ 回填历史标签颜色
pub const SCHEMA_VERSION: i64 = 2;

/// 数据库结构 DDL（始终是最新版本，`IF NOT EXISTS` 保证可重复执行）
pub const DDL: &str = r#"
CREATE TABLE IF NOT EXISTS schema_meta (
    version INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS vaults (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    color       TEXT,
    sort_order  INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS tags (
    id      TEXT PRIMARY KEY,
    name    TEXT NOT NULL UNIQUE,
    color   TEXT
);

CREATE TABLE IF NOT EXISTS tasks (
    id            TEXT PRIMARY KEY,
    vault_id      TEXT NOT NULL REFERENCES vaults(id) ON DELETE CASCADE,
    title         TEXT NOT NULL,
    note          TEXT,
    priority      INTEGER NOT NULL DEFAULT 0 CHECK (priority IN (0,1,2,3)),
    status        TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active','completed')),
    due_at        INTEGER,
    completed_at  INTEGER,
    sort_order    INTEGER NOT NULL DEFAULT 0,
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_tasks_vault_status ON tasks(vault_id, status);
CREATE INDEX IF NOT EXISTS idx_tasks_completed_at ON tasks(completed_at);

CREATE TABLE IF NOT EXISTS task_tags (
    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    tag_id  TEXT NOT NULL REFERENCES tags(id)  ON DELETE CASCADE,
    PRIMARY KEY (task_id, tag_id)
);
CREATE INDEX IF NOT EXISTS idx_task_tags_tag_id ON task_tags(tag_id);

CREATE TABLE IF NOT EXISTS pomodoro_sessions (
    id            TEXT PRIMARY KEY,
    task_id       TEXT REFERENCES tasks(id) ON DELETE SET NULL,
    kind          TEXT NOT NULL CHECK (kind IN ('focus','short_break','long_break')),
    planned_secs  INTEGER NOT NULL,
    actual_secs   INTEGER NOT NULL DEFAULT 0,
    started_at    INTEGER NOT NULL,
    ended_at      INTEGER,
    finished      INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_pomodoro_task ON pomodoro_sessions(task_id);

CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#;

/// SQLite 数据库句柄（轻量路径包装）。
///
/// rusqlite 的 `Connection` 不可跨线程共享，因此这里保存路径、
/// 每次操作时用 [`Database::connect`] 新建连接（WAL 模式下开销极小），
/// 这样 `Database` 本身可以安全地在线程间移动与克隆。
#[derive(Debug, Clone)]
pub struct Database {
    path: PathBuf,
}

impl Database {
    /// 打开（必要时创建并初始化）指定路径的数据库。
    pub fn new(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        // edition 2024 的 let-chain：父目录存在且不是空字符串（纯文件名）才创建
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        let db = Self { path };
        let conn = db.connect()?;
        migrate(&conn)?;
        Ok(db)
    }

    /// 新建一个到同一数据库的连接。
    pub fn connect(&self) -> Result<Connection> {
        let conn = Connection::open(&self.path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "busy_timeout", 5000_i64)?;
        Ok(conn)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// 执行结构迁移（幂等）。
///
/// `DDL` 用 `IF NOT EXISTS` 把库补到最新形态，`schema_meta` 记录版本；
/// 需要用 Rust 代码处理的数据迁移（如 v2 的颜色回填）按版本号逐级补齐。
pub fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(DDL)?;

    // 空表时 `MAX(version)` 返回一行 NULL，因此这里拿到的是 Option 而不是 NoRows。
    let current: Option<i64> =
        conn.query_row("SELECT MAX(version) FROM schema_meta", [], |r| r.get(0))?;

    match current {
        None => {
            conn.execute(
                "INSERT INTO schema_meta(version) VALUES (?1)",
                params![SCHEMA_VERSION],
            )?;
        }
        Some(v) if v < SCHEMA_VERSION => {
            if v < 2 {
                migrate_to_v2(conn)?;
            }
            // 原地更新，保持 schema_meta 只有一行（该表无主键约束，INSERT 会累积垃圾行）
            conn.execute(
                "UPDATE schema_meta SET version = ?1",
                params![SCHEMA_VERSION],
            )?;
        }
        Some(_) => {}
    }
    Ok(())
}

/// v2：为 `task_tags` 建反向索引，并回填历史标签颜色。
fn migrate_to_v2(conn: &Connection) -> Result<()> {
    conn.execute_batch("CREATE INDEX IF NOT EXISTS idx_task_tags_tag_id ON task_tags(tag_id);")?;
    backfill_tag_colors(conn)
}

/// 给 `color` 为 NULL / 空串的标签按名称派生一个稳定颜色。
///
/// 只处理缺失值 → 幂等；已有颜色（包括用户手选的）一律不动。
fn backfill_tag_colors(conn: &Connection) -> Result<()> {
    let pending: Vec<(String, String)> = {
        let mut stmt =
            conn.prepare("SELECT id, name FROM tags WHERE color IS NULL OR color = ''")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()?
    };
    for (id, name) in pending {
        conn.execute(
            "UPDATE tags SET color = ?2 WHERE id = ?1",
            params![id, default_tag_color(&name)],
        )?;
    }
    Ok(())
}
