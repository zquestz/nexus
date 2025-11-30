//! i18n constants (locale codes and error messages)

// =============================================================================
// Error Messages (operator-facing, not translated)
// =============================================================================

/// Error when adding resource to bundle fails
pub(super) const ERR_ADD_RESOURCE: &str = "Failed to add resource to bundle";

/// Error when translation key is missing
pub(super) const ERR_MISSING_KEY: &str = "Missing translation key";

/// Error when translation key is missing in English
pub(super) const ERR_MISSING_KEY_ENGLISH: &str = "Missing translation key in English";

/// Error when FTL file parsing fails
pub(super) const ERR_PARSE_FTL: &str = "Failed to parse FTL file";

/// Error when translation has formatting errors
pub(super) const ERR_TRANSLATION_ERRORS: &str = "Translation errors for key";

/// "for locale" - used in error messages
pub(super) const MSG_FOR_LOCALE: &str = "for locale";

// =============================================================================
// Locale Constants
// =============================================================================

/// Default locale (English)
pub const DEFAULT_LOCALE: &str = "en";

/// Supported locale: Chinese (generic/Simplified)
pub(super) const LOCALE_CHINESE: &str = "zh";

/// Supported locale: Chinese (Simplified)
pub(super) const LOCALE_CHINESE_CN: &str = "zh-CN";

/// Supported locale: Chinese (Traditional)
pub(super) const LOCALE_CHINESE_TW: &str = "zh-TW";

/// Supported locale: Dutch
pub(super) const LOCALE_DUTCH: &str = "nl";

/// Supported locale: French
pub(super) const LOCALE_FRENCH: &str = "fr";

/// Supported locale: German
pub(super) const LOCALE_GERMAN: &str = "de";

/// Supported locale: Italian
pub(super) const LOCALE_ITALIAN: &str = "it";

/// Supported locale: Japanese
pub(super) const LOCALE_JAPANESE: &str = "ja";

/// Supported locale: Korean
pub(super) const LOCALE_KOREAN: &str = "ko";

/// Supported locale: Portuguese (generic/Brazilian)
pub(super) const LOCALE_PORTUGUESE: &str = "pt";

/// Supported locale: Portuguese (Brazil)
pub(super) const LOCALE_PORTUGUESE_BR: &str = "pt-BR";

/// Supported locale: Portuguese (Portugal)
pub(super) const LOCALE_PORTUGUESE_PT: &str = "pt-PT";

/// Supported locale: Russian
pub(super) const LOCALE_RUSSIAN: &str = "ru";

/// Supported locale: Spanish
pub(super) const LOCALE_SPANISH: &str = "es";
