//! 应用设置的读写：`settings` KV 表 + 类型化 [`AppSettings`]。
//!
//! 设计要点见 `docs/adr/ADR-0003-settings-storage.md`：
//! - 键名用 `域.字段` 分层（`ui.topmost` / `pomodoro.focus_mins`）；
//! - 旧版下划线键（`pomodoro_focus_mins` 等）**只读兼容**，写回只写新键；
//! - 越界值在 `load` / `save` 两侧都钳制，不信任任何来源。

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::repo;

// ---------------------------------------------------------------------------
// 键名常量
// ---------------------------------------------------------------------------

pub const KEY_UI_TOPMOST: &str = "ui.topmost";
pub const KEY_UI_SHOW_COMPLETED: &str = "ui.show_completed";
pub const KEY_POMODORO_FOCUS: &str = "pomodoro.focus_mins";
pub const KEY_POMODORO_SHORT_BREAK: &str = "pomodoro.short_break_mins";
pub const KEY_POMODORO_LONG_BREAK: &str = "pomodoro.long_break_mins";
pub const KEY_POMODORO_ROUNDS: &str = "pomodoro.rounds_per_long_break";
pub const KEY_POMODORO_NOTIFY: &str = "pomodoro.notify";

/// 初版用过、从未被写入过的下划线键，仅用于读取兼容。
const LEGACY_FOCUS: &str = "pomodoro_focus_mins";
const LEGACY_SHORT_BREAK: &str = "pomodoro_short_break_mins";
const LEGACY_LONG_BREAK: &str = "pomodoro_long_break_mins";

// ---------------------------------------------------------------------------
// 番茄钟配置
// ---------------------------------------------------------------------------

/// 番茄钟节奏配置。长度单位为分钟。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PomodoroConfig {
    pub focus_mins: u32,
    pub short_break_mins: u32,
    pub long_break_mins: u32,
    /// 连续几轮专注后进入一次长休息
    pub rounds_per_long_break: u32,
    /// 阶段结束时是否弹系统通知
    pub notify: bool,
}

impl Default for PomodoroConfig {
    fn default() -> Self {
        Self {
            focus_mins: 25,
            short_break_mins: 5,
            long_break_mins: 15,
            rounds_per_long_break: 4,
            notify: true,
        }
    }
}

impl PomodoroConfig {
    /// 把各字段钳制到可用区间。越界值不报错——设置界面的滑杆/输入框
    /// 不该因为一个手改的数据库值而无法保存。
    pub fn clamped(mut self) -> Self {
        self.focus_mins = self.focus_mins.clamp(1, 180);
        self.short_break_mins = self.short_break_mins.clamp(1, 60);
        self.long_break_mins = self.long_break_mins.clamp(1, 120);
        self.rounds_per_long_break = self.rounds_per_long_break.clamp(1, 12);
        self
    }

    pub fn focus_secs(&self) -> i64 {
        i64::from(self.focus_mins) * 60
    }

    pub fn short_break_secs(&self) -> i64 {
        i64::from(self.short_break_mins) * 60
    }

    pub fn long_break_secs(&self) -> i64 {
        i64::from(self.long_break_mins) * 60
    }
}

// ---------------------------------------------------------------------------
// 应用设置
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppSettings {
    /// 主窗是否始终置顶
    pub topmost: bool,
    /// 任务列表是否显示已完成项
    pub show_completed: bool,
    pub pomodoro: PomodoroConfig,
}

impl AppSettings {
    /// 从数据库读取；缺失的键回落默认值，非法值回落默认值。
    pub fn load(conn: &Connection) -> Result<Self> {
        let d = Self::default();
        let pomo = PomodoroConfig {
            focus_mins: read_u32(
                conn,
                &[KEY_POMODORO_FOCUS, LEGACY_FOCUS],
                d.pomodoro.focus_mins,
            )?,
            short_break_mins: read_u32(
                conn,
                &[KEY_POMODORO_SHORT_BREAK, LEGACY_SHORT_BREAK],
                d.pomodoro.short_break_mins,
            )?,
            long_break_mins: read_u32(
                conn,
                &[KEY_POMODORO_LONG_BREAK, LEGACY_LONG_BREAK],
                d.pomodoro.long_break_mins,
            )?,
            rounds_per_long_break: read_u32(
                conn,
                &[KEY_POMODORO_ROUNDS],
                d.pomodoro.rounds_per_long_break,
            )?,
            notify: read_bool(conn, KEY_POMODORO_NOTIFY, d.pomodoro.notify)?,
        };
        Ok(Self {
            topmost: read_bool(conn, KEY_UI_TOPMOST, d.topmost)?,
            show_completed: read_bool(conn, KEY_UI_SHOW_COMPLETED, d.show_completed)?,
            pomodoro: pomo.clamped(),
        })
    }

    /// 全量写回（7 个键）。写前 clamp，保证库里不会留下越界值。
    pub fn save(&self, conn: &Connection) -> Result<()> {
        let pomo = self.pomodoro.clamped();
        repo::set_setting(conn, KEY_UI_TOPMOST, bool_str(self.topmost))?;
        repo::set_setting(conn, KEY_UI_SHOW_COMPLETED, bool_str(self.show_completed))?;
        repo::set_setting(conn, KEY_POMODORO_FOCUS, &pomo.focus_mins.to_string())?;
        repo::set_setting(
            conn,
            KEY_POMODORO_SHORT_BREAK,
            &pomo.short_break_mins.to_string(),
        )?;
        repo::set_setting(
            conn,
            KEY_POMODORO_LONG_BREAK,
            &pomo.long_break_mins.to_string(),
        )?;
        repo::set_setting(
            conn,
            KEY_POMODORO_ROUNDS,
            &pomo.rounds_per_long_break.to_string(),
        )?;
        repo::set_setting(conn, KEY_POMODORO_NOTIFY, bool_str(pomo.notify))?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 读取原语
// ---------------------------------------------------------------------------

fn bool_str(v: bool) -> &'static str {
    if v { "1" } else { "0" }
}

fn parse_bool(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn read_bool(conn: &Connection, key: &str, default: bool) -> Result<bool> {
    match repo::get_setting(conn, key)? {
        Some(raw) => Ok(parse_bool(&raw).unwrap_or_else(|| {
            log::warn!("设置 {key} 的值 {raw:?} 无法解析为布尔，回落默认值 {default}");
            default
        })),
        None => Ok(default),
    }
}

/// 依次尝试多个键（新键在前、旧键在后），第一个能解析成功的生效。
fn read_u32(conn: &Connection, keys: &[&str], default: u32) -> Result<u32> {
    for key in keys {
        if let Some(raw) = repo::get_setting(conn, key)? {
            match raw.trim().parse::<u32>() {
                Ok(v) => return Ok(v),
                Err(_) => log::warn!("设置 {key} 的值 {raw:?} 无法解析为整数，继续尝试下一个键"),
            }
        }
    }
    Ok(default)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Database;

    fn db() -> (tempfile::TempDir, Database) {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path().join("t.db")).unwrap();
        (dir, db)
    }

    #[test]
    fn defaults_when_empty() {
        let (_d, db) = db();
        let conn = db.connect().unwrap();
        let s = AppSettings::load(&conn).unwrap();
        assert_eq!(s, AppSettings::default());
    }

    #[test]
    fn save_then_load_roundtrip() {
        let (_d, db) = db();
        let conn = db.connect().unwrap();
        let s = AppSettings {
            topmost: true,
            show_completed: true,
            pomodoro: PomodoroConfig {
                focus_mins: 50,
                short_break_mins: 10,
                long_break_mins: 30,
                rounds_per_long_break: 3,
                notify: false,
            },
        };
        s.save(&conn).unwrap();
        assert_eq!(AppSettings::load(&conn).unwrap(), s);
    }

    #[test]
    fn clamps_out_of_range_values() {
        let (_d, db) = db();
        let conn = db.connect().unwrap();
        repo::set_setting(&conn, KEY_POMODORO_FOCUS, "9999").unwrap();
        repo::set_setting(&conn, KEY_POMODORO_SHORT_BREAK, "0").unwrap();
        repo::set_setting(&conn, KEY_POMODORO_LONG_BREAK, "999").unwrap();
        repo::set_setting(&conn, KEY_POMODORO_ROUNDS, "99").unwrap();
        let s = AppSettings::load(&conn).unwrap();
        assert_eq!(s.pomodoro.focus_mins, 180);
        assert_eq!(s.pomodoro.short_break_mins, 1);
        assert_eq!(s.pomodoro.long_break_mins, 120);
        assert_eq!(s.pomodoro.rounds_per_long_break, 12);
    }

    #[test]
    fn legacy_keys_are_read() {
        let (_d, db) = db();
        let conn = db.connect().unwrap();
        repo::set_setting(&conn, LEGACY_FOCUS, "40").unwrap();
        repo::set_setting(&conn, LEGACY_SHORT_BREAK, "8").unwrap();
        let s = AppSettings::load(&conn).unwrap();
        assert_eq!(s.pomodoro.focus_mins, 40);
        assert_eq!(s.pomodoro.short_break_mins, 8);
    }

    #[test]
    fn new_key_wins_over_legacy() {
        let (_d, db) = db();
        let conn = db.connect().unwrap();
        repo::set_setting(&conn, LEGACY_FOCUS, "40").unwrap();
        repo::set_setting(&conn, KEY_POMODORO_FOCUS, "30").unwrap();
        assert_eq!(AppSettings::load(&conn).unwrap().pomodoro.focus_mins, 30);
    }

    #[test]
    fn save_does_not_touch_legacy_keys() {
        let (_d, db) = db();
        let conn = db.connect().unwrap();
        repo::set_setting(&conn, LEGACY_FOCUS, "40").unwrap();
        AppSettings::default().save(&conn).unwrap();
        assert_eq!(
            repo::get_setting(&conn, LEGACY_FOCUS).unwrap().as_deref(),
            Some("40")
        );
    }

    #[test]
    fn garbage_value_falls_back_to_default() {
        let (_d, db) = db();
        let conn = db.connect().unwrap();
        repo::set_setting(&conn, KEY_UI_TOPMOST, "maybe").unwrap();
        repo::set_setting(&conn, KEY_POMODORO_FOCUS, "abc").unwrap();
        let s = AppSettings::load(&conn).unwrap();
        assert!(!s.topmost);
        assert_eq!(s.pomodoro.focus_mins, 25);
    }
}
