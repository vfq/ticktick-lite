//! 滴答 Lite —— 桌面悬浮小窗 Todo 应用
//!
//! 技术栈：Rust + gpui（经 gpui-kit 单一依赖引入）+ SQLite。
//! 功能：无边框置顶小窗、多 vault 清单、任务标签与优先级颜色、
//! 已完成任务按 vault 导出 JSON、番茄钟插件、系统托盘。

mod platform;
mod state;
mod theme_ext;
mod tray;
mod view;

use gpui_kit::prelude::*;
use gpui_kit::*;
use ticktick_core::Database;

fn main() {
    env_logger::init();

    let db = Database::new(state::db_path()).expect("无法打开数据库");
    state::ensure_seeded(&db).expect("初始化种子数据失败");

    let app = gpui_kit::application().with_assets(gpui_kit::assets::Assets);
    app.run(move |cx| {
        gpui_kit::init(cx);

        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(theme_ext::WINDOW_W), px(theme_ext::WINDOW_H)),
                cx,
            ))),
            // 无边框小窗，标题栏自绘
            titlebar: None,
            window_min_size: Some(size(px(340.0), px(480.0))),
            ..Default::default()
        };

        let db2 = db.clone();
        // open_window 返回 (窗口句柄, 根视图实体)
        let (window_handle, _root) = gpui_kit::open_window(options, cx, move |window, cx| {
            cx.new(|cx| view::RootView::new(window, cx, db2))
        })
        .expect("无法创建主窗口");

        tray::spawn_tray(cx, window_handle);
    });
}
