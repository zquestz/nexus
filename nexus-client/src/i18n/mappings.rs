//! Translation key mappings for values defined outside the client
//!
//! These helpers map values from `nexus-common` types or wire protocol strings
//! to i18n translation keys. They exist here because the types themselves live
//! in `nexus-common` which has no locale files.

use nexus_common::validators::PasswordStrength;

/// Get the i18n translation key for a password strength level
pub fn strength_translation_key(strength: PasswordStrength) -> &'static str {
    match strength {
        PasswordStrength::Weak => "password-strength-weak",
        PasswordStrength::Fair => "password-strength-fair",
        PasswordStrength::Good => "password-strength-good",
        PasswordStrength::Strong => "password-strength-strong",
        PasswordStrength::Excellent => "password-strength-excellent",
    }
}

/// Get the i18n translation key for a log level wire value
pub fn log_level_translation_key(level: &str) -> &'static str {
    match level {
        "none" => "label-log-level-none",
        "error" => "label-log-level-error",
        "warn" => "label-log-level-warn",
        "info" => "label-log-level-info",
        "debug" => "label-log-level-debug",
        _ => "label-log-level-info",
    }
}
