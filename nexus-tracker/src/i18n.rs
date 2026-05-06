//! Internationalization support using Fluent
//!
//! Tracker errors localize via Fluent. 13 locales ship in
//! `nexus-tracker/locales/`; unsupported locales fall back to English.
//! Generic codes (`pt`, `zh`) normalize to a regional variant before
//! lookup. A key missing in English panics — the assumption is that
//! the call site can't reference a key that doesn't exist in the
//! bundled FTL, so a missing-key panic flags a programming error, not
//! an operator-actionable failure.

use fluent_bundle::{FluentArgs, FluentBundle, FluentResource, FluentValue};
use tracing::warn;
use unic_langid::LanguageIdentifier;

use crate::constants::{
    DEFAULT_LOCALE, ERR_DEFAULT_LOCALE_INVALID, ERR_I18N_ADD_RESOURCE,
    ERR_I18N_MISSING_KEY_ENGLISH, ERR_I18N_PARSE_FTL, LOCALE_CHINESE, LOCALE_CHINESE_CN,
    LOCALE_CHINESE_TW, LOCALE_DUTCH, LOCALE_FRENCH, LOCALE_GERMAN, LOCALE_ITALIAN, LOCALE_JAPANESE,
    LOCALE_KOREAN, LOCALE_PORTUGUESE, LOCALE_PORTUGUESE_BR, LOCALE_PORTUGUESE_PT, LOCALE_RUSSIAN,
    LOCALE_SPANISH, LOG_MISSING_TRANSLATION_KEY, LOG_TRANSLATION_ERRORS,
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

/// Build a Fluent bundle for the requested locale. Unsupported locales
/// fall back to English. Each call constructs a fresh bundle —
/// `FluentBundle` contains non-`Send` types, and the tracker's
/// translation rate is low enough that caching isn't worth the complexity.
fn get_bundle(locale: &str) -> FluentBundle<FluentResource> {
    let lang: LanguageIdentifier = locale
        .parse()
        .unwrap_or_else(|_| DEFAULT_LOCALE.parse().expect(ERR_DEFAULT_LOCALE_INVALID));

    let mut bundle = FluentBundle::new(vec![lang]);

    // Normalize locale to actual file location.
    // Generic locales (`pt`, `zh`) map to their default regional variants.
    let normalized_locale = match locale {
        LOCALE_PORTUGUESE => LOCALE_PORTUGUESE_BR, // "pt" -> "pt-BR"
        LOCALE_CHINESE => LOCALE_CHINESE_CN,       // "zh" -> "zh-CN"
        other => other,
    };

    // Load errors.ftl for this locale (fallback to English).
    let ftl_string = match normalized_locale {
        LOCALE_SPANISH => include_str!("../locales/es/errors.ftl"),
        LOCALE_JAPANESE => include_str!("../locales/ja/errors.ftl"),
        LOCALE_FRENCH => include_str!("../locales/fr/errors.ftl"),
        LOCALE_GERMAN => include_str!("../locales/de/errors.ftl"),
        LOCALE_PORTUGUESE_PT => include_str!("../locales/pt-PT/errors.ftl"),
        LOCALE_PORTUGUESE_BR => include_str!("../locales/pt-BR/errors.ftl"),
        LOCALE_RUSSIAN => include_str!("../locales/ru/errors.ftl"),
        LOCALE_CHINESE_CN => include_str!("../locales/zh-CN/errors.ftl"),
        LOCALE_CHINESE_TW => include_str!("../locales/zh-TW/errors.ftl"),
        LOCALE_KOREAN => include_str!("../locales/ko/errors.ftl"),
        LOCALE_ITALIAN => include_str!("../locales/it/errors.ftl"),
        LOCALE_DUTCH => include_str!("../locales/nl/errors.ftl"),
        _ => include_str!("../locales/en/errors.ftl"),
    };

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
            result.contains("characters"),
            "expected 'characters' in result, got: {result}"
        );
    }

    #[test]
    fn test_unknown_locale_falls_back_to_english() {
        // An unsupported locale (anything not in the `match` in `get_bundle`)
        // resolves via the `_` arm to the English bundle.
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

    // Per-locale resolution: ensures each bundled `.ftl` is wired up in
    // `get_bundle` and parses cleanly. Spot-checks one key per locale.

    #[test]
    fn test_translation_spanish() {
        assert_eq!(
            t("es", "err-tracker-unauthorized"),
            "Contraseña incorrecta o ausente"
        );
    }

    #[test]
    fn test_translation_french() {
        assert_eq!(
            t("fr", "err-tracker-unauthorized"),
            "Mot de passe incorrect ou manquant"
        );
    }

    #[test]
    fn test_translation_german() {
        assert_eq!(
            t("de", "err-tracker-unauthorized"),
            "Falsches oder fehlendes Passwort"
        );
    }

    #[test]
    fn test_translation_italian() {
        assert_eq!(
            t("it", "err-tracker-unauthorized"),
            "Password errata o mancante"
        );
    }

    #[test]
    fn test_translation_dutch() {
        assert_eq!(
            t("nl", "err-tracker-unauthorized"),
            "Wachtwoord onjuist of ontbrekend"
        );
    }

    #[test]
    fn test_translation_portuguese_br() {
        assert_eq!(
            t("pt-BR", "err-tracker-unauthorized"),
            "Senha incorreta ou ausente"
        );
    }

    #[test]
    fn test_translation_portuguese_pt() {
        assert_eq!(
            t("pt-PT", "err-tracker-unauthorized"),
            "Palavra-passe incorreta ou em falta"
        );
    }

    #[test]
    fn test_translation_portuguese_generic_maps_to_br() {
        // Generic "pt" should normalize to pt-BR.
        assert_eq!(
            t("pt", "err-tracker-unauthorized"),
            "Senha incorreta ou ausente"
        );
    }

    #[test]
    fn test_translation_russian() {
        assert_eq!(
            t("ru", "err-tracker-unauthorized"),
            "Неверный или отсутствующий пароль"
        );
    }

    #[test]
    fn test_translation_japanese() {
        assert_eq!(
            t("ja", "err-tracker-unauthorized"),
            "パスワードが間違っているか、指定されていません"
        );
    }

    #[test]
    fn test_translation_chinese_cn() {
        assert_eq!(t("zh-CN", "err-tracker-unauthorized"), "密码错误或缺失");
    }

    #[test]
    fn test_translation_chinese_tw() {
        assert_eq!(t("zh-TW", "err-tracker-unauthorized"), "密碼錯誤或缺失");
    }

    #[test]
    fn test_translation_chinese_generic_maps_to_cn() {
        // Generic "zh" should normalize to zh-CN.
        assert_eq!(t("zh", "err-tracker-unauthorized"), "密码错误或缺失");
    }

    #[test]
    fn test_translation_korean() {
        assert_eq!(
            t("ko", "err-tracker-unauthorized"),
            "잘못되었거나 누락된 비밀번호"
        );
    }

    #[test]
    fn test_t_args_substitutes_in_non_english_locale() {
        // Sanity-check that argument substitution works in a non-English
        // bundle and that the fallback path doesn't get triggered for a
        // known-good key/locale pair.
        let result = t_args("es", "err-tracker-name-too-long", &[("max_length", "64")]);
        assert!(
            result.contains("nombre del servidor"),
            "expected Spanish phrase, got: {result}"
        );
        assert!(
            result.contains("64"),
            "expected '64' in result, got: {result}"
        );
    }
}
