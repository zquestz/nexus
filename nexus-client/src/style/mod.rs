//! Shared UI styling for consistent appearance across views
//!
//! This module provides:
//! - Window constants (in `window` module)
//! - Font constants and sizes (in `fonts` module)
//! - Layout constants for dimensions, spacing, padding (in `layout` module)
//! - Color palette constants (in `palette` module)
//! - Theme-aware color helper functions (in `colors` module)
//! - Text shaping helpers with CJK support (in `shaping` module)
//! - Widget style functions (in `widgets` module)
//!
//! ## Usage
//!
//! Always use these helper functions in view code, never hardcode colors:
//! ```ignore
//! // ✅ GOOD
//! text("Hello").style(|theme| text::Style {
//!     color: Some(section_title_color(theme)),
//! })
//!
//! // ❌ BAD
//! text("Hello").color(Color::from_rgb(0.7, 0.7, 0.7))
//! ```

mod colors;
mod fonts;
mod icons;
mod layout;
mod palette;
mod shaping;
mod widgets;
mod window;

// Re-export all public items from submodules
pub use colors::*;
pub use fonts::*;
pub use icons::*;
pub use layout::*;
pub use shaping::{shaped_text, shaped_text_wrapped};
pub use widgets::{
    chat_tab_active_style, chat_tab_inactive_style, modal_overlay_style, primary_button_style,
    primary_checkbox_style, primary_scrollbar_style, primary_text_input_style,
    tooltip_container_style,
};
pub use window::*;
