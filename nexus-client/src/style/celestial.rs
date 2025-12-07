//! Celestial custom theme definitions
//!
//! Based on the Celestial GTK Theme by zquestz.
//! <https://github.com/zquestz/celestial-gtk-theme>
//!
//! Provides 8 theme variants:
//! - Sea (teal accent) - Light and Dark
//! - Aliz (red accent) - Light and Dark
//! - Azul (blue accent) - Light and Dark
//! - Pueril (green accent) - Light and Dark

use iced::theme::Palette;
use iced::{Color, Theme};

// =============================================================================
// Color Parsing Helper
// =============================================================================

/// Parse a hex color string to an iced Color
const fn hex(hex: u32) -> Color {
    Color {
        r: ((hex >> 16) & 0xFF) as f32 / 255.0,
        g: ((hex >> 8) & 0xFF) as f32 / 255.0,
        b: (hex & 0xFF) as f32 / 255.0,
        a: 1.0,
    }
}

// =============================================================================
// Sea Theme (Teal Accent)
// =============================================================================

/// Celestial Sea Dark palette
fn sea_dark_palette() -> Palette {
    Palette {
        background: hex(0x1b2224),
        text: hex(0xccd7d4),
        primary: hex(0x2eb398),
        success: hex(0x2eb398),
        warning: hex(0xf0c674),
        danger: hex(0xfc4138),
    }
}

/// Celestial Sea Light palette
fn sea_light_palette() -> Palette {
    Palette {
        background: hex(0xf7f7f7),
        text: hex(0x273134),
        primary: hex(0x2eb398),
        success: hex(0x2eb398),
        warning: hex(0xf0c674),
        danger: hex(0xfc4138),
    }
}

// =============================================================================
// Aliz Theme (Red Accent)
// =============================================================================

/// Celestial Aliz Dark palette
fn aliz_dark_palette() -> Palette {
    Palette {
        background: hex(0x222222),
        text: hex(0xcbbfbf),
        primary: hex(0xf0544c),
        success: hex(0x3498db),
        warning: hex(0xf0c674),
        danger: hex(0xfc4138),
    }
}

/// Celestial Aliz Light palette
fn aliz_light_palette() -> Palette {
    Palette {
        background: hex(0xf7f7f7),
        text: hex(0x323232),
        primary: hex(0xf0544c),
        success: hex(0x3498db),
        warning: hex(0xf0c674),
        danger: hex(0xfc4138),
    }
}

// =============================================================================
// Azul Theme (Blue Accent)
// =============================================================================

/// Celestial Azul Dark palette
fn azul_dark_palette() -> Palette {
    Palette {
        background: hex(0x1b1d24),
        text: hex(0xbbc3c8),
        primary: hex(0x3498db),
        success: hex(0x2eb398),
        warning: hex(0xf0c674),
        danger: hex(0xfc4138),
    }
}

/// Celestial Azul Light palette
fn azul_light_palette() -> Palette {
    Palette {
        background: hex(0xf7f7f7),
        text: hex(0x2b2f3b),
        primary: hex(0x3498db),
        success: hex(0x2eb398),
        warning: hex(0xf0c674),
        danger: hex(0xfc4138),
    }
}

// =============================================================================
// Pueril Theme (Green Accent)
// =============================================================================

/// Celestial Pueril Dark palette
fn pueril_dark_palette() -> Palette {
    Palette {
        background: hex(0x222222),
        text: hex(0xbfbfbf),
        primary: hex(0x97bb72),
        success: hex(0x3498db),
        warning: hex(0xf0c674),
        danger: hex(0xfc4138),
    }
}

/// Celestial Pueril Light palette
fn pueril_light_palette() -> Palette {
    Palette {
        background: hex(0xf7f7f7),
        text: hex(0x323232),
        primary: hex(0x97bb72),
        success: hex(0x3498db),
        warning: hex(0xf0c674),
        danger: hex(0xfc4138),
    }
}

// =============================================================================
// Theme Constructors
// =============================================================================

/// Create Celestial Sea Dark theme
pub fn sea_dark() -> Theme {
    Theme::custom("Celestial Sea Dark".to_string(), sea_dark_palette())
}

/// Create Celestial Sea Light theme
pub fn sea_light() -> Theme {
    Theme::custom("Celestial Sea Light".to_string(), sea_light_palette())
}

/// Create Celestial Aliz Dark theme
pub fn aliz_dark() -> Theme {
    Theme::custom("Celestial Aliz Dark".to_string(), aliz_dark_palette())
}

/// Create Celestial Aliz Light theme
pub fn aliz_light() -> Theme {
    Theme::custom("Celestial Aliz Light".to_string(), aliz_light_palette())
}

/// Create Celestial Azul Dark theme
pub fn azul_dark() -> Theme {
    Theme::custom("Celestial Azul Dark".to_string(), azul_dark_palette())
}

/// Create Celestial Azul Light theme
pub fn azul_light() -> Theme {
    Theme::custom("Celestial Azul Light".to_string(), azul_light_palette())
}

/// Create Celestial Pueril Dark theme
pub fn pueril_dark() -> Theme {
    Theme::custom("Celestial Pueril Dark".to_string(), pueril_dark_palette())
}

/// Create Celestial Pueril Light theme
pub fn pueril_light() -> Theme {
    Theme::custom("Celestial Pueril Light".to_string(), pueril_light_palette())
}

// =============================================================================
// Theme Lookup
// =============================================================================

/// Get a Celestial theme by name
pub fn get_by_name(name: &str) -> Option<Theme> {
    match name {
        "Celestial Sea Dark" => Some(sea_dark()),
        "Celestial Sea Light" => Some(sea_light()),
        "Celestial Aliz Dark" => Some(aliz_dark()),
        "Celestial Aliz Light" => Some(aliz_light()),
        "Celestial Azul Dark" => Some(azul_dark()),
        "Celestial Azul Light" => Some(azul_light()),
        "Celestial Pueril Dark" => Some(pueril_dark()),
        "Celestial Pueril Light" => Some(pueril_light()),
        _ => None,
    }
}

/// Get all Celestial themes
pub fn all() -> Vec<Theme> {
    vec![
        sea_dark(),
        sea_light(),
        aliz_dark(),
        aliz_light(),
        azul_dark(),
        azul_light(),
        pueril_dark(),
        pueril_light(),
    ]
}
