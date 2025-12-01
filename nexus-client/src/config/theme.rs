//! Theme preference configuration
//!
//! Uses Iced's built-in Theme enum directly, with string-based serialization
//! that matches Theme's Display implementation.

use iced::Theme;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Theme preference wrapper that enables serialization of iced::Theme
///
/// Serializes as the theme's display name (e.g., "Catppuccin Frappé").
/// Defaults to Dark theme if deserialization fails.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ThemePreference(pub Theme);

impl ThemePreference {
    /// Get the inner iced Theme
    pub fn to_iced_theme(&self) -> Theme {
        self.0.clone()
    }
}

impl From<Theme> for ThemePreference {
    fn from(theme: Theme) -> Self {
        Self(theme)
    }
}

impl std::fmt::Display for ThemePreference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for ThemePreference {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Serialize as the display name string
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for ThemePreference {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = String::deserialize(deserializer)?;

        // Find matching theme by display name
        let theme = Theme::ALL
            .iter()
            .find(|t| t.to_string() == name)
            .cloned()
            .unwrap_or(Theme::Dark);

        Ok(Self(theme))
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_dark() {
        assert_eq!(ThemePreference::default().0, Theme::Dark);
    }

    #[test]
    fn test_serialization_roundtrip() {
        for theme in Theme::ALL {
            let pref = ThemePreference(theme.clone());
            let serialized = serde_json::to_string(&pref).expect("serialize");
            let deserialized: ThemePreference =
                serde_json::from_str(&serialized).expect("deserialize");
            assert_eq!(pref.0, deserialized.0);
        }
    }

    #[test]
    fn test_serializes_as_display_name() {
        let pref = ThemePreference(Theme::CatppuccinFrappe);
        let serialized = serde_json::to_string(&pref).expect("serialize");
        assert_eq!(serialized, "\"Catppuccin Frappé\"");

        // Verify it's a plain string, not an object
        let dark = ThemePreference(Theme::Dark);
        let serialized = serde_json::to_string(&dark).expect("serialize");
        assert_eq!(serialized, "\"Dark\"");
    }

    #[test]
    fn test_config_serialization_format() {
        // Verify the theme appears as a string in the full config JSON
        use crate::config::{Config, Settings};

        let config = Config {
            settings: Settings {
                theme: ThemePreference(Theme::Nord),
                ..Default::default()
            },
            ..Default::default()
        };

        let json = serde_json::to_string_pretty(&config).expect("serialize");
        assert!(
            json.contains("\"theme\": \"Nord\""),
            "Expected theme as string, got: {}",
            json
        );
    }

    #[test]
    fn test_unknown_theme_defaults_to_dark() {
        let deserialized: ThemePreference =
            serde_json::from_str("\"Unknown Theme\"").expect("deserialize");
        assert_eq!(deserialized.0, Theme::Dark);
    }

    #[test]
    fn test_backwards_compatible_with_old_format() {
        // Old format used variant names like "Dark" or "Light"
        let dark: ThemePreference = serde_json::from_str("\"Dark\"").expect("deserialize");
        assert_eq!(dark.0, Theme::Dark);

        let light: ThemePreference = serde_json::from_str("\"Light\"").expect("deserialize");
        assert_eq!(light.0, Theme::Light);
    }
}
