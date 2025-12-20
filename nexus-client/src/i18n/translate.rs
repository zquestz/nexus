//! Translation functions

use fluent_bundle::{FluentArgs, FluentValue};

use super::bundle::get_bundle;
use super::constants::*;
use super::locale::get_locale;

/// Get a translated message using the current locale
///
/// # Arguments
/// * `key` - The translation key from the .ftl file
///
/// # Returns
/// The translated string, or falls back to English if locale not supported
///
/// # Example
/// ```ignore
/// let cancel = t("button-cancel"); // "Cancel"
/// ```
pub fn t(key: &str) -> String {
    translate(get_locale(), key)
}

/// Get a translated message with arguments using the current locale
///
/// # Arguments
/// * `key` - The translation key from the .ftl file
/// * `args` - Slice of (key, value) tuples for parameter substitution
///
/// # Returns
/// The translated string with parameters substituted
///
/// # Example
/// ```ignore
/// let msg = t_args("msg-user-connected", &[("nickname", "alice")]);
/// ```
pub fn t_args(key: &str, args: &[(&str, &str)]) -> String {
    translate_with_args(get_locale(), key, args)
}

/// Get a translated message for a specific locale
///
/// Falls back to English if the key is missing in the requested locale.
fn translate(locale: &str, key: &str) -> String {
    let bundle = get_bundle(locale);

    if let Some(msg) = bundle.get_message(key).and_then(|m| m.value()) {
        let mut errors = vec![];
        let value = bundle.format_pattern(msg, None, &mut errors);

        if !errors.is_empty() {
            eprintln!("{} '{}': {:?}", ERR_TRANSLATION_ERRORS, key, errors);
        }

        return value.to_string();
    }

    // Fallback to English if key missing in requested locale
    eprintln!(
        "{} '{}' {} {}",
        ERR_MISSING_KEY, key, MSG_FOR_LOCALE, locale
    );
    if locale != DEFAULT_LOCALE {
        return translate(DEFAULT_LOCALE, key);
    }

    panic!("{} '{}'", ERR_MISSING_KEY_ENGLISH, key);
}

/// Get a translated message with arguments for a specific locale
///
/// Falls back to English if the key is missing in the requested locale.
fn translate_with_args(locale: &str, key: &str, args: &[(&str, &str)]) -> String {
    let bundle = get_bundle(locale);

    if let Some(msg) = bundle.get_message(key).and_then(|m| m.value()) {
        let mut fluent_args = FluentArgs::new();
        for (k, v) in args {
            fluent_args.set(*k, FluentValue::from(*v));
        }

        let mut errors = vec![];
        let value = bundle.format_pattern(msg, Some(&fluent_args), &mut errors);

        if !errors.is_empty() {
            eprintln!("{} '{}': {:?}", ERR_TRANSLATION_ERRORS, key, errors);
        }

        return value.to_string();
    }

    // Fallback to English if key missing in requested locale
    eprintln!(
        "{} '{}' {} {}",
        ERR_MISSING_KEY, key, MSG_FOR_LOCALE, locale
    );
    if locale != DEFAULT_LOCALE {
        return translate_with_args(DEFAULT_LOCALE, key, args);
    }

    panic!("{} '{}'", ERR_MISSING_KEY_ENGLISH, key);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_translation_english() {
        let result = translate("en", "button-cancel");
        assert_eq!(result, "Cancel");
    }

    #[test]
    fn test_translation_with_args_english() {
        let result = translate_with_args("en", "msg-user-connected", &[("nickname", "alice")]);
        // Fluent adds Unicode directional markers around substituted values
        assert!(result.contains("alice"));
        assert!(result.contains("connected"));
    }

    #[test]
    fn test_fallback_to_english() {
        let result = translate("xx", "button-cancel");
        assert_eq!(result, "Cancel");
    }
}
