//! Locale validation
//!
//! Validates locale/language code strings.

/// Maximum length for locale strings in characters
///
/// Locales like "zh-Hant-TW" are 10 characters, so 16 gives some headroom.
pub const MAX_LOCALE_LENGTH: usize = 16;

/// Validation error for locales
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocaleError {
    /// Locale exceeds maximum length
    TooLong,
    /// Locale contains invalid characters
    InvalidCharacters,
}

/// Validate a locale string
///
/// Checks:
/// - Does not exceed maximum length (16 characters)
/// - No control characters
///
/// Note: Empty locale is allowed (will use server default).
///
/// # Errors
///
/// Returns a `LocaleError` variant describing the validation failure.
pub fn validate_locale(locale: &str) -> Result<(), LocaleError> {
    if locale.len() > MAX_LOCALE_LENGTH {
        return Err(LocaleError::TooLong);
    }
    for ch in locale.chars() {
        if ch.is_control() {
            return Err(LocaleError::InvalidCharacters);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_locales() {
        assert!(validate_locale("en").is_ok());
        assert!(validate_locale("en-US").is_ok());
        assert!(validate_locale("zh-CN").is_ok());
        assert!(validate_locale("zh-TW").is_ok());
        assert!(validate_locale("zh-Hant-TW").is_ok());
        assert!(validate_locale("pt-BR").is_ok());
        assert!(validate_locale("pt-PT").is_ok());
        assert!(validate_locale(&"a".repeat(MAX_LOCALE_LENGTH)).is_ok());
    }

    #[test]
    fn test_empty_allowed() {
        assert!(validate_locale("").is_ok());
    }

    #[test]
    fn test_too_long() {
        assert_eq!(
            validate_locale(&"a".repeat(MAX_LOCALE_LENGTH + 1)),
            Err(LocaleError::TooLong)
        );
    }

    #[test]
    fn test_control_characters() {
        assert_eq!(
            validate_locale("en\0US"),
            Err(LocaleError::InvalidCharacters)
        );
        assert_eq!(
            validate_locale("en\nUS"),
            Err(LocaleError::InvalidCharacters)
        );
        assert_eq!(
            validate_locale("en\tUS"),
            Err(LocaleError::InvalidCharacters)
        );
    }
}
