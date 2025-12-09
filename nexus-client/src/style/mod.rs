//! Shared UI styling for consistent appearance across views
//!
//! This module provides:
//! - Window constants (in `window` module)
//! - Font constants and sizes (in `fonts` module)
//! - Layout constants for dimensions, spacing, padding (in `layout` module)
//! - UI colors from Iced's theme palette (in `ui` module)
//! - Chat-specific custom colors (in `chat` module)
//! - Text shaping helpers with CJK support (in `shaping` module)
//! - Widget style functions (in `widgets` module)
//!
//! ## Usage
//!
//! For UI elements, use the `ui` module functions which derive from the theme palette:
//! ```ignore
//! use crate::style::ui;
//!
//! // ✅ GOOD - uses theme palette
//! text("Hello").style(|theme| text::Style {
//!     color: Some(ui::muted_text_color(theme)),
//! })
//!
//! // ❌ BAD - hardcoded color
//! text("Hello").color(Color::from_rgb(0.7, 0.7, 0.7))
//! ```
//!
//! For chat messages, use the `chat` module which has custom colors:
//! ```ignore
//! use crate::style::chat;
//!
//! let color = chat::admin(theme);  // Custom admin red
//! let color = chat::error(theme);  // Uses palette.danger
//! ```

pub mod celestial;
pub mod chat;
mod fonts;
mod icons;
mod layout;
mod shaping;
pub mod ui;
mod widgets;
mod window;

// Re-export all public items from submodules
pub use fonts::*;
pub use icons::*;
pub use layout::*;
pub use shaping::{shaped_text, shaped_text_wrapped};
pub use widgets::{
    alternating_row_style, chat_tab_active_style, close_button_on_primary_style,
    content_background_style, danger_icon_button_style, disabled_icon_button_style,
    error_text_style, icon_button_with_hover_style, list_item_button_style, modal_overlay_style,
    muted_text_style, separator_style, sidebar_panel_style, subheading_text_style,
    toolbar_background_style, toolbar_button_style, tooltip_container_style,
    transparent_icon_button_style, user_list_item_button_style, user_toolbar_separator_style,
};
pub use window::*;
