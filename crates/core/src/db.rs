use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::error::Result;

/// 数据库结构 DDL（版本 1）
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
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
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
pub fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(DDL)?;
    // schema_meta 初始化
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM schema_meta", [], |r| r.get(0))?;
    if count == 0 {
        conn.execute("INSERT INTO schema_meta(version) VALUES (1)", [])?;
    }
    Ok(())
}
