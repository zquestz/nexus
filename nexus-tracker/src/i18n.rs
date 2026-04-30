//! Internationalization support using Fluent
//!
//! Tracker errors localize via Fluent. v0.1.0 ships English only; other
//! locales are added before release. Unsupported locales fall back to
//! English. A key missing in English panics — the assumption is that the
//! call site can't reference a key that doesn't exist in the bundled
//! FTL, so a missing-key panic flags a programming error, not an
//! operator-actionable failure.

use fluent_bundle::{FluentArgs, FluentBundle, FluentResource, FluentValue};
use tracing::warn;
use unic_langid::LanguageIdentifier;

use crate::constants::{
    DEFAULT_LOCALE, ERR_DEFAULT_LOCALE_INVALID, ERR_I18N_ADD_RESOURCE,
    ERR_I18N_MISSING_KEY_ENGLISH, ERR_I18N_PARSE_FTL, LOG_MISSING_TRANSLATION_KEY,
    LOG_TRANSLATION_ERRORS,
};

/// Resolve a translation key in the given locale. Falls back to English
/// when the key is missing in the requested locale. Panics if the key
/// is missing in English (programming error).
#[must_use]
pub fn t(locale: &str, key: &str) -> String {
    let bundle = get_bundle(locale);

    if let Some(msg) = bundle.get_message(key).and_then(|m| m.value()) {
        let mut errors = vec![];
        let value = bundle.format_pattern(msg, None, &mut errors);

        if !errors.is_empty() {
            warn!(key = %key, errors = ?errors, "{}", LOG_TRANSLATION_ERRORS);
        }

        return value.to_string();
    }

    warn!(key = %key, locale = %locale, "{}", LOG_MISSING_TRANSLATION_KEY);
    if locale != DEFAULT_LOCALE {
        return t(DEFAULT_LOCALE, key);
    }
    panic!("{} '{}'", ERR_I18N_MISSING_KEY_ENGLISH, key);
}

/// Resolve a translation key with parameter substitution. Behaves like
/// [`t`] for fallback semantics; arguments are matched against `$name`
/// placeholders in the FTL pattern.
///
/// Integer-shaped values are passed as `FluentValue::Number` so Fluent
/// numeric selectors (e.g., plural forms) match correctly.
#[must_use]
pub fn t_args(locale: &str, key: &str, args: &[(&str, &str)]) -> String {
    let bundle = get_bundle(locale);

    if let Some(msg) = bundle.get_message(key).and_then(|m| m.value()) {
        let mut fluent_args = FluentArgs::new();
        for (k, v) in args {
            if let Ok(n) = v.parse::<i64>() {
                fluent_args.set(*k, FluentValue::from(n));
            } else {
                fluent_args.set(*k, FluentValue::from(*v));
            }
        }

        let mut errors = vec![];
        let value = bundle.format_pattern(msg, Some(&fluent_args), &mut errors);

        if !errors.is_empty() {
            warn!(key = %key, errors = ?errors, "{}", LOG_TRANSLATION_ERRORS);
        }

        return value.to_string();
    }

    warn!(key = %key, locale = %locale, "{}", LOG_MISSING_TRANSLATION_KEY);
    if locale != DEFAULT_LOCALE {
        return t_args(DEFAULT_LOCALE, key, args);
    }
    panic!("{} '{}'", ERR_I18N_MISSING_KEY_ENGLISH, key);
}

/// Build a Fluent bundle for the requested locale. Currently English-only;
/// other locales fall back to English. Each call constructs a fresh
/// bundle — `FluentBundle` contains non-`Send` types, and the tracker's
/// translation rate is low enough that caching isn't worth the complexity.
fn get_bundle(locale: &str) -> FluentBundle<FluentResource> {
    let lang: LanguageIdentifier = locale
        .parse()
        .unwrap_or_else(|_| DEFAULT_LOCALE.parse().expect(ERR_DEFAULT_LOCALE_INVALID));

    let mut bundle = FluentBundle::new(vec![lang]);

    // English-only for v0.1.0. Add a `match` arm per locale here as
    // translations land.
    let ftl_string = include_str!("../locales/en/errors.ftl");

    let resource = FluentResource::try_new(ftl_string.to_string()).expect(ERR_I18N_PARSE_FTL);
    bundle.add_resource(resource).expect(ERR_I18N_ADD_RESOURCE);
    bundle
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_known_key_resolves_to_english_text() {
        assert_eq!(
            t("en", "err-tracker-unauthorized"),
            "Wrong or missing password"
        );
    }

    #[test]
    fn test_t_args_substitutes_parameters() {
        let result = t_args("en", "err-tracker-name-too-long", &[("max_length", "64")]);
        assert!(
            result.contains("Server name is too long"),
            "expected the prefix, got: {result}"
        );
        // Fluent wraps substituted values in Unicode directional markers,
        // so just check for the digit substring.
        assert!(
            result.contains("64"),
            "expected '64' in result, got: {result}"
        );
        assert!(
            result.contains("bytes"),
            "expected 'bytes' in result, got: {result}"
        );
    }

    #[test]
    fn test_unknown_locale_falls_back_to_english() {
        // Until other locales ship, every locale falls back to English.
        // Once they're populated, this test should still pass for an
        // unsupported locale like "xx".
        assert_eq!(
            t("xx", "err-tracker-unauthorized"),
            "Wrong or missing password"
        );
    }

    #[test]
    #[should_panic(expected = "Missing translation key in English")]
    fn test_missing_key_in_english_panics() {
        let _ = t("en", "err-tracker-this-key-does-not-exist");
    }
}
