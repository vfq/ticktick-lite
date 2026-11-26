//! 应用设置的跨窗口共享实体。
//!
//! 主窗与设置窗都持有同一个 `Entity<SettingsStore>`，任何一侧 `update()` 都会
//! 落库并 `emit` 事件，另一侧订阅后立即同步——不存在"设置里改了但主窗没变"。
//! 设计见 `docs/adr/ADR-0003-settings-storage.md`。

use gpui_kit::EventEmitter;
use gpui_kit::prelude::*;
use ticktick_core::{AppSettings, Database};

/// 设置已变更。订阅方通常只需在回调里重读 [`SettingsStore::get`]。
#[derive(Debug, Clone, Copy)]
pub struct SettingsChanged;

pub struct SettingsStore {
    db: Database,
    settings: AppSettings,
}

impl EventEmitter<SettingsChanged> for SettingsStore {}

impl SettingsStore {
    pub fn new(db: Database, _cx: &mut Context<Self>) -> Self {
        let settings = db
            .connect()
            .ok()
            .and_then(|conn| AppSettings::load(&conn).ok())
            .unwrap_or_default();
        Self { db, settings }
    }

    pub fn get(&self) -> AppSettings {
        self.settings
    }

    /// 改设置：内存生效 → 钳制 → 落库 → 广播。
    ///
    /// 值没有实际变化时不落库也不广播，避免订阅方被无意义刷新（例如拖动滑杆）。
    pub fn edit(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut AppSettings)) {
        let mut next = self.settings;
        f(&mut next);
        next.pomodoro = next.pomodoro.clamped();
        if next == self.settings {
            return;
        }
        self.settings = next;
        self.persist();
        cx.emit(SettingsChanged);
        cx.notify();
    }

    /// 设置本身没变、但依赖它的数据变了（如标签改名/改色）时，用它通知订阅方刷新。
    pub fn notify_changed(&mut self, cx: &mut Context<Self>) {
        cx.emit(SettingsChanged);
        cx.notify();
    }

    fn persist(&self) {
        match self.db.connect() {
            Ok(conn) => {
                if let Err(e) = self.settings.save(&conn) {
                    // 落库失败不回滚内存值：用户刚改的开关不应自己弹回去。
                    // 下次任一设置变更会连同这次的改动一起写回。
                    log::error!("保存设置失败: {e}");
                }
            }
            Err(e) => log::error!("打开数据库失败，设置未能落库: {e}"),
        }
    }
}
