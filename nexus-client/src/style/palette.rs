//! Color palette constants for the Nexus BBS Client
//!
//! This module defines named colors by their actual color name, not their usage.
//! The `colors` module references these to provide theme-aware color functions.
//!
//! Colors are sorted alphabetically for easy lookup.

use iced::Color;

// ============================================================================
// Named Colors (Alphabetically Sorted)
// ============================================================================

/// Azure blue - medium blue for info text (light theme)
pub const AZURE: Color = Color::from_rgb(0.2, 0.5, 0.8);

/// Black - pure black
pub const BLACK: Color = Color::BLACK;

/// Black with 80% opacity - for overlays and tooltip backgrounds
pub const BLACK_80: Color = Color::from_rgba(0.0, 0.0, 0.0, 0.8);

/// Charcoal - dark gray (0.15)
pub const CHARCOAL: Color = Color::from_rgb(0.15, 0.15, 0.15);

/// Cobalt blue - bright blue for sidebar icon hover (light theme)
pub const COBALT: Color = Color::from_rgb(0.2, 0.4, 0.8);

/// Coral - bright red for dark theme (1.0, 0.3, 0.3)
pub const CORAL: Color = Color::from_rgb(1.0, 0.3, 0.3);

/// Cornflower blue - light blue for sidebar icon hover (dark theme)
pub const CORNFLOWER: Color = Color::from_rgb(0.5, 0.7, 1.0);

/// Crimson - dark red for light theme (0.8, 0.0, 0.0)
pub const CRIMSON: Color = Color::from_rgb(0.8, 0.0, 0.0);

/// Dark slate gray (0.35)
pub const DARK_SLATE: Color = Color::from_rgb(0.35, 0.35, 0.35);

/// Dim gray (0.3)
pub const DIM_GRAY: Color = Color::from_rgb(0.3, 0.3, 0.3);

/// Ebony - very dark gray, nearly black (0.12)
pub const EBONY: Color = Color::from_rgb(0.12, 0.12, 0.12);

/// Gainsboro - very light gray (0.8)
pub const GAINSBORO: Color = Color::from_rgb(0.8, 0.8, 0.8);

/// Granite - medium-dark gray (0.4)
pub const GRANITE: Color = Color::from_rgb(0.4, 0.4, 0.4);

/// Gunmetal - dark gray (0.25)
pub const GUNMETAL: Color = Color::from_rgb(0.25, 0.25, 0.25);

/// Jet - very dark gray (0.2)
pub const JET: Color = Color::from_rgb(0.2, 0.2, 0.2);

/// Light slate gray (0.6)
pub const LIGHT_SLATE: Color = Color::from_rgb(0.6, 0.6, 0.6);

/// Pewter - very light gray (0.9)
pub const PEWTER: Color = Color::from_rgb(0.9, 0.9, 0.9);

/// Platinum - near-white gray (0.92)
pub const PLATINUM: Color = Color::from_rgb(0.92, 0.92, 0.92);

/// Silver - light gray (0.7)
pub const SILVER: Color = Color::from_rgb(0.7, 0.7, 0.7);

/// Sky blue - light blue for info text (dark theme)
pub const SKY_BLUE: Color = Color::from_rgb(0.5, 0.8, 1.0);

/// Slate gray - medium gray (0.5)
pub const SLATE: Color = Color::from_rgb(0.5, 0.5, 0.5);

/// Smoke - near-white gray (0.95)
pub const SMOKE: Color = Color::from_rgb(0.95, 0.95, 0.95);

/// Steel blue - our signature blue (0.3, 0.5, 0.7)
pub const STEEL_BLUE: Color = Color::from_rgb(0.3, 0.5, 0.7);

/// Steel blue dark - darker variant for pressed states (0.25, 0.45, 0.65)
pub const STEEL_BLUE_DARK: Color = Color::from_rgb(0.25, 0.45, 0.65);

/// Steel blue pale - lighter variant for hover states (0.35, 0.55, 0.75)
pub const STEEL_BLUE_PALE: Color = Color::from_rgb(0.35, 0.55, 0.75);

/// White - pure white
pub const WHITE: Color = Color::WHITE;
