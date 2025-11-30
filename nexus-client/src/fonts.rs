//! Embedded fonts for consistent cross-platform rendering
//!
//! SauceCodePro Nerd Font Mono is embedded in the binary for consistent
//! rendering across all platforms. This avoids ligatures and ensures
//! predictable character spacing in text inputs and chat display.
//!
//! License: SIL Open Font License (OFL)

/// SauceCodePro Nerd Font Mono - Regular
pub const SAUCECODE_PRO_MONO: &[u8] =
    include_bytes!("../fonts/SauceCodeProNerdFontMono-Regular.ttf");

/// SauceCodePro Nerd Font Mono - Bold
pub const SAUCECODE_PRO_MONO_BOLD: &[u8] =
    include_bytes!("../fonts/SauceCodeProNerdFontMono-Bold.ttf");

/// SauceCodePro Nerd Font Mono - Italic
pub const SAUCECODE_PRO_MONO_ITALIC: &[u8] =
    include_bytes!("../fonts/SauceCodeProNerdFontMono-Italic.ttf");

/// SauceCodePro Nerd Font Mono - Bold Italic
pub const SAUCECODE_PRO_MONO_BOLD_ITALIC: &[u8] =
    include_bytes!("../fonts/SauceCodeProNerdFontMono-BoldItalic.ttf");
