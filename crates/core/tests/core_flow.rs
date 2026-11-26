//! 验收测试：建 vault → 建任务 → 打标签 → 完成 → 番茄聚合 → JSON 导出。

use ticktick_core::export;
use ticktick_core::model::{now_ts, PomodoroKind, PomodoroSession, Priority, TaskStatus};
use ticktick_core::repo::{self, NewTask, TaskFilter};
use ticktick_core::Database;

fn temp_db() -> (tempfile::TempDir, Database) {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::new(dir.path().join("test.db")).unwrap();
    (dir, db)
}

#[test]
fn full_flow_vault_task_tag_complete_export() {
    let (_dir, db) = temp_db();
    let conn = db.connect().unwrap();

    // vault
    let vault = repo::create_vault(&conn, "工作", None).unwrap();
    assert_eq!(repo::list_vaults(&conn).unwrap().len(), 1);

    // task with tags
    let task = repo::create_task(
        &conn,
        NewTask {
            vault_id: vault.id.clone(),
            title: "写季度汇报".into(),
            note: Some("附 Q3 数据".into()),
            priority: Priority::High,
            due_at: None,
            tag_names: vec!["汇报".into(), "文档".into(), "汇报".into()],
        },
    )
    .unwrap();
    assert_eq!(task.tags.len(), 2, "标签应去重");
    assert_eq!(task.priority, Priority::High);
    assert_eq!(repo::total_active_count(&conn).unwrap(), 1);

    // list + filter
    let active = repo::list_tasks(
        &conn,
        &TaskFilter {
            vault_id: Some(vault.id.clone()),
            status: Some(TaskStatus::Active),
        },
    )
    .unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].tags.len(), 2);

    // pomodoro: 一次完整、一次放弃
    let s1 = PomodoroSession {
        id: ticktick_core::model::new_id(),
        task_id: Some(task.id.clone()),
        kind: PomodoroKind::Focus,
        planned_secs: 1500,
        actual_secs: 1500,
        started_at: now_ts() - 2000,
        ended_at: Some(now_ts() - 500),
        finished: true,
    };
    let s2 = PomodoroSession {
        id: ticktick_core::model::new_id(),
        task_id: Some(task.id.clone()),
        kind: PomodoroKind::Focus,
        planned_secs: 1500,
        actual_secs: 300,
        started_at: now_ts() - 400,
        ended_at: Some(now_ts() - 100),
        finished: false,
    };
    repo::insert_pomodoro_session(&conn, &s1).unwrap();
    repo::insert_pomodoro_session(&conn, &s2).unwrap();
    let totals = repo::focus_totals(&conn, &[task.id.clone()]).unwrap();
    assert_eq!(totals.get(&task.id), Some(&1500), "仅统计完整完成的专注");

    // complete task
    repo::set_task_status(&conn, &task.id, TaskStatus::Completed).unwrap();
    assert_eq!(repo::total_active_count(&conn).unwrap(), 0);
    let done = repo::get_task(&conn, &task.id).unwrap();
    assert!(done.completed_at.is_some());

    // export
    let json = export::export_vault_completed_json(&conn, &vault.id).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["app"], "ticktick-lite");
    assert_eq!(value["format_version"], 1);
    assert_eq!(value["vault"]["name"], "工作");
    assert_eq!(value["task_count"], 1);
    assert_eq!(value["tasks"][0]["title"], "写季度汇报");
    assert_eq!(value["tasks"][0]["priority"], "high");
    assert_eq!(value["tasks"][0]["total_focus_secs"], 1500);
    assert_eq!(value["tasks"][0]["pomodoros"].as_array().unwrap().len(), 2);
    assert!(value["exported_at"].as_str().unwrap().contains('T'));
}

#[test]
fn export_empty_vault_is_ok() {
    let (_dir, db) = temp_db();
    let conn = db.connect().unwrap();
    let vault = repo::create_vault(&conn, "生活", None).unwrap();
    let json = export::export_vault_completed_json(&conn, &vault.id).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["task_count"], 0);
    assert_eq!(value["tasks"].as_array().unwrap().len(), 0);
}

#[test]
fn cascade_delete_vault_removes_tasks() {
    let (_dir, db) = temp_db();
    let conn = db.connect().unwrap();
    let vault = repo::create_vault(&conn, "临时", None).unwrap();
    let task = repo::create_task(
        &conn,
        NewTask {
            vault_id: vault.id.clone(),
            title: "临时任务".into(),
            note: None,
            priority: Priority::None,
            due_at: None,
            tag_names: vec![],
        },
    )
    .unwrap();
    repo::delete_vault(&conn, &vault.id).unwrap();
    assert!(repo::get_task(&conn, &task.id).is_err());
}

#[test]
fn settings_kv_roundtrip() {
    let (_dir, db) = temp_db();
    let conn = db.connect().unwrap();
    assert_eq!(
        repo::get_setting(&conn, "pomodoro_focus_mins").unwrap(),
        None
    );
    repo::set_setting(&conn, "pomodoro_focus_mins", "25").unwrap();
    assert_eq!(
        repo::get_setting(&conn, "pomodoro_focus_mins").unwrap(),
        Some("25".to_string())
    );
    repo::set_setting(&conn, "pomodoro_focus_mins", "50").unwrap();
    assert_eq!(
        repo::get_setting(&conn, "pomodoro_focus_mins").unwrap(),
        Some("50".to_string())
    );
}

#[test]
fn task_update_and_move() {
    let (_dir, db) = temp_db();
    let conn = db.connect().unwrap();
    let v1 = repo::create_vault(&conn, "A", None).unwrap();
    let v2 = repo::create_vault(&conn, "B", None).unwrap();
    let task = repo::create_task(
        &conn,
        NewTask {
            vault_id: v1.id.clone(),
            title: "原标题".into(),
            note: None,
            priority: Priority::Low,
            due_at: None,
            tag_names: vec!["旧".into()],
        },
    )
    .unwrap();
    let updated = repo::update_task_details(
        &conn,
        &task.id,
        "新标题",
        Some("备注"),
        Priority::Medium,
        None,
        &v2.id,
        &["新标签".to_string()],
    )
    .unwrap();
    assert_eq!(updated.title, "新标题");
    assert_eq!(updated.vault_id, v2.id);
    assert_eq!(updated.priority, Priority::Medium);
    assert_eq!(updated.tags.len(), 1);
    assert_eq!(updated.tags[0].name, "新标签");
    // 优先级排序：高优先级应排前
    let _ = repo::create_task(
        &conn,
        NewTask {
            vault_id: v2.id.clone(),
            title: "低优先级".into(),
            note: None,
            priority: Priority::None,
            due_at: None,
            tag_names: vec![],
        },
    )
    .unwrap();
    let tasks = repo::list_tasks(
        &conn,
        &TaskFilter {
            vault_id: Some(v2.id.clone()),
            status: Some(TaskStatus::Active),
        },
    )
    .unwrap();
    assert_eq!(tasks[0].priority, Priority::Medium);
}
