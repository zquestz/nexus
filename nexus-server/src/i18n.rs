//! Internationalization support using Fluent

use fluent_bundle::{FluentArgs, FluentBundle, FluentResource, FluentValue};
use tracing::warn;
use unic_langid::LanguageIdentifier;

use crate::constants::*;

/// Translate `key` for `locale`, falling back to English if the locale is
/// unsupported or the key is missing there.
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

    // Fallback to English if key missing in requested locale
    warn!(key = %key, locale = %locale, "{}", LOG_MISSING_TRANSLATION_KEY);
    if locale != DEFAULT_LOCALE {
        return t(DEFAULT_LOCALE, key);
    }

    panic!("{} '{}'", ERR_I18N_MISSING_KEY_ENGLISH, key);
}

/// Translate `key` for `locale` with `(name, value)` parameter substitution,
/// falling back to English if the locale is unsupported or the key is missing.
pub fn t_args(locale: &str, key: &str, args: &[(&str, &str)]) -> String {
    let bundle = get_bundle(locale);

    if let Some(msg) = bundle.get_message(key).and_then(|m| m.value()) {
        let mut fluent_args = FluentArgs::new();
        for (k, v) in args {
            // Try parsing as integer first so Fluent numeric selectors match correctly
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

    // Fallback to English if key missing in requested locale
    warn!(key = %key, locale = %locale, "{}", LOG_MISSING_TRANSLATION_KEY);
    if locale != DEFAULT_LOCALE {
        return t_args(DEFAULT_LOCALE, key, args);
    }

    panic!("{} '{}'", ERR_I18N_MISSING_KEY_ENGLISH, key);
}

/// Build a Fluent bundle for `locale` (English for unsupported locales).
///
/// Builds a fresh bundle per call: `FluentBundle` holds non-Send types
/// (RefCell, TypeMap) that can't be cached across threads, and errors are
/// infrequent enough on a BBS server that the cost is acceptable.
fn get_bundle(locale: &str) -> FluentBundle<FluentResource> {
    let canonical_locale = canonical_locale(locale);
    let lang: LanguageIdentifier = canonical_locale
        .parse()
        .unwrap_or_else(|_| DEFAULT_LOCALE.parse().expect(ERR_DEFAULT_LOCALE_INVALID));

    let mut bundle = FluentBundle::new(vec![lang]);

    // Generic locales (pt, zh) map to their default regional variants.
    let normalized_locale = match canonical_locale.as_str() {
        LOCALE_PORTUGUESE => LOCALE_PORTUGUESE_BR, // "pt" -> "pt-BR"
        LOCALE_CHINESE => LOCALE_CHINESE_CN,       // "zh" -> "zh-CN"
        other => other,
    };

    // errors.ftl for this locale; the wildcard arm falls back to English.
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

fn canonical_locale(locale: &str) -> String {
    let parts: Vec<&str> = locale.split('-').collect();
    let lang = parts.first().copied().unwrap_or_default().to_lowercase();
    if lang != LOCALE_CHINESE {
        return locale.to_string();
    }

    for subtag in &parts[1..] {
        match subtag.to_ascii_lowercase().as_str() {
            "hans" => return LOCALE_CHINESE_CN.to_string(),
            "hant" => return LOCALE_CHINESE_TW.to_string(),
            _ => {}
        }
    }

    for subtag in &parts[1..] {
        match subtag.to_ascii_uppercase().as_str() {
            "TW" | "HK" | "MO" => return LOCALE_CHINESE_TW.to_string(),
            _ => {}
        }
    }

    LOCALE_CHINESE_CN.to_string()
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

    /// Russian's three plural forms ("секунду / секунды / секунд") are visibly
    /// distinct, proving `t_args` passes the count as a number — a string would
    /// fall through to `*[other]` for every input.
    #[test]
    fn test_russian_plural_three_forms_via_flood_warning() {
        let one = t_args(
            "ru",
            "err-flood-warning",
            &[
                ("violation", "1"),
                ("max_violations", "3"),
                ("seconds", "1"),
            ],
        );
        assert!(one.contains("секунду"), "expected 'секунду' for one: {one}");

        let few = t_args(
            "ru",
            "err-flood-warning",
            &[
                ("violation", "1"),
                ("max_violations", "3"),
                ("seconds", "3"),
            ],
        );
        assert!(few.contains("секунды"), "expected 'секунды' for few: {few}");

        let other = t_args(
            "ru",
            "err-flood-warning",
            &[
                ("violation", "1"),
                ("max_violations", "3"),
                ("seconds", "5"),
            ],
        );
        assert!(
            other.contains("секунд") && !other.contains("секунду") && !other.contains("секунды"),
            "expected bare 'секунд' for other: {other}"
        );
    }

    /// Russian numeric-noun forms after a numeral: `[one]` (n≡1 mod 10, ≢11
    /// mod 100) → "байт"; `[few]` (n≡2..4 mod 10, ≢12..14 mod 100) → "байта";
    /// `*[other]` → "байт". `min = 1024` lands in `[few]`, the real configured
    /// minimum operators see when trying smaller chunks.
    #[test]
    fn test_russian_plural_bandwidth_chunk_size() {
        let one = t_args("ru", "err-bandwidth-chunk-size-too-small", &[("min", "1")]);
        assert!(
            one.contains("байт") && !one.contains("байта"),
            "expected bare 'байт' for one: {one}"
        );

        let few = t_args(
            "ru",
            "err-bandwidth-chunk-size-too-small",
            &[("min", "1024")],
        );
        assert!(few.contains("байта"), "expected 'байта' for few: {few}");

        // 100 lands in [other] (100 % 10 = 0). 8192 would NOT — it's [few]
        // (8192 % 10 = 2).
        let other = t_args(
            "ru",
            "err-bandwidth-chunk-size-too-small",
            &[("min", "100")],
        );
        assert!(
            other.contains("байт") && !other.contains("байта"),
            "expected bare 'байт' for other: {other}"
        );
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
    fn test_translation_chinese_script_tags() {
        assert_eq!(t("zh-Hans-CN", "err-not-logged-in"), "未登录");
        assert_eq!(t("zh-Hant-TW", "err-not-logged-in"), "未登入");
        assert_eq!(t("zh-Hant-HK", "err-not-logged-in"), "未登入");
        assert_eq!(t("zh-HK", "err-not-logged-in"), "未登入");
        assert_eq!(t("zh-MO", "err-not-logged-in"), "未登入");
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

    /// All non-EN locale codes the server ships, mirroring the directories
    /// under `nexus-server/locales/`.
    const NON_EN_LOCALES: &[&str] = &[
        "es", "fr", "de", "it", "nl", "pt-BR", "pt-PT", "ru", "ja", "zh-CN", "zh-TW", "ko",
    ];

    /// Top-level message keys in a `.ftl` file, via the real
    /// `fluent_syntax::parser` so every Fluent feature is handled correctly.
    fn collect_keys_in_ftl(path: &std::path::Path) -> std::collections::HashSet<String> {
        use fluent_syntax::ast::Entry;
        // Panic with path + OS error: a missing locale file is a real bug, and
        // this is more diagnostic than an "every key is missing" diff.
        let content = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("failed to read {}: {}", path.display(), e));
        // `parse` returns `Err((Resource, Vec<Error>))` on partial parse — keep
        // what parsed by accepting either arm.
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

    fn locale_errors_path(locale: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("locales")
            .join(locale)
            .join("errors.ftl")
    }

    fn strip_line_comments(content: &str) -> String {
        let mut stripped = String::with_capacity(content.len());
        for line in content.lines() {
            let scan = match line.find("//") {
                Some(idx) => &line[..idx],
                None => line,
            };
            stripped.push_str(scan);
            stripped.push('\n');
        }
        stripped
    }

    fn is_ident_char(c: char) -> bool {
        c.is_alphanumeric() || c == '_'
    }

    fn skip_ws(scan: &str, mut pos: usize) -> usize {
        while pos < scan.len() && scan.as_bytes()[pos].is_ascii_whitespace() {
            pos += 1;
        }
        pos
    }

    fn insert_key_if_valid(key: &str, found: &mut std::collections::HashSet<String>) {
        if !key.is_empty() && !key.contains('{') && !key.contains('\\') {
            found.insert(key.to_string());
        }
    }

    fn extract_translation_keys_from_source(
        content: &str,
        found: &mut std::collections::HashSet<String>,
    ) {
        let scan = strip_line_comments(content);
        for name in &["t_args", "t"] {
            let mut pos = 0;
            while let Some(idx) = scan[pos..].find(name) {
                let abs = pos + idx;
                let before = if abs == 0 {
                    None
                } else {
                    scan[..abs].chars().last()
                };
                let bare_call = before.is_none_or(|c| !is_ident_char(c));
                if bare_call {
                    let open_paren = skip_ws(&scan, abs + name.len());
                    if scan.as_bytes().get(open_paren) == Some(&b'(')
                        && let Some(comma_rel) = scan[open_paren + 1..].find(',')
                    {
                        let key_pos = skip_ws(&scan, open_paren + 1 + comma_rel + 1);
                        if scan.as_bytes().get(key_pos) == Some(&b'"') {
                            let start = key_pos + 1;
                            if let Some(end) = scan[start..].find('"') {
                                insert_key_if_valid(&scan[start..start + end], found);
                            }
                        }
                    }
                }
                pos = abs + name.len();
            }
        }
    }

    fn collect_translation_keys_from_sources(
        dir: &std::path::Path,
        found: &mut std::collections::HashSet<String>,
    ) {
        let entries = match std::fs::read_dir(dir) {
            Ok(it) => it,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_translation_keys_from_sources(&path, found);
            } else if path.extension().and_then(|s| s.to_str()) == Some("rs")
                && let Ok(content) = std::fs::read_to_string(&path)
            {
                let scan = match content.find("#[cfg(test)]") {
                    Some(idx) => &content[..idx],
                    None => content.as_str(),
                };
                extract_translation_keys_from_source(scan, found);
            }
        }
    }

    fn extract_likely_i18n_literals(
        content: &str,
        en_keys: &std::collections::HashSet<String>,
        found: &mut std::collections::HashSet<String>,
    ) {
        let prefixes: std::collections::HashSet<&str> = en_keys
            .iter()
            .filter_map(|key| key.split_once('-').map(|(prefix, _)| prefix))
            .collect();

        let scan = strip_line_comments(content);
        let mut pos = 0;
        while let Some(idx) = scan[pos..].find('"') {
            let start_quote = pos + idx;
            let start = start_quote + 1;
            let Some(end_rel) = scan[start..].find('"') else {
                break;
            };
            let end = start + end_rel;
            let value = &scan[start..end];
            let first_prefix = value.split_once('-').map(|(prefix, _)| prefix);
            if value.contains('-')
                && value
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
                && first_prefix.is_some_and(|prefix| prefixes.contains(prefix))
            {
                found.insert(value.to_string());
            }
            pos = end + 1;
        }
    }

    fn collect_likely_i18n_literals(
        dir: &std::path::Path,
        en_keys: &std::collections::HashSet<String>,
        found: &mut std::collections::HashSet<String>,
    ) {
        let entries = match std::fs::read_dir(dir) {
            Ok(it) => it,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_likely_i18n_literals(&path, en_keys, found);
            } else if path.extension().and_then(|s| s.to_str()) == Some("rs")
                && let Ok(content) = std::fs::read_to_string(&path)
            {
                let scan = match content.find("#[cfg(test)]") {
                    Some(idx) => &content[..idx],
                    None => content.as_str(),
                };
                extract_likely_i18n_literals(scan, en_keys, found);
            }
        }
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

    #[test]
    fn every_t_literal_has_en_key() {
        let src_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let en_keys = collect_keys_in_ftl(&locale_errors_path("en"));
        assert!(!en_keys.is_empty());

        let mut found = std::collections::HashSet::new();
        collect_translation_keys_from_sources(&src_dir, &mut found);
        assert!(
            !found.is_empty(),
            "scanner found zero t()/t_args() literals — paths or pattern likely broken"
        );

        let mut missing: Vec<String> = found.difference(&en_keys).cloned().collect();
        missing.sort();
        assert!(
            missing.is_empty(),
            "Server t()/t_args() literal(s) reference keys missing from en/errors.ftl. \
             Missing: {:#?}",
            missing,
        );
    }

    #[test]
    fn every_likely_i18n_key_literal_has_en_key() {
        let src_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let en_keys = collect_keys_in_ftl(&locale_errors_path("en"));
        assert!(!en_keys.is_empty());

        let mut found = std::collections::HashSet::new();
        collect_likely_i18n_literals(&src_dir, &en_keys, &mut found);

        let mut missing: Vec<String> = found.difference(&en_keys).cloned().collect();
        missing.sort();
        assert!(
            missing.is_empty(),
            "Likely server i18n key literal(s) are missing from en/errors.ftl. \
             Missing: {:#?}",
            missing,
        );
    }
}
