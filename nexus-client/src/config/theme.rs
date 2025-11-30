//! Theme preference configuration

/// Theme preference (Light or Dark mode)
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum ThemePreference {
    #[default]
    Dark,
    Light,
}
