//! 设计令牌：颜色、圆角、动效节奏集中定义，保证视觉一致性（亮色主题）。
//!
//! 规范见 `design/ui-spec.html`：
//! - 间距刻度 4 / 8 / 12 / 16 / 24px，4px 为基准
//! - 圆角 控件 6px、卡片 12px、chip 全圆角
//! - 过渡 150ms ease-out，优先级使用「色条 + 分组标题」双编码

use gpui_kit::{Rgba, rgb};
use ticktick_core::Priority;

// 中性
pub const BG: u32 = 0xFFFFFF;
pub const BG_SUB: u32 = 0xF7F8FA;
pub const BG_HOVER: u32 = 0xECEEF1;
pub const BG_ACTIVE: u32 = 0xE8EEFB;
pub const BG_CHIP: u32 = 0xF1F3F6;
pub const BG_BTN: u32 = 0xF1F3F6;
pub const BG_BTN_HOVER: u32 = 0xE5E8EC;

// 边框与文字
pub const BORDER: u32 = 0xECEEF1;
pub const TEXT: u32 = 0x1F2328;
pub const TEXT_MUTED: u32 = 0x6B7280;
pub const TEXT_FAINT: u32 = 0x9CA3AF;

// 语义色
pub const ACCENT: u32 = 0x2563EB;
pub const ACCENT_HOVER: u32 = 0x1D4ED8;
pub const DANGER: u32 = 0xDC2626;
pub const DANGER_HOVER: u32 = 0xB91C1C;
pub const SUCCESS: u32 = 0x30A46C;
pub const OVERDUE: u32 = 0xDC2626;
pub const FOCUS: u32 = 0xE5484D;

// 尺寸
pub const WINDOW_W: f32 = 380.0;
pub const WINDOW_H: f32 = 620.0;
pub const TITLEBAR_H: f32 = 40.0;
pub const CHIP_ROW_H: f32 = 36.0;

/// 控件圆角 6px
pub const RADIUS_CONTROL: f32 = 6.0;

/// 卡片圆角 12px
pub const RADIUS_CARD: f32 = 12.0;

pub fn rgba(color: u32) -> Rgba {
    rgb(color)
}

/// 优先级颜色：高=红 / 中=橙 / 低=蓝 / 无=灰
pub fn priority_color(p: Priority) -> Rgba {
    rgb(p.color_hex())
}

pub fn tag_chip_color() -> Rgba {
    rgb(BG_CHIP)
}
