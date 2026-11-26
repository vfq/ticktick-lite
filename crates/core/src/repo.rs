//! 数据访问层：vault / task / tag / pomodoro / settings 的 CRUD。

use std::collections::HashMap;

use rusqlite::{Connection, params};

use crate::error::{Error, Result};
use crate::model::{
    PomodoroKind, PomodoroSession, Priority, Tag, Task, TaskStatus, Vault, new_id, now_ts,
};

// ---------------------------------------------------------------------------
// Vault
// ---------------------------------------------------------------------------

pub fn create_vault(conn: &Connection, name: &str, color: Option<&str>) -> Result<Vault> {
    let vault = Vault {
        id: new_id(),
        name: name.trim().to_string(),
        color: color.map(|s| s.to_string()),
        sort_order: 0,
        created_at: now_ts(),
    };
    conn.execute(
        "INSERT INTO vaults(id, name, color, sort_order, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            vault.id,
            vault.name,
            vault.color,
            vault.sort_order,
            vault.created_at
        ],
    )?;
    Ok(vault)
}

pub fn list_vaults(conn: &Connection) -> Result<Vec<Vault>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, color, sort_order, created_at FROM vaults ORDER BY sort_order, created_at",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Vault {
            id: r.get(0)?,
            name: r.get(1)?,
            color: r.get(2)?,
            sort_order: r.get(3)?,
            created_at: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

pub fn get_vault(conn: &Connection, id: &str) -> Result<Vault> {
    conn.query_row(
        "SELECT id, name, color, sort_order, created_at FROM vaults WHERE id = ?1",
        params![id],
        |r| {
            Ok(Vault {
                id: r.get(0)?,
                name: r.get(1)?,
                color: r.get(2)?,
                sort_order: r.get(3)?,
                created_at: r.get(4)?,
            })
        },
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Error::NotFound(format!("vault {id}")),
        other => Error::Db(other),
    })
}

pub fn rename_vault(conn: &Connection, id: &str, name: &str) -> Result<()> {
    conn.execute(
        "UPDATE vaults SET name = ?2 WHERE id = ?1",
        params![id, name.trim()],
    )?;
    Ok(())
}

pub fn delete_vault(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM vaults WHERE id = ?1", params![id])?;
    Ok(())
}

/// 各 vault 的进行中任务数（用于侧栏计数），key 为 vault_id。
pub fn active_counts_by_vault(conn: &Connection) -> Result<HashMap<String, i64>> {
    let mut stmt = conn.prepare(
        "SELECT vault_id, COUNT(*) FROM tasks WHERE status = 'active' GROUP BY vault_id",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
    Ok(rows.collect::<std::result::Result<HashMap<_, _>, _>>()?)
}

pub fn total_active_count(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM tasks WHERE status = 'active'",
        [],
        |r| r.get(0),
    )?)
}

// ---------------------------------------------------------------------------
// Task
// ---------------------------------------------------------------------------

/// 任务查询过滤器：`vault_id` 为 None 表示全部 vault。
#[derive(Debug, Clone, Default)]
pub struct TaskFilter {
    pub vault_id: Option<String>,
    /// None = 全部状态；Some(Active) = 仅进行中；Some(Completed) = 仅已完成
    pub status: Option<TaskStatus>,
}

fn task_from_row(r: &rusqlite::Row) -> rusqlite::Result<Task> {
    let priority: i64 = r.get(4)?;
    let status: String = r.get(5)?;
    Ok(Task {
        id: r.get(0)?,
        vault_id: r.get(1)?,
        title: r.get(2)?,
        note: r.get(3)?,
        priority: Priority::from_i64(priority),
        status: TaskStatus::from_db_str(&status),
        due_at: r.get(6)?,
        completed_at: r.get(7)?,
        sort_order: r.get(8)?,
        created_at: r.get(9)?,
        updated_at: r.get(10)?,
        tags: Vec::new(),
    })
}

const TASK_COLS: &str = "id, vault_id, title, note, priority, status, due_at, completed_at, sort_order, created_at, updated_at";

pub struct NewTask {
    pub vault_id: String,
    pub title: String,
    pub note: Option<String>,
    pub priority: Priority,
    pub due_at: Option<i64>,
    pub tag_names: Vec<String>,
}

pub fn create_task(conn: &Connection, input: NewTask) -> Result<Task> {
    let now = now_ts();
    let task = Task {
        id: new_id(),
        vault_id: input.vault_id,
        title: input.title.trim().to_string(),
        note: input.note.filter(|n| !n.trim().is_empty()),
        priority: input.priority,
        status: TaskStatus::Active,
        due_at: input.due_at,
        completed_at: None,
        sort_order: 0,
        created_at: now,
        updated_at: now,
        tags: Vec::new(),
    };
    conn.execute(
        &format!("INSERT INTO tasks({TASK_COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)"),
        params![
            task.id,
            task.vault_id,
            task.title,
            task.note,
            task.priority.as_i64(),
            task.status.as_str(),
            task.due_at,
            task.completed_at,
            task.sort_order,
            task.created_at,
            task.updated_at,
        ],
    )?;
    if !input.tag_names.is_empty() {
        let ids = ensure_tags(conn, &input.tag_names)?;
        set_task_tags(conn, &task.id, &ids)?;
    }
    get_task(conn, &task.id)
}

pub fn get_task(conn: &Connection, id: &str) -> Result<Task> {
    let mut task = conn
        .query_row(
            &format!("SELECT {TASK_COLS} FROM tasks WHERE id = ?1"),
            params![id],
            task_from_row,
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Error::NotFound(format!("task {id}")),
            other => Error::Db(other),
        })?;
    let tags = tags_for_tasks(conn, std::slice::from_ref(&task.id))?;
    task.tags = tags.into_values().next().unwrap_or_default();
    Ok(task)
}

/// 列出任务并按 状态 → 优先级(高→低) → 创建时间 排序，附带标签。
pub fn list_tasks(conn: &Connection, filter: &TaskFilter) -> Result<Vec<Task>> {
    let mut sql = format!("SELECT {TASK_COLS} FROM tasks WHERE 1=1");
    let mut values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    if let Some(vault_id) = &filter.vault_id {
        sql.push_str(" AND vault_id = ?");
        values.push(Box::new(vault_id.clone()));
    }
    if let Some(status) = filter.status {
        sql.push_str(" AND status = ?");
        values.push(Box::new(status.as_str().to_string()));
    }
    sql.push_str(
        " ORDER BY CASE status WHEN 'active' THEN 0 ELSE 1 END, priority DESC, created_at ASC",
    );
    let params: Vec<&dyn rusqlite::types::ToSql> = values.iter().map(|b| b.as_ref()).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params.as_slice(), task_from_row)?;
    let mut tasks: Vec<Task> = rows.collect::<std::result::Result<Vec<_>, _>>()?;
    if !tasks.is_empty() {
        let ids: Vec<String> = tasks.iter().map(|t| t.id.clone()).collect();
        let tag_map = tags_for_tasks(conn, &ids)?;
        for t in &mut tasks {
            if let Some(tags) = tag_map.get(&t.id) {
                t.tags = tags.clone();
            }
        }
    }
    Ok(tasks)
}

// TODO(P2)：参数已达 8 个，P2 改造标签时收敛为 `TaskEdit` 结构体传参，
// 届时去掉这条 allow。当前保留 allow 是为了不在 P0 阶段牵动 app 侧调用点
// （app 无编译验证通道，见 ADR-0002）。
#[allow(clippy::too_many_arguments)]
pub fn update_task_details(
    conn: &Connection,
    id: &str,
    title: &str,
    note: Option<&str>,
    priority: Priority,
    due_at: Option<i64>,
    vault_id: &str,
    tag_names: &[String],
) -> Result<Task> {
    conn.execute(
        "UPDATE tasks SET title = ?2, note = ?3, priority = ?4, due_at = ?5, vault_id = ?6, updated_at = ?7 WHERE id = ?1",
        params![
            id,
            title.trim(),
            note.filter(|n| !n.trim().is_empty()),
            priority.as_i64(),
            due_at,
            vault_id,
            now_ts()
        ],
    )?;
    let ids = ensure_tags(conn, tag_names)?;
    set_task_tags(conn, id, &ids)?;
    get_task(conn, id)
}

pub fn set_task_status(conn: &Connection, id: &str, status: TaskStatus) -> Result<()> {
    let completed_at: Option<i64> = match status {
        TaskStatus::Completed => Some(now_ts()),
        TaskStatus::Active => None,
    };
    conn.execute(
        "UPDATE tasks SET status = ?2, completed_at = ?3, updated_at = ?4 WHERE id = ?1",
        params![id, status.as_str(), completed_at, now_ts()],
    )?;
    Ok(())
}

pub fn delete_task(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM tasks WHERE id = ?1", params![id])?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tag
// ---------------------------------------------------------------------------

pub fn list_tags(conn: &Connection) -> Result<Vec<Tag>> {
    let mut stmt = conn.prepare("SELECT id, name, color FROM tags ORDER BY name")?;
    let rows = stmt.query_map([], |r| {
        Ok(Tag {
            id: r.get(0)?,
            name: r.get(1)?,
            color: r.get(2)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

/// 按名称确保标签存在（不存在则创建），返回标签 id 列表（顺序与输入一致，自动去重）。
pub fn ensure_tags(conn: &Connection, names: &[String]) -> Result<Vec<String>> {
    let mut ids = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for name in names {
        let name = name.trim();
        if name.is_empty() || !seen.insert(name.to_string()) {
            continue;
        }
        let existing: Option<String> = conn
            .query_row("SELECT id FROM tags WHERE name = ?1", params![name], |r| {
                r.get(0)
            })
            .ok();
        let id = match existing {
            Some(id) => id,
            None => {
                let id = new_id();
                conn.execute(
                    "INSERT INTO tags(id, name, color) VALUES (?1, ?2, NULL)",
                    params![id, name],
                )?;
                id
            }
        };
        ids.push(id);
    }
    Ok(ids)
}

pub fn set_task_tags(conn: &Connection, task_id: &str, tag_ids: &[String]) -> Result<()> {
    conn.execute("DELETE FROM task_tags WHERE task_id = ?1", params![task_id])?;
    for tag_id in tag_ids {
        conn.execute(
            "INSERT OR IGNORE INTO task_tags(task_id, tag_id) VALUES (?1, ?2)",
            params![task_id, tag_id],
        )?;
    }
    Ok(())
}

/// 批量查询任务的标签，避免 N+1。
pub fn tags_for_tasks(conn: &Connection, task_ids: &[String]) -> Result<HashMap<String, Vec<Tag>>> {
    let mut map: HashMap<String, Vec<Tag>> = HashMap::new();
    if task_ids.is_empty() {
        return Ok(map);
    }
    let placeholders = task_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT tt.task_id, t.id, t.name, t.color FROM task_tags tt \
         JOIN tags t ON t.id = tt.tag_id WHERE tt.task_id IN ({placeholders}) ORDER BY t.name"
    );
    let params: Vec<&dyn rusqlite::types::ToSql> = task_ids
        .iter()
        .map(|id| id as &dyn rusqlite::types::ToSql)
        .collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params.as_slice(), |r| {
        Ok((
            r.get::<_, String>(0)?,
            Tag {
                id: r.get(1)?,
                name: r.get(2)?,
                color: r.get(3)?,
            },
        ))
    })?;
    for row in rows {
        let (task_id, tag) = row?;
        map.entry(task_id).or_default().push(tag);
    }
    Ok(map)
}

// ---------------------------------------------------------------------------
// Pomodoro
// ---------------------------------------------------------------------------

pub fn insert_pomodoro_session(conn: &Connection, session: &PomodoroSession) -> Result<()> {
    conn.execute(
        "INSERT INTO pomodoro_sessions(id, task_id, kind, planned_secs, actual_secs, started_at, ended_at, finished) \
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        params![
            session.id,
            session.task_id,
            session.kind.as_str(),
            session.planned_secs,
            session.actual_secs,
            session.started_at,
            session.ended_at,
            session.finished as i64,
        ],
    )?;
    Ok(())
}

pub fn finish_pomodoro_session(
    conn: &Connection,
    id: &str,
    actual_secs: i64,
    ended_at: i64,
    finished: bool,
) -> Result<()> {
    conn.execute(
        "UPDATE pomodoro_sessions SET actual_secs = ?2, ended_at = ?3, finished = ?4 WHERE id = ?1",
        params![id, actual_secs, ended_at, finished as i64],
    )?;
    Ok(())
}

/// 各任务的累计已完成功时长（秒），key 为 task_id。
pub fn focus_totals(conn: &Connection, task_ids: &[String]) -> Result<HashMap<String, i64>> {
    let mut map = HashMap::new();
    if task_ids.is_empty() {
        return Ok(map);
    }
    let placeholders = task_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT task_id, SUM(actual_secs) FROM pomodoro_sessions \
         WHERE kind = 'focus' AND finished = 1 AND task_id IN ({placeholders}) GROUP BY task_id"
    );
    let params: Vec<&dyn rusqlite::types::ToSql> = task_ids
        .iter()
        .map(|id| id as &dyn rusqlite::types::ToSql)
        .collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params.as_slice(), |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
    })?;
    for row in rows {
        let (id, secs) = row?;
        map.insert(id, secs);
    }
    Ok(map)
}

/// 批量查询任务关联的番茄记录，避免 N+1。
pub fn pomodoros_for_tasks(
    conn: &Connection,
    task_ids: &[String],
) -> Result<HashMap<String, Vec<PomodoroSession>>> {
    let mut map: HashMap<String, Vec<PomodoroSession>> = HashMap::new();
    if task_ids.is_empty() {
        return Ok(map);
    }
    let placeholders = task_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT id, task_id, kind, planned_secs, actual_secs, started_at, ended_at, finished \
         FROM pomodoro_sessions WHERE task_id IN ({placeholders}) ORDER BY started_at"
    );
    let params: Vec<&dyn rusqlite::types::ToSql> = task_ids
        .iter()
        .map(|id| id as &dyn rusqlite::types::ToSql)
        .collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params.as_slice(), |r| {
        let kind: String = r.get(2)?;
        Ok(PomodoroSession {
            id: r.get(0)?,
            task_id: r.get(1)?,
            kind: PomodoroKind::from_db_str(&kind),
            planned_secs: r.get(3)?,
            actual_secs: r.get(4)?,
            started_at: r.get(5)?,
            ended_at: r.get(6)?,
            finished: r.get::<_, i64>(7)? != 0,
        })
    })?;
    for row in rows {
        let session = row?;
        if let Some(task_id) = session.task_id.clone() {
            map.entry(task_id).or_default().push(session);
        }
    }
    Ok(map)
}

// ---------------------------------------------------------------------------
// Settings (KV)
// ---------------------------------------------------------------------------

pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>> {
    let value = conn
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |r| r.get(0),
        )
        .ok();
    Ok(value)
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO settings(key, value) VALUES (?1, ?2) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}
