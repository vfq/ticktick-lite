//! 数据访问层：vault / task / tag / pomodoro / settings 的 CRUD。

use std::collections::HashMap;

use rusqlite::{Connection, params};

use crate::error::{Error, Result};
use crate::model::{
    PomodoroKind, PomodoroSession, Priority, Tag, Task, TaskStatus, Vault, default_tag_color,
    new_id, now_ts, tag_color_or_default,
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
    /// 标签筛选，语义为 **OR（并集）**：命中任一标签即入选。
    ///
    /// 空 Vec 表示不按标签过滤。用 `EXISTS` 而非 JOIN，天然避免一个任务命中
    /// 多个选中标签时重复出现，也就不需要 `DISTINCT`。
    pub tag_ids: Vec<String>,
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
    if !filter.tag_ids.is_empty() {
        let marks = filter
            .tag_ids
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        sql.push_str(&format!(
            " AND EXISTS (SELECT 1 FROM task_tags tt \
             WHERE tt.task_id = tasks.id AND tt.tag_id IN ({marks}))"
        ));
        for id in &filter.tag_ids {
            values.push(Box::new(id.clone()));
        }
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

/// 任务编辑的入参集合。
///
/// 参数超过 5 个后逐位传参极易错位（`due_at` 与 `vault_id` 类型不同还好，
/// 将来加同类型字段就难查了），统一收进结构体，调用侧用字段名表达意图。
pub struct TaskEdit<'a> {
    pub title: &'a str,
    pub note: Option<&'a str>,
    pub priority: Priority,
    pub due_at: Option<i64>,
    pub vault_id: &'a str,
    /// 目标标签名集合，覆盖式写入（空切片 = 清空标签）
    pub tag_names: &'a [String],
}

pub fn update_task(conn: &Connection, id: &str, edit: TaskEdit<'_>) -> Result<Task> {
    conn.execute(
        "UPDATE tasks SET title = ?2, note = ?3, priority = ?4, due_at = ?5, vault_id = ?6, updated_at = ?7 WHERE id = ?1",
        params![
            id,
            edit.title.trim(),
            edit.note.filter(|n| !n.trim().is_empty()),
            edit.priority.as_i64(),
            edit.due_at,
            edit.vault_id,
            now_ts()
        ],
    )?;
    let ids = ensure_tags(conn, edit.tag_names)?;
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

/// 标签列表附带任务计数，供设置页展示「哪些标签在用」。
#[derive(Debug, Clone)]
pub struct TagCount {
    pub tag: Tag,
    /// 挂在该标签下的进行中任务数
    pub active: i64,
    /// 挂在该标签下的任务总数（含已完成）
    pub total: i64,
}

/// 记录某标签名已存在时，返回带名字的冲突错误（UI 直接展示这句）。
fn tag_name_taken(name: &str) -> Error {
    Error::Conflict(format!("标签「{name}」已存在"))
}

fn ensure_tag_name_free(conn: &Connection, name: &str, except_id: Option<&str>) -> Result<()> {
    let taken: i64 = conn.query_row(
        "SELECT COUNT(*) FROM tags WHERE name = ?1 AND id <> COALESCE(?2, '')",
        params![name, except_id],
        |r| r.get(0),
    )?;
    if taken > 0 {
        return Err(tag_name_taken(name));
    }
    Ok(())
}

/// 新建标签。颜色缺省时按名称派生，因此新建的标签立刻就有颜色。
pub fn create_tag(conn: &Connection, name: &str, color: Option<&str>) -> Result<Tag> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::Other("标签名不能为空".into()));
    }
    ensure_tag_name_free(conn, name, None)?;
    let tag = Tag {
        id: new_id(),
        name: name.to_string(),
        color: Some(tag_color_or_default(color, name)),
    };
    conn.execute(
        "INSERT INTO tags(id, name, color) VALUES (?1, ?2, ?3)",
        params![tag.id, tag.name, tag.color],
    )?;
    Ok(tag)
}

pub fn rename_tag(conn: &Connection, id: &str, name: &str) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::Other("标签名不能为空".into()));
    }
    ensure_tag_name_free(conn, name, Some(id))?;
    let affected = conn.execute("UPDATE tags SET name = ?2 WHERE id = ?1", params![id, name])?;
    if affected == 0 {
        return Err(Error::NotFound(format!("tag {id}")));
    }
    Ok(())
}

/// 设置标签颜色；传 `None` 表示恢复为名称派生色。
pub fn set_tag_color(conn: &Connection, id: &str, color: Option<&str>) -> Result<()> {
    let name: String = conn
        .query_row("SELECT name FROM tags WHERE id = ?1", params![id], |r| {
            r.get(0)
        })
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Error::NotFound(format!("tag {id}")),
            other => Error::Db(other),
        })?;
    conn.execute(
        "UPDATE tags SET color = ?2 WHERE id = ?1",
        params![id, tag_color_or_default(color, &name)],
    )?;
    Ok(())
}

/// 删除标签。`task_tags` 的外键是 `ON DELETE CASCADE`，关联会自动清理，
/// 任务本身保留（连接已开 `PRAGMA foreign_keys`）。
pub fn delete_tag(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM tags WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn list_tags_with_counts(conn: &Connection) -> Result<Vec<TagCount>> {
    let mut stmt = conn.prepare(
        "SELECT t.id, t.name, t.color,
                COALESCE(SUM(CASE WHEN tk.status = 'active' THEN 1 ELSE 0 END), 0),
                COUNT(tk.id)
         FROM tags t
         LEFT JOIN task_tags tt ON tt.tag_id = t.id
         LEFT JOIN tasks tk ON tk.id = tt.task_id
         GROUP BY t.id, t.name, t.color
         ORDER BY t.name",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(TagCount {
            tag: Tag {
                id: r.get(0)?,
                name: r.get(1)?,
                color: r.get(2)?,
            },
            active: r.get(3)?,
            total: r.get(4)?,
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
                    "INSERT INTO tags(id, name, color) VALUES (?1, ?2, ?3)",
                    params![id, name, default_tag_color(name)],
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
