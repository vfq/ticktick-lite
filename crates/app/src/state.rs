//! 应用级状态辅助：数据目录、首启种子数据。

use std::path::PathBuf;

use ticktick_core::{repo, Database, Result};

/// 数据库文件位置：`%APPDATA%/ticktick-lite/TickTickLite/data/data.db`
pub fn db_path() -> PathBuf {
    if let Some(dirs) = directories::ProjectDirs::from("com", "ticktick-lite", "TickTickLite") {
        return dirs.data_dir().join("data.db");
    }
    PathBuf::from("ticktick-data.db")
}

/// 首次启动时创建默认 vault。
pub fn ensure_seeded(db: &Database) -> Result<()> {
    let conn = db.connect()?;
    if repo::list_vaults(&conn)?.is_empty() {
        repo::create_vault(&conn, "默认", None)?;
    }
    Ok(())
}
