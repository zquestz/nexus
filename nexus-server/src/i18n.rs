//! Internationalization support using Fluent

use crate::constants::*;
use fluent_bundle::{FluentArgs, FluentBundle, FluentResource, FluentValue};
use unic_langid::LanguageIdentifier;

/// Get a translated message
///
/// # Arguments
/// * `locale` - The locale code (e.g., "en", "es")
/// * `key` - The translation key from the .ftl file
///
/// # Returns
/// The translated string, or falls back to English if locale not supported
pub fn t(locale: &str, key: &str) -> String {
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
        return t(DEFAULT_LOCALE, key);
    }

    panic!("{} '{}'", ERR_I18N_MISSING_KEY_ENGLISH, key);
}

/// Get a translated message with arguments
///
/// # Arguments
/// * `locale` - The locale code (e.g., "en", "es")
/// * `key` - The translation key from the .ftl file
/// * `args` - Slice of (key, value) tuples for parameter substitution
///
/// # Returns
/// The translated string with parameters substituted
pub fn t_args(locale: &str, key: &str, args: &[(&str, &str)]) -> String {
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
        return t_args(DEFAULT_LOCALE, key, args);
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
/// For a BBS server with infrequent errors, this performance trade-off is acceptable.
fn get_bundle(locale: &str) -> FluentBundle<FluentResource> {
    let lang: LanguageIdentifier = locale.parse().unwrap_or_else(|_| {
        DEFAULT_LOCALE
            .parse()
            .expect("DEFAULT_LOCALE is a valid locale")
    });

    let mut bundle = FluentBundle::new(vec![lang]);

    // Normalize locale to actual file location
    // Generic locales (pt, zh) map to their default variants
    let normalized_locale = match locale {
        LOCALE_PORTUGUESE => LOCALE_PORTUGUESE_BR, // "pt" -> "pt-BR"
        LOCALE_CHINESE => LOCALE_CHINESE_CN,       // "zh" -> "zh-CN"
        other => other,
    };

    // Load errors.ftl for this locale (fallback to English)
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
    fn test_translation_english() {
        let result = t("en", "err-not-logged-in");
        assert_eq!(result, "Not logged in");
    }

    #[test]
    fn test_translation_spanish() {
        let result = t("es", "err-not-logged-in");
        assert_eq!(result, "No has iniciado sesión");
    }

    #[test]
    fn test_translation_with_args_english() {
        let result = t_args("en", "err-username-exists", &[("username", "alice")]);
        // Fluent adds Unicode directional markers around substituted values
        assert!(result.contains("alice"));
        assert!(result.contains("Username"));
        assert!(result.contains("already exists"));
    }

    #[test]
    fn test_translation_with_args_spanish() {
        let result = t_args("es", "err-username-exists", &[("username", "alice")]);
        // Fluent adds Unicode directional markers around substituted values
        assert!(result.contains("alice"));
        assert!(result.contains("El nombre de usuario"));
        assert!(result.contains("ya existe"));
    }

    #[test]
    fn test_translation_multiple_args_english() {
        let result = t_args(
            "en",
            "err-version-major-mismatch",
            &[("server_major", "1"), ("client_major", "2")],
        );
        assert!(result.contains("Incompatible protocol version"));
        // Fluent wraps variables with Unicode directional markers, so just check the values
        assert!(result.contains('1'));
        assert!(result.contains('2'));
    }

    #[test]
    fn test_translation_multiple_args_spanish() {
        let result = t_args(
            "es",
            "err-version-major-mismatch",
            &[("server_major", "1"), ("client_major", "2")],
        );
        assert!(result.contains("incompatible"));
        // Fluent wraps variables with Unicode directional markers, so just check the values
        assert!(result.contains('1'));
        assert!(result.contains('2'));
    }

    #[test]
    fn test_fallback_to_english() {
        // Use unsupported locale, should fall back to English
        let result = t("xx", "err-not-logged-in");
        assert_eq!(result, "Not logged in");
    }

    #[test]
    fn test_numeric_args() {
        let result = t_args("en", "err-broadcast-too-long", &[("max_length", "1024")]);
        assert!(result.contains("Message too long"));
        assert!(result.contains("1024"));
        assert!(result.contains("characters"));
    }

    #[test]
    fn test_numeric_args_spanish() {
        let result = t_args("es", "err-broadcast-too-long", &[("max_length", "1024")]);
        assert!(result.contains("Mensaje demasiado largo"));
        assert!(result.contains("1024"));
        assert!(result.contains("caracteres"));
    }

    #[test]
    fn test_translation_japanese() {
        let result = t("ja", "err-not-logged-in");
        assert_eq!(result, "ログインしていません");
    }

    #[test]
    fn test_translation_with_args_japanese() {
        let result = t_args("ja", "err-username-exists", &[("username", "alice")]);
        assert!(result.contains("alice"));
        assert!(result.contains("ユーザー名"));
        assert!(result.contains("既に存在します"));
    }

    #[test]
    fn test_numeric_args_japanese() {
        let result = t_args("ja", "err-broadcast-too-long", &[("max_length", "1024")]);
        assert!(result.contains("メッセージが長すぎます"));
        assert!(result.contains("1024"));
        assert!(result.contains("文字"));
    }

    #[test]
    fn test_translation_french() {
        let result = t("fr", "err-not-logged-in");
        assert_eq!(result, "Non connecté");
    }

    #[test]
    fn test_translation_german() {
        let result = t("de", "err-not-logged-in");
        assert_eq!(result, "Nicht angemeldet");
    }

    #[test]
    fn test_translation_portuguese() {
        let result = t("pt", "err-not-logged-in");
        assert_eq!(result, "Não conectado"); // Defaults to pt-BR
    }

    #[test]
    fn test_translation_russian() {
        let result = t("ru", "err-not-logged-in");
        assert_eq!(result, "Не выполнен вход");
    }

    #[test]
    fn test_translation_chinese() {
        let result = t("zh", "err-not-logged-in");
        assert_eq!(result, "未登录"); // Defaults to zh-CN
    }

    #[test]
    fn test_translation_chinese_cn() {
        let result = t("zh-CN", "err-not-logged-in");
        assert_eq!(result, "未登录");
    }

    #[test]
    fn test_translation_korean() {
        let result = t("ko", "err-not-logged-in");
        assert_eq!(result, "로그인되지 않음");
    }

    #[test]
    fn test_translation_italian() {
        let result = t("it", "err-not-logged-in");
        assert_eq!(result, "Non connesso");
    }

    #[test]
    fn test_translation_dutch() {
        let result = t("nl", "err-not-logged-in");
        assert_eq!(result, "Niet ingelogd");
    }

    #[test]
    fn test_translation_portuguese_pt() {
        let result = t("pt-PT", "err-not-logged-in");
        assert_eq!(result, "Sessão não iniciada");
    }

    #[test]
    fn test_translation_portuguese_br() {
        let result = t("pt-BR", "err-not-logged-in");
        assert_eq!(result, "Não conectado");
    }

    #[test]
    fn test_translation_chinese_tw() {
        let result = t("zh-TW", "err-not-logged-in");
        assert_eq!(result, "未登入");
    }
}
