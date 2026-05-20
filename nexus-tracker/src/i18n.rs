//! Fluent i18n for tracker errors. 13 locales ship in
//! `nexus-tracker/locales/`; unsupported locales (and generic `pt`/`zh`)
//! fall back to a regional variant or English. A key missing in English
//! panics — that's a programming error, not an operator-actionable one.

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

/// Resolve `key` in `locale`, falling back to English. Panics if the
/// key is missing in English.
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

/// Like [`t`], with `$name` argument substitution. Integer-shaped
/// values pass as `FluentValue::Number` so numeric selectors (plurals)
/// match.
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

/// Build a fresh Fluent bundle for `locale` (English fallback).
/// Uncached: `FluentBundle` is non-`Send` and the translation rate is
/// too low to justify the complexity.
fn get_bundle(locale: &str) -> FluentBundle<FluentResource> {
    let lang: LanguageIdentifier = locale
        .parse()
        .unwrap_or_else(|_| DEFAULT_LOCALE.parse().expect(ERR_DEFAULT_LOCALE_INVALID));

    let mut bundle = FluentBundle::new(vec![lang]);

    // Generic locales (`pt`, `zh`) map to their default regional variants.
    let normalized_locale = match locale {
        LOCALE_PORTUGUESE => LOCALE_PORTUGUESE_BR,
        LOCALE_CHINESE => LOCALE_CHINESE_CN,
        other => other,
    };

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
        // Unsupported locale resolves via the `_` arm to the EN bundle.
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

    // Spot-check one key per locale: each `.ftl` is wired in and parses.

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
        // Argument substitution works in a non-EN bundle (no fallback).
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

    // The two key-coverage tests below catch untranslated-new-key and
    // stale-renamed-key mistakes at test time.

    /// Top-level message keys in a Fluent `.ftl`, via the real parser.
    fn collect_keys_in_ftl(path: &std::path::Path) -> std::collections::HashSet<String> {
        use fluent_syntax::ast::Entry;
        let content = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("failed to read {}: {}", path.display(), e));
        let resource = fluent_syntax::parser::parse(content.as_str()).unwrap_or_else(|(r, _)| r);
        resource
            .body
            .into_iter()
            .filter_map(|entry| match entry {
                Entry::Message(msg) => Some(msg.id.name.to_string()),
                _ => None,
            })
            .collect()
    }

    /// Non-EN locale codes, mirroring dirs under `locales/`.
    const NON_EN_LOCALES: &[&str] = &[
        "es", "fr", "de", "it", "nl", "pt-BR", "pt-PT", "ru", "ja", "zh-CN", "zh-TW", "ko",
    ];

    fn locale_errors_path(locale: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("locales")
            .join(locale)
            .join("errors.ftl")
    }

    #[test]
    fn every_en_key_exists_in_all_locales() {
        let en_keys = collect_keys_in_ftl(&locale_errors_path("en"));
        assert!(
            !en_keys.is_empty(),
            "en/errors.ftl scanner found zero keys — path or parser likely broken"
        );

        let mut report: Vec<String> = Vec::new();
        for locale in NON_EN_LOCALES {
            let locale_keys = collect_keys_in_ftl(&locale_errors_path(locale));
            let mut missing: Vec<&String> = en_keys.difference(&locale_keys).collect();
            missing.sort();
            if !missing.is_empty() {
                report.push(format!(
                    "[{}] missing {} key(s): {:#?}",
                    locale,
                    missing.len(),
                    missing
                ));
            }
        }

        assert!(
            report.is_empty(),
            "Locales missing keys present in en/errors.ftl. Translate the \
             missing keys (or remove them from EN if intentional):\n\n{}",
            report.join("\n\n"),
        );
    }

    #[test]
    fn no_orphan_keys_in_non_en_locales() {
        let en_keys = collect_keys_in_ftl(&locale_errors_path("en"));
        assert!(!en_keys.is_empty());

        let mut report: Vec<String> = Vec::new();
        for locale in NON_EN_LOCALES {
            let locale_keys = collect_keys_in_ftl(&locale_errors_path(locale));
            let mut orphans: Vec<&String> = locale_keys.difference(&en_keys).collect();
            orphans.sort();
            if !orphans.is_empty() {
                report.push(format!(
                    "[{}] {} orphan key(s) (in this locale but not in EN): {:#?}",
                    locale,
                    orphans.len(),
                    orphans
                ));
            }
        }

        assert!(
            report.is_empty(),
            "Non-EN locales carry keys not present in en/errors.ftl. \
             These are leftovers from a rename or removal — drop them \
             from the locale file:\n\n{}",
            report.join("\n\n"),
        );
    }
}
