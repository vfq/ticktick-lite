//! 验收测试：建 vault → 建任务 → 打标签 → 完成 → 番茄聚合 → JSON 导出，
//! 以及标签 CRUD / 筛选 / schema v2 迁移 / 设置读写。

use ticktick_core::Database;
use ticktick_core::db;
use ticktick_core::export;
use ticktick_core::model::{
    PomodoroKind, PomodoroSession, Priority, TaskStatus, default_tag_color, now_ts, parse_hex_color,
};
use ticktick_core::repo::{self, NewTask, TaskEdit, TaskFilter};

fn temp_db() -> (tempfile::TempDir, Database) {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::new(dir.path().join("test.db")).unwrap();
    (dir, db)
}

fn new_task(vault_id: &str, title: &str, priority: Priority, tags: &[&str]) -> NewTask {
    NewTask {
        vault_id: vault_id.to_string(),
        title: title.to_string(),
        note: None,
        priority,
        due_at: None,
        tag_names: tags.iter().map(|s| s.to_string()).collect(),
    }
}

// ---------------------------------------------------------------------------
// 主链路
// ---------------------------------------------------------------------------

#[test]
fn full_flow_vault_task_tag_complete_export() {
    let (_dir, db) = temp_db();
    let conn = db.connect().unwrap();

    let vault = repo::create_vault(&conn, "工作", None).unwrap();
    assert_eq!(repo::list_vaults(&conn).unwrap().len(), 1);

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

    let active = repo::list_tasks(
        &conn,
        &TaskFilter {
            vault_id: Some(vault.id.clone()),
            status: Some(TaskStatus::Active),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].tags.len(), 2);

    // 番茄：一次完整、一次放弃
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
        finished: false,
        actual_secs: 300,
        id: ticktick_core::model::new_id(),
        ..s1.clone()
    };
    repo::insert_pomodoro_session(&conn, &s1).unwrap();
    repo::insert_pomodoro_session(&conn, &s2).unwrap();
    let totals = repo::focus_totals(&conn, std::slice::from_ref(&task.id)).unwrap();
    assert_eq!(totals.get(&task.id), Some(&1500), "仅统计完整完成的专注");

    repo::set_task_status(&conn, &task.id, TaskStatus::Completed).unwrap();
    assert_eq!(repo::total_active_count(&conn).unwrap(), 0);
    let done = repo::get_task(&conn, &task.id).unwrap();
    assert!(done.completed_at.is_some());

    let json = export::export_vault_completed_json(&conn, &vault.id).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["app"], "ticktick-lite");
    assert_eq!(value["format_version"], 2, "导出格式已升到 v2");
    assert_eq!(value["vault"]["name"], "工作");
    assert_eq!(value["task_count"], 1);
    assert_eq!(value["tasks"][0]["title"], "写季度汇报");
    assert_eq!(value["tasks"][0]["priority"], "high");
    assert_eq!(value["tasks"][0]["total_focus_secs"], 1500);
    assert_eq!(value["tasks"][0]["pomodoros"].as_array().unwrap().len(), 2);
    assert!(value["exported_at"].as_str().unwrap().contains('T'));

    // v2：tags 是对象数组，带 #RRGGBB 颜色
    let tags = value["tasks"][0]["tags"].as_array().unwrap();
    assert_eq!(tags.len(), 2);
    for t in tags {
        let color = t["color"].as_str().unwrap();
        assert!(
            color.starts_with('#') && color.len() == 7,
            "颜色应为 #RRGGBB，实际 {color}"
        );
        assert!(parse_hex_color(color).is_some());
        assert!(t["name"].as_str().is_some());
    }
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
    let task =
        repo::create_task(&conn, new_task(&vault.id, "临时任务", Priority::None, &[])).unwrap();
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
    let task =
        repo::create_task(&conn, new_task(&v1.id, "原标题", Priority::Low, &["旧"])).unwrap();

    let updated = repo::update_task(
        &conn,
        &task.id,
        TaskEdit {
            title: "新标题",
            note: Some("备注"),
            priority: Priority::Medium,
            due_at: None,
            vault_id: &v2.id,
            tag_names: &["新标签".to_string()],
        },
    )
    .unwrap();
    assert_eq!(updated.title, "新标题");
    assert_eq!(updated.vault_id, v2.id);
    assert_eq!(updated.priority, Priority::Medium);
    assert_eq!(updated.tags.len(), 1);
    assert_eq!(updated.tags[0].name, "新标签");
    assert!(updated.tags[0].color.is_some(), "新标签应自动带颜色");

    repo::create_task(&conn, new_task(&v2.id, "无优先级", Priority::None, &[])).unwrap();
    let tasks = repo::list_tasks(
        &conn,
        &TaskFilter {
            vault_id: Some(v2.id.clone()),
            status: Some(TaskStatus::Active),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(tasks[0].priority, Priority::Medium, "高优先级应排前");
}

// ---------------------------------------------------------------------------
// 标签
// ---------------------------------------------------------------------------

#[test]
fn create_tag_defaults_color_and_rejects_duplicate_name() {
    let (_dir, db) = temp_db();
    let conn = db.connect().unwrap();

    let tag = repo::create_tag(&conn, "  工作  ", None).unwrap();
    assert_eq!(tag.name, "工作", "名称应 trim");
    assert_eq!(
        tag.color.as_deref(),
        Some(default_tag_color("工作").as_str()),
        "缺省色按名称派生"
    );

    let err = repo::create_tag(&conn, "工作", None).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("已存在"), "重复名应报冲突，实际：{msg}");
}

#[test]
fn create_tag_keeps_explicit_color_and_normalizes_case() {
    let (_dir, db) = temp_db();
    let conn = db.connect().unwrap();
    let tag = repo::create_tag(&conn, "紧急", Some("#e5484d")).unwrap();
    assert_eq!(tag.color.as_deref(), Some("#E5484D"));

    // 非法颜色回落名称派生色，而不是写进库
    let bad = repo::create_tag(&conn, "奇怪", Some("红色")).unwrap();
    assert_eq!(
        bad.color.as_deref(),
        Some(default_tag_color("奇怪").as_str())
    );
}

#[test]
fn rename_and_recolor_tag() {
    let (_dir, db) = temp_db();
    let conn = db.connect().unwrap();
    let a = repo::create_tag(&conn, "旧名", None).unwrap();
    repo::create_tag(&conn, "占位", None).unwrap();

    repo::rename_tag(&conn, &a.id, "新名").unwrap();
    let tags = repo::list_tags(&conn).unwrap();
    assert!(tags.iter().any(|t| t.name == "新名"));

    // 改成已存在的名字应被拒绝
    assert!(repo::rename_tag(&conn, &a.id, "占位").is_err());

    repo::set_tag_color(&conn, &a.id, Some("#0091FF")).unwrap();
    let tags = repo::list_tags(&conn).unwrap();
    let t = tags.iter().find(|t| t.id == a.id).unwrap();
    assert_eq!(t.color.as_deref(), Some("#0091FF"));

    // 传 None 恢复名称派生色
    repo::set_tag_color(&conn, &a.id, None).unwrap();
    let tags = repo::list_tags(&conn).unwrap();
    let t = tags.iter().find(|t| t.id == a.id).unwrap();
    assert_eq!(t.color.as_deref(), Some(default_tag_color("新名").as_str()));
}

#[test]
fn delete_tag_keeps_task_but_drops_association() {
    let (_dir, db) = temp_db();
    let conn = db.connect().unwrap();
    let vault = repo::create_vault(&conn, "V", None).unwrap();
    let task = repo::create_task(
        &conn,
        new_task(&vault.id, "带标签", Priority::None, &["甲", "乙"]),
    )
    .unwrap();
    assert_eq!(repo::get_task(&conn, &task.id).unwrap().tags.len(), 2);

    let tag = repo::list_tags(&conn)
        .unwrap()
        .into_iter()
        .find(|t| t.name == "甲")
        .unwrap();
    repo::delete_tag(&conn, &tag.id).unwrap();

    let reloaded = repo::get_task(&conn, &task.id).unwrap();
    assert_eq!(reloaded.tags.len(), 1, "关联应级联删除");
    assert_eq!(reloaded.tags[0].name, "乙");
    assert!(repo::get_task(&conn, &task.id).is_ok(), "任务本身保留");
}

#[test]
fn list_tags_with_counts_separates_active_and_total() {
    let (_dir, db) = temp_db();
    let conn = db.connect().unwrap();
    let vault = repo::create_vault(&conn, "V", None).unwrap();
    let t1 =
        repo::create_task(&conn, new_task(&vault.id, "一", Priority::None, &["工作"])).unwrap();
    repo::create_task(&conn, new_task(&vault.id, "二", Priority::None, &["工作"])).unwrap();
    repo::create_task(&conn, new_task(&vault.id, "三", Priority::None, &["工作"])).unwrap();
    repo::set_task_status(&conn, &t1.id, TaskStatus::Completed).unwrap();

    let counts = repo::list_tags_with_counts(&conn).unwrap();
    let c = counts.iter().find(|c| c.tag.name == "工作").unwrap();
    assert_eq!(c.active, 2);
    assert_eq!(c.total, 3);
}

#[test]
fn filter_by_tags_is_or_and_deduped() {
    let (_dir, db) = temp_db();
    let conn = db.connect().unwrap();
    let vault = repo::create_vault(&conn, "V", None).unwrap();
    let both = repo::create_task(
        &conn,
        new_task(&vault.id, "甲乙", Priority::None, &["甲", "乙"]),
    )
    .unwrap();
    let only_a =
        repo::create_task(&conn, new_task(&vault.id, "甲", Priority::None, &["甲"])).unwrap();
    repo::create_task(&conn, new_task(&vault.id, "丙", Priority::None, &["丙"])).unwrap();

    let ids: Vec<String> = repo::list_tags(&conn)
        .unwrap()
        .into_iter()
        .filter(|t| t.name == "甲" || t.name == "乙")
        .map(|t| t.id)
        .collect();
    assert_eq!(ids.len(), 2);

    let hits = repo::list_tasks(
        &conn,
        &TaskFilter {
            vault_id: Some(vault.id.clone()),
            status: Some(TaskStatus::Active),
            tag_ids: ids,
        },
    )
    .unwrap();
    // OR 语义：命中「甲」或「乙」；"甲乙" 同时命中两个标签但只出现一次
    assert_eq!(hits.len(), 2);
    let hit_ids: Vec<&str> = hits.iter().map(|t| t.id.as_str()).collect();
    assert!(hit_ids.contains(&both.id.as_str()));
    assert!(hit_ids.contains(&only_a.id.as_str()));
}

#[test]
fn filter_with_empty_tag_ids_does_not_filter() {
    let (_dir, db) = temp_db();
    let conn = db.connect().unwrap();
    let vault = repo::create_vault(&conn, "V", None).unwrap();
    repo::create_task(&conn, new_task(&vault.id, "一", Priority::None, &[])).unwrap();
    let all = repo::list_tasks(
        &conn,
        &TaskFilter {
            vault_id: Some(vault.id.clone()),
            status: Some(TaskStatus::Active),
            tag_ids: Vec::new(),
        },
    )
    .unwrap();
    assert_eq!(all.len(), 1);
}

// ---------------------------------------------------------------------------
// 迁移与颜色派生
// ---------------------------------------------------------------------------

#[test]
fn migration_backfills_tag_color_and_is_idempotent() {
    let (_dir, db) = temp_db();
    let conn = db.connect().unwrap();

    // 模拟 v1 库：一条 NULL 颜色的标签 + 版本回退到 1
    conn.execute(
        "INSERT INTO tags(id, name, color) VALUES ('legacy-1', '旧标签', NULL)",
        [],
    )
    .unwrap();
    conn.execute("UPDATE schema_meta SET version = 1", [])
        .unwrap();

    db::migrate(&conn).unwrap();
    let color: Option<String> = conn
        .query_row("SELECT color FROM tags WHERE id = 'legacy-1'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(color.as_deref(), Some(default_tag_color("旧标签").as_str()));

    let version: i64 = conn
        .query_row("SELECT MAX(version) FROM schema_meta", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, db::SCHEMA_VERSION);

    // 再跑一次：颜色不变、版本不重复插入
    db::migrate(&conn).unwrap();
    let color2: Option<String> = conn
        .query_row("SELECT color FROM tags WHERE id = 'legacy-1'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(color2, color, "回填幂等");
    let rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM schema_meta", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rows, 1, "版本记录不应重复插入");
}

#[test]
fn migration_keeps_user_chosen_color() {
    let (_dir, db) = temp_db();
    let conn = db.connect().unwrap();
    conn.execute(
        "INSERT INTO tags(id, name, color) VALUES ('t-keep', '手工色', '#123456')",
        [],
    )
    .unwrap();
    conn.execute("UPDATE schema_meta SET version = 1", [])
        .unwrap();
    db::migrate(&conn).unwrap();

    let color: Option<String> = conn
        .query_row("SELECT color FROM tags WHERE id = 't-keep'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(color.as_deref(), Some("#123456"), "已有颜色不应被覆盖");
}

#[test]
fn v2_index_exists() {
    let (_dir, db) = temp_db();
    let conn = db.connect().unwrap();
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'idx_task_tags_tag_id'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 1);
}

#[test]
fn default_tag_color_is_stable_and_palette_bound() {
    // 同名同色：两次调用结果一致，且落在调色板内
    let a = default_tag_color("工作");
    assert_eq!(a, default_tag_color("工作"));
    assert!(ticktick_core::model::TAG_PALETTE.contains(&parse_hex_color(&a).unwrap()));

    // 不同名未必不同色（8 色必然碰撞），但必须都是合法色
    for name in [
        "工作", "学习", "家庭", "紧急", "周会", "文档", "汇报", "复盘",
    ] {
        let c = default_tag_color(name);
        assert!(parse_hex_color(&c).is_some(), "{name} -> {c}");
    }
}

#[test]
fn tag_color_hex_falls_back_for_dirty_data() {
    use ticktick_core::model::Tag;
    let dirty = Tag {
        id: "x".into(),
        name: "脏数据".into(),
        color: Some("not-a-color".into()),
    };
    assert_eq!(
        dirty.color_hex_string(),
        default_tag_color("脏数据"),
        "非法颜色应回落名称派生色"
    );

    let missing = Tag {
        id: "y".into(),
        name: "缺色".into(),
        color: None,
    };
    assert_eq!(missing.color_hex_string(), default_tag_color("缺色"));
}
