//! Internationalization support using Fluent
//!
//! This module provides translation functions for the client UI.
//! It follows the same pattern as the server's i18n module.
//!
//! ## Permission Translation
//!
//! Permission names (like "user_list", "chat_send") are translated using the
//! `translate_permission()` function, which looks up the corresponding
//! "permission-{name}" key in the translation files.

use fluent_bundle::{FluentArgs, FluentBundle, FluentResource, FluentValue};
use once_cell::sync::Lazy;
use std::sync::RwLock;
use unic_langid::LanguageIdentifier;

// =============================================================================
// Locale Constants
// =============================================================================

/// Default locale (English)
pub const DEFAULT_LOCALE: &str = "en";

/// Supported locale: Spanish
pub const LOCALE_SPANISH: &str = "es";

/// Supported locale: Japanese
pub const LOCALE_JAPANESE: &str = "ja";

/// Supported locale: French
pub const LOCALE_FRENCH: &str = "fr";

/// Supported locale: German
pub const LOCALE_GERMAN: &str = "de";

/// Supported locale: Portuguese (generic/Brazilian)
pub const LOCALE_PORTUGUESE: &str = "pt";

/// Supported locale: Portuguese (Portugal)
pub const LOCALE_PORTUGUESE_PT: &str = "pt-PT";

/// Supported locale: Portuguese (Brazil)
pub const LOCALE_PORTUGUESE_BR: &str = "pt-BR";

/// Supported locale: Russian
pub const LOCALE_RUSSIAN: &str = "ru";

/// Supported locale: Chinese (generic/Simplified)
pub const LOCALE_CHINESE: &str = "zh";

/// Supported locale: Chinese (Simplified)
pub const LOCALE_CHINESE_CN: &str = "zh-CN";

/// Supported locale: Chinese (Traditional)
pub const LOCALE_CHINESE_TW: &str = "zh-TW";

/// Supported locale: Korean
pub const LOCALE_KOREAN: &str = "ko";

/// Supported locale: Italian
pub const LOCALE_ITALIAN: &str = "it";

/// Supported locale: Dutch
pub const LOCALE_DUTCH: &str = "nl";

// =============================================================================
// Error Messages (operator-facing, not translated)
// =============================================================================

/// Error when translation key is missing
const ERR_I18N_MISSING_KEY: &str = "Missing translation key";

/// Error when translation key is missing in English
const ERR_I18N_MISSING_KEY_ENGLISH: &str = "Missing translation key in English";

/// Error when translation has formatting errors
const ERR_I18N_TRANSLATION_ERRORS: &str = "Translation errors for key";

/// "for locale" - used in i18n error messages
const MSG_I18N_FOR_LOCALE: &str = "for locale";

/// Error when FTL file parsing fails
const ERR_I18N_PARSE_FTL: &str = "Failed to parse FTL file";

/// Error when adding resource to bundle fails
const ERR_I18N_ADD_RESOURCE: &str = "Failed to add resource to bundle";

// =============================================================================
// Global Locale State
// =============================================================================

/// Global locale setting, detected at startup and used for all translations
static CURRENT_LOCALE: Lazy<RwLock<String>> = Lazy::new(|| {
    let locale = sys_locale::get_locale()
        .map(|loc| {
            // Handle locales like "en-US" -> "en" but preserve "zh-CN", "pt-BR", etc.
            let parts: Vec<&str> = loc.split('-').collect();
            if parts.len() >= 2 {
                let lang = parts[0].to_lowercase();
                let region = parts[1].to_uppercase();
                // Keep region for languages with significant regional variants
                match lang.as_str() {
                    "zh" => format!("{}-{}", lang, region),
                    "pt" => format!("{}-{}", lang, region),
                    _ => lang,
                }
            } else {
                loc.to_lowercase()
            }
        })
        .unwrap_or_else(|| DEFAULT_LOCALE.to_string());
    RwLock::new(locale)
});

/// Get the current locale
pub fn get_locale() -> String {
    CURRENT_LOCALE.read().unwrap().clone()
}

/// Set the current locale (for testing or manual override)
#[allow(dead_code)]
pub fn set_locale(locale: &str) {
    let mut current = CURRENT_LOCALE.write().unwrap();
    *current = locale.to_string();
}

// =============================================================================
// Translation Functions
// =============================================================================

/// Get a translated message using the current locale
///
/// # Arguments
/// * `key` - The translation key from the .ftl file
///
/// # Returns
/// The translated string, or falls back to English if locale not supported
pub fn t(key: &str) -> String {
    let locale = get_locale();
    t_with_locale(&locale, key)
}

/// Get a translated message with arguments using the current locale
///
/// # Arguments
/// * `key` - The translation key from the .ftl file
/// * `args` - Slice of (key, value) tuples for parameter substitution
///
/// # Returns
/// The translated string with parameters substituted
pub fn t_args(key: &str, args: &[(&str, &str)]) -> String {
    let locale = get_locale();
    t_args_with_locale(&locale, key, args)
}

/// Translate a permission name to a user-friendly display string
///
/// Looks up the translation key "permission-{permission_name}" and returns
/// the localized string. Falls back to replacing underscores with spaces
/// if no translation is found.
///
/// # Arguments
/// * `permission` - The permission name (e.g., "user_list", "chat_send")
///
/// # Returns
/// The translated permission name, or a fallback with underscores replaced by spaces
///
/// # Example
/// ```
/// let display = translate_permission("user_list"); // "User List" in English
/// ```
pub fn translate_permission(permission: &str) -> String {
    let key = format!("permission-{}", permission);
    let locale = get_locale();
    let bundle = get_bundle(&locale);

    if let Some(msg) = bundle.get_message(&key).and_then(|m| m.value()) {
        let mut errors = vec![];
        let value = bundle.format_pattern(msg, None, &mut errors);
        return value.to_string();
    }

    // Fallback: replace underscores with spaces and title case
    permission.replace('_', " ")
}

/// Get a translated message for a specific locale
///
/// # Arguments
/// * `locale` - The locale code (e.g., "en", "es")
/// * `key` - The translation key from the .ftl file
///
/// # Returns
/// The translated string, or falls back to English if locale not supported
fn t_with_locale(locale: &str, key: &str) -> String {
    let bundle = get_bundle(locale);

    if let Some(msg) = bundle.get_message(key).and_then(|m| m.value()) {
        let mut errors = vec![];
        let value = bundle.format_pattern(msg, None, &mut errors);

        if !errors.is_empty() {
            eprintln!("{} '{}': {:?}", ERR_I18N_TRANSLATION_ERRORS, key, errors);
        }

        return value.to_string();
    }

    // Fallback to English if key missing in requested locale
    eprintln!(
        "{} '{}' {} {}",
        ERR_I18N_MISSING_KEY, key, MSG_I18N_FOR_LOCALE, locale
    );
    if locale != DEFAULT_LOCALE {
        return t_with_locale(DEFAULT_LOCALE, key);
    }

    panic!("{} '{}'", ERR_I18N_MISSING_KEY_ENGLISH, key);
}

/// Get a translated message with arguments for a specific locale
///
/// # Arguments
/// * `locale` - The locale code (e.g., "en", "es")
/// * `key` - The translation key from the .ftl file
/// * `args` - Slice of (key, value) tuples for parameter substitution
///
/// # Returns
/// The translated string with parameters substituted
fn t_args_with_locale(locale: &str, key: &str, args: &[(&str, &str)]) -> String {
    let bundle = get_bundle(locale);

    if let Some(msg) = bundle.get_message(key).and_then(|m| m.value()) {
        let mut fluent_args = FluentArgs::new();
        for (k, v) in args {
            fluent_args.set(*k, FluentValue::from(*v));
        }

        let mut errors = vec![];
        let value = bundle.format_pattern(msg, Some(&fluent_args), &mut errors);

        if !errors.is_empty() {
            eprintln!("{} '{}': {:?}", ERR_I18N_TRANSLATION_ERRORS, key, errors);
        }

        return value.to_string();
    }

    // Fallback to English if key missing in requested locale
    eprintln!(
        "{} '{}' {} {}",
        ERR_I18N_MISSING_KEY, key, MSG_I18N_FOR_LOCALE, locale
    );
    if locale != DEFAULT_LOCALE {
        return t_args_with_locale(DEFAULT_LOCALE, key, args);
    }

    panic!("{} '{}'", ERR_I18N_MISSING_KEY_ENGLISH, key);
}

/// Get a Fluent bundle for the specified locale
///
/// Loads the appropriate .ftl file and creates a bundle.
/// Falls back to English for unsupported locales.
///
/// Note: Currently creates a new bundle on each call. FluentBundle contains
/// non-Send types (RefCell, TypeMap) which prevent safe caching across threads.
/// For a GUI client, this performance trade-off is acceptable.
fn get_bundle(locale: &str) -> FluentBundle<FluentResource> {
    let lang: LanguageIdentifier = locale
        .parse()
        .unwrap_or_else(|_| DEFAULT_LOCALE.parse().unwrap());

    let mut bundle = FluentBundle::new(vec![lang]);

    // Normalize locale to actual file location
    // Generic locales (pt, zh) map to their default variants
    let normalized_locale = match locale {
        LOCALE_PORTUGUESE => LOCALE_PORTUGUESE_BR, // "pt" -> "pt-BR"
        LOCALE_CHINESE => LOCALE_CHINESE_CN,       // "zh" -> "zh-CN"
        other => other,
    };

    // Load ui.ftl for this locale (fallback to English)
    let ftl_string = match normalized_locale {
        LOCALE_SPANISH => include_str!("../locales/es/ui.ftl"),
        LOCALE_JAPANESE => include_str!("../locales/ja/ui.ftl"),
        LOCALE_FRENCH => include_str!("../locales/fr/ui.ftl"),
        LOCALE_GERMAN => include_str!("../locales/de/ui.ftl"),
        LOCALE_PORTUGUESE_PT => include_str!("../locales/pt-PT/ui.ftl"),
        LOCALE_PORTUGUESE_BR => include_str!("../locales/pt-BR/ui.ftl"),
        LOCALE_RUSSIAN => include_str!("../locales/ru/ui.ftl"),
        LOCALE_CHINESE_CN => include_str!("../locales/zh-CN/ui.ftl"),
        LOCALE_CHINESE_TW => include_str!("../locales/zh-TW/ui.ftl"),
        LOCALE_KOREAN => include_str!("../locales/ko/ui.ftl"),
        LOCALE_ITALIAN => include_str!("../locales/it/ui.ftl"),
        LOCALE_DUTCH => include_str!("../locales/nl/ui.ftl"),
        _ => include_str!("../locales/en/ui.ftl"),
    };

    let resource = FluentResource::try_new(ftl_string.to_string()).expect(ERR_I18N_PARSE_FTL);

    bundle.add_resource(resource).expect(ERR_I18N_ADD_RESOURCE);

    bundle
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_translation_english() {
        set_locale("en");
        let result = t("button-cancel");
        assert_eq!(result, "Cancel");
    }

    #[test]
    fn test_translation_with_args_english() {
        set_locale("en");
        let result = t_args("msg-user-connected", &[("username", "alice")]);
        // Fluent adds Unicode directional markers around substituted values
        assert!(result.contains("alice"));
        assert!(result.contains("connected"));
    }

    #[test]
    fn test_fallback_to_english() {
        // Use t_with_locale directly to test fallback without affecting global state
        let result = t_with_locale("xx", "button-cancel");
        assert_eq!(result, "Cancel");
    }
}
