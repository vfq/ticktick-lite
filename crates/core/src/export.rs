//! JSON 导出：按 vault 导出已完成任务（含标签与番茄钟记录）。

use rusqlite::Connection;
use serde::Serialize;
use time::format_description::well_known::Rfc3339;
use time::{OffsetDateTime, UtcOffset};

use crate::error::Result;
use crate::model::TaskStatus;
use crate::repo::{self, TaskFilter};

#[derive(Debug, Serialize)]
pub struct ExportFile {
    pub app: &'static str,
    pub format_version: u32,
    pub exported_at: String,
    pub vault: ExportVault,
    pub task_count: usize,
    pub tasks: Vec<ExportTask>,
}

#[derive(Debug, Serialize)]
pub struct ExportVault {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct ExportTask {
    pub id: String,
    pub title: String,
    pub note: Option<String>,
    pub priority: String,
    /// v2 起为对象数组，带颜色；v1 是纯字符串数组
    pub tags: Vec<ExportTag>,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub due_at: Option<String>,
    pub pomodoros: Vec<ExportPomodoro>,
    pub total_focus_secs: i64,
}

#[derive(Debug, Serialize)]
pub struct ExportTag {
    pub name: String,
    /// `#RRGGBB`
    pub color: String,
}

#[derive(Debug, Serialize)]
pub struct ExportPomodoro {
    pub started_at: String,
    pub planned_secs: i64,
    pub actual_secs: i64,
    pub finished: bool,
}

/// 将 unix 秒格式化为带本地时区偏移的 RFC3339（失败时退化为 UTC）。
pub fn format_ts(ts: i64) -> String {
    let utc = OffsetDateTime::from_unix_timestamp(ts).unwrap_or(OffsetDateTime::UNIX_EPOCH);
    let local = UtcOffset::current_local_offset()
        .map(|off| utc.to_offset(off))
        .unwrap_or(utc);
    local.format(&Rfc3339).unwrap_or_else(|_| ts.to_string())
}

/// 导出指定 vault 的全部已完成任务。
pub fn export_vault_completed(conn: &Connection, vault_id: &str) -> Result<ExportFile> {
    let vault = repo::get_vault(conn, vault_id)?;
    let tasks = repo::list_tasks(
        conn,
        &TaskFilter {
            vault_id: Some(vault_id.to_string()),
            status: Some(TaskStatus::Completed),
        },
    )?;
    let ids: Vec<String> = tasks.iter().map(|t| t.id.clone()).collect();
    let pomodoro_map = repo::pomodoros_for_tasks(conn, &ids)?;

    let mut export_tasks = Vec::with_capacity(tasks.len());
    for task in tasks {
        let sessions = pomodoro_map.get(&task.id).cloned().unwrap_or_default();
        let total_focus_secs: i64 = sessions
            .iter()
            .filter(|s| s.kind == crate::model::PomodoroKind::Focus && s.finished)
            .map(|s| s.actual_secs)
            .sum();
        let pomodoros = sessions
            .iter()
            .map(|s| ExportPomodoro {
                started_at: format_ts(s.started_at),
                planned_secs: s.planned_secs,
                actual_secs: s.actual_secs,
                finished: s.finished,
            })
            .collect();
        export_tasks.push(ExportTask {
            id: task.id,
            title: task.title,
            note: task.note,
            priority: task.priority.as_str().to_string(),
            tags: task
                .tags
                .iter()
                .map(|t| ExportTag {
                    name: t.name.clone(),
                    color: t.color_hex_string(),
                })
                .collect(),
            created_at: format_ts(task.created_at),
            completed_at: task.completed_at.map(format_ts),
            due_at: task.due_at.map(format_ts),
            pomodoros,
            total_focus_secs,
        });
    }

    Ok(ExportFile {
        app: "ticktick-lite",
        format_version: 2,
        exported_at: format_ts(crate::model::now_ts()),
        vault: ExportVault {
            id: vault.id,
            name: vault.name,
        },
        task_count: export_tasks.len(),
        tasks: export_tasks,
    })
}

/// 导出并序列化为 pretty JSON 字符串。
pub fn export_vault_completed_json(conn: &Connection, vault_id: &str) -> Result<String> {
    let file = export_vault_completed(conn, vault_id)?;
    Ok(serde_json::to_string_pretty(&file)?)
}
