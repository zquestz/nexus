//! Custom widgets
//!
//! `MenuButton` stores hover state in tree state so it works inside the
//! per-frame-rebuilt context menu overlay, which Iced's `Button` cannot.

mod menu_button;

pub use menu_button::{MenuButton, Status as MenuButtonStatus, Style as MenuButtonStyle};
