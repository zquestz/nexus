//! Window constants for application dimensions
//!
//! Window size and title constants are defined here.

use crate::constants::APP_NAME;

// ============================================================================
// Window Dimensions
// ============================================================================

/// Default window width
pub const WINDOW_WIDTH: f32 = 1200.0;

/// Default window height
pub const WINDOW_HEIGHT: f32 = 700.0;

/// Minimum window width
pub const WINDOW_WIDTH_MIN: f32 = 800.0;

/// Minimum window height
pub const WINDOW_HEIGHT_MIN: f32 = 500.0;

/// Window title (same as APP_NAME - Iced requires &'static str or closure)
pub const WINDOW_TITLE: &str = APP_NAME;
