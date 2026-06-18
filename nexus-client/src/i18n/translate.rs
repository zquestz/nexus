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
            // Diagnostic visibility is dev-only — stderr is invisible
            // in shipped GUI builds. The build-time t() coverage check
            // catches missing keys at compile time; this only fires
            // for live Fluent interpolation errors during development.
            #[cfg(debug_assertions)]
            eprintln!("{} '{}': {:?}", ERR_TRANSLATION_ERRORS, key, errors);
            #[cfg(not(debug_assertions))]
            let _ = errors;
        }

        return value.to_string();
    }

    // Fallback to English if key missing in requested locale.
    // Diagnostic is dev-only (stderr invisible in shipped builds).
    #[cfg(debug_assertions)]
    eprintln!(
        "{} '{}' {} {}",
        ERR_MISSING_KEY, key, MSG_FOR_LOCALE, locale
    );
    if locale != DEFAULT_LOCALE {
        return translate(DEFAULT_LOCALE, key);
    }

    // Missing even in English: degrade to the raw key rather than
    // crashing the GUI. Build-time coverage tests catch literal keys;
    // this guards the dynamic-key paths they can't see.
    #[cfg(debug_assertions)]
    eprintln!("{} '{}'", ERR_MISSING_KEY_ENGLISH, key);
    key.to_string()
}

/// Get a translated message with arguments for a specific locale
///
/// Falls back to English if the key is missing in the requested locale.
fn translate_with_args(locale: &str, key: &str, args: &[(&str, &str)]) -> String {
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
            // Diagnostic visibility is dev-only — stderr is invisible
            // in shipped GUI builds. The build-time t() coverage check
            // catches missing keys at compile time; this only fires
            // for live Fluent interpolation errors during development.
            #[cfg(debug_assertions)]
            eprintln!("{} '{}': {:?}", ERR_TRANSLATION_ERRORS, key, errors);
            #[cfg(not(debug_assertions))]
            let _ = errors;
        }

        return value.to_string();
    }

    // Fallback to English if key missing in requested locale.
    // Diagnostic is dev-only.
    #[cfg(debug_assertions)]
    eprintln!(
        "{} '{}' {} {}",
        ERR_MISSING_KEY, key, MSG_FOR_LOCALE, locale
    );
    if locale != DEFAULT_LOCALE {
        return translate_with_args(DEFAULT_LOCALE, key, args);
    }

    // Missing even in English: degrade to the raw key rather than
    // crashing the GUI. Build-time coverage tests catch literal keys;
    // this guards the dynamic-key paths they can't see.
    #[cfg(debug_assertions)]
    eprintln!("{} '{}'", ERR_MISSING_KEY_ENGLISH, key);
    key.to_string()
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

    #[test]
    fn missing_key_degrades_to_raw_key_without_panic() {
        // A key absent from every locale (including English) must not
        // panic the GUI — it degrades to the raw key string.
        let result = translate("en", "this-key-does-not-exist-anywhere");
        assert_eq!(result, "this-key-does-not-exist-anywhere");
    }

    #[test]
    fn missing_key_with_args_degrades_to_raw_key_without_panic() {
        let result = translate_with_args("en", "this-key-does-not-exist-anywhere", &[("x", "1")]);
        assert_eq!(result, "this-key-does-not-exist-anywhere");
    }

    #[test]
    fn missing_key_via_non_english_fallback_degrades_without_panic() {
        // Non-English locale → English fallback → still missing → degrade.
        // Exercises the recursion arm, not just the terminal English case.
        let result = translate("fr", "this-key-does-not-exist-anywhere");
        assert_eq!(result, "this-key-does-not-exist-anywhere");
    }

    #[test]
    fn missing_key_with_args_via_non_english_fallback_degrades_without_panic() {
        let result = translate_with_args("fr", "this-key-does-not-exist-anywhere", &[("x", "1")]);
        assert_eq!(result, "this-key-does-not-exist-anywhere");
    }

    // Pinning the Fluent plural selector dispatch on the chars-counted
    // length keys. The selector was added because flat strings rendered
    // "1 characters" / "1 символов" — verify both singular forms fire
    // and the [few] form fires for Russian numbers 2-4.
    #[test]
    fn test_message_too_long_singular_english() {
        let result = translate_with_args(
            "en",
            "err-message-too-long",
            &[("length", "1"), ("max", "200")],
        );
        assert!(result.contains("character"), "got: {result}");
        assert!(!result.contains("characters"), "got: {result}");
    }

    #[test]
    fn test_message_too_long_plural_english() {
        let result = translate_with_args(
            "en",
            "err-message-too-long",
            &[("length", "5"), ("max", "200")],
        );
        assert!(result.contains("characters"), "got: {result}");
    }

    #[test]
    fn test_message_too_long_singular_russian() {
        let result = translate_with_args(
            "ru",
            "err-message-too-long",
            &[("length", "1"), ("max", "200")],
        );
        assert!(result.contains("символ"), "got: {result}");
        assert!(!result.contains("символа"), "got: {result}");
        assert!(!result.contains("символов"), "got: {result}");
    }

    #[test]
    fn test_message_too_long_few_russian() {
        let result = translate_with_args(
            "ru",
            "err-message-too-long",
            &[("length", "3"), ("max", "200")],
        );
        assert!(result.contains("символа"), "got: {result}");
        assert!(!result.contains("символов"), "got: {result}");
    }

    #[test]
    fn test_message_too_long_other_russian() {
        let result = translate_with_args(
            "ru",
            "err-message-too-long",
            &[("length", "5"), ("max", "200")],
        );
        assert!(result.contains("символов"), "got: {result}");
    }

    // =========================================================================
    // Build-time-ish coverage check: every `t("...")` / `t_args("...")`
    // literal in the client source tree must have a corresponding key in
    // `locales/en/ui.ftl`. `translate()` degrades a missing-in-English key
    // to the raw key string rather than panicking, so a stale or typo'd
    // literal would render untranslated in the UI — we'd rather catch it at
    // `cargo test` than ship it.
    //
    // Scope: literal `"..."` keys only. Dynamic keys (variable, format!,
    // concat) cannot be statically checked and are skipped.
    // =========================================================================

    use std::collections::HashSet;
    use std::fs;
    use std::path::Path;

    /// Recursively walk a directory and collect every `t("...")` /
    /// `t_args("...")` literal-string key into `found`.
    fn collect_t_literals(dir: &Path, found: &mut HashSet<String>) {
        let entries = match fs::read_dir(dir) {
            Ok(it) => it,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_t_literals(&path, found);
            } else if path.extension().and_then(|s| s.to_str()) == Some("rs")
                && let Ok(content) = fs::read_to_string(&path)
            {
                // Strip the `#[cfg(test)] mod tests { ... }` block
                // before scanning. Project convention is that tests
                // live at the end of each file, so a simple
                // truncate-at-marker is sufficient and avoids
                // false-positives from test fixture keys.
                let scan = match content.find("#[cfg(test)]") {
                    Some(idx) => &content[..idx],
                    None => content.as_str(),
                };
                extract_keys_from_source(scan, found);
            }
        }
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

    fn insert_key_if_valid(key: &str, found: &mut HashSet<String>) {
        if !key.is_empty() && !key.contains('{') && !key.contains('\\') {
            found.insert(key.to_string());
        }
    }

    /// Pull every `t("…")` / `t_args("…", ...)` literal-string key out of a
    /// Rust source file's text. Only matches `t` / `t_args` as bare
    /// function calls (preceded by a non-identifier character) so we
    /// don't catch `format!(...)` or other unrelated names ending in
    /// `t`. Allows whitespace/newlines between the function name, `(`,
    /// and string literal. Skips empty keys, keys containing `{`
    /// (`format!`-style interpolation), and escape sequences (`\n`
    /// etc., which can't be Fluent keys anyway). Line comments
    /// (`// ...`) are stripped before scanning so docstring examples
    /// don't trip the check.
    fn extract_keys_from_source(content: &str, found: &mut HashSet<String>) {
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
                // Bare-name guard: the char immediately before `t`
                // (or `t_args`) must NOT be an identifier char.
                // Otherwise we'd match `_t("…")` or the like.
                let bare_call = before.is_none_or(|c| !is_ident_char(c));
                if bare_call {
                    let open_paren = skip_ws(&scan, abs + name.len());
                    if scan.as_bytes().get(open_paren) == Some(&b'(') {
                        let quote = skip_ws(&scan, open_paren + 1);
                        if scan.as_bytes().get(quote) == Some(&b'"') {
                            let start = quote + 1;
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

    fn extract_likely_i18n_literals(
        content: &str,
        en_keys: &HashSet<String>,
        found: &mut HashSet<String>,
    ) {
        let prefixes: HashSet<&str> = en_keys
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
        dir: &Path,
        en_keys: &HashSet<String>,
        found: &mut HashSet<String>,
    ) {
        let entries = match fs::read_dir(dir) {
            Ok(it) => it,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_likely_i18n_literals(&path, en_keys, found);
            } else if path.extension().and_then(|s| s.to_str()) == Some("rs")
                && let Ok(content) = fs::read_to_string(&path)
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
    fn every_t_literal_has_en_key() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let src_dir = Path::new(manifest_dir).join("src");

        let mut found: HashSet<String> = HashSet::new();
        collect_t_literals(&src_dir, &mut found);

        // Sanity check — if zero keys were found, the scanner is broken.
        assert!(
            !found.is_empty(),
            "scanner found zero t() literals — paths or pattern likely broken"
        );

        let bundle = get_bundle("en");
        let mut missing: Vec<String> = found
            .iter()
            .filter(|key| bundle.get_message(key).is_none())
            .cloned()
            .collect();
        missing.sort();

        assert!(
            missing.is_empty(),
            "{} t() literal(s) reference keys missing from en/ui.ftl. \
             Add the keys (and translate to the other 12 locales) or \
             rename the call site. Missing: {:#?}",
            missing.len(),
            missing,
        );
    }

    #[test]
    fn extract_keys_handles_simple_t_call() {
        let mut found: HashSet<String> = HashSet::new();
        extract_keys_from_source(r#"shaped_text(t("hello-world"))"#, &mut found);
        assert!(found.contains("hello-world"));
    }

    #[test]
    fn extract_keys_handles_t_args_call() {
        let mut found: HashSet<String> = HashSet::new();
        extract_keys_from_source(r#"t_args("err-with-arg", &[("arg", "value")])"#, &mut found);
        assert!(found.contains("err-with-arg"));
    }

    #[test]
    fn extract_keys_handles_multiline_t_args_call() {
        let mut found: HashSet<String> = HashSet::new();
        extract_keys_from_source(
            r#"
            t_args(
                "err-with-arg",
                &[("arg", "value")],
            )
            "#,
            &mut found,
        );
        assert!(found.contains("err-with-arg"));
    }

    #[test]
    fn extract_keys_handles_namespaced_t_call() {
        let mut found: HashSet<String> = HashSet::new();
        extract_keys_from_source(r#"crate::i18n::t("button-cancel")"#, &mut found);
        assert!(found.contains("button-cancel"));
    }

    #[test]
    fn extract_keys_skips_identifier_suffix_match() {
        // `mt("foo")` would naively match `t("foo")` if we don't guard
        // against an alphanumeric char before the `t`.
        let mut found: HashSet<String> = HashSet::new();
        extract_keys_from_source(r#"some_mt("not-a-key")"#, &mut found);
        assert!(found.is_empty());
    }

    #[test]
    fn extract_keys_skips_dynamic_keys() {
        // `t(&format!("dyn-{}", x))` shouldn't produce a literal key.
        // Our scanner only matches `t("..." )` with a literal opening
        // quote immediately after the paren, so dynamic forms are
        // skipped naturally.
        let mut found: HashSet<String> = HashSet::new();
        extract_keys_from_source(r#"t(&format!("dyn-{}", x))"#, &mut found);
        assert!(found.is_empty());
    }

    // =========================================================================
    // Locale parity: every EN key must exist in all 12 other locales,
    // and no non-EN locale may carry an orphan key that EN doesn't.
    //
    // Catches the "added a key in EN but forgot to translate" mistake at
    // test time, and the "renamed an EN key but the other locales still
    // carry the old name" mistake too.
    // =========================================================================

    /// Read a Fluent `.ftl` file and return the set of top-level message
    /// keys defined in it. Uses the real `fluent_syntax::parser` so any
    /// Fluent feature (multi-line values, message attributes, term refs,
    /// etc.) is handled correctly.
    fn collect_keys_in_ftl(path: &Path) -> HashSet<String> {
        use fluent_syntax::ast::Entry;
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return HashSet::new(),
        };
        // `parse` returns `Err((Resource, Vec<Error>))` on partial parse —
        // we still want what parsed successfully, so accept either arm.
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

    /// All non-EN locale codes the client supports. Mirrors the file
    /// names under `nexus-client/locales/` (excluding `en`).
    const NON_EN_LOCALES: &[&str] = &[
        "es", "fr", "de", "it", "nl", "pt-BR", "pt-PT", "ru", "ja", "zh-CN", "zh-TW", "ko",
    ];

    fn locale_ftl_path(locale: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("locales")
            .join(locale)
            .join("ui.ftl")
    }

    #[test]
    fn every_en_key_exists_in_all_locales() {
        let en_keys = collect_keys_in_ftl(&locale_ftl_path("en"));
        assert!(
            !en_keys.is_empty(),
            "en/ui.ftl scanner found zero keys — path or parser likely broken"
        );

        let mut report: Vec<String> = Vec::new();
        for locale in NON_EN_LOCALES {
            let locale_keys = collect_keys_in_ftl(&locale_ftl_path(locale));
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
            "Locales missing keys present in en/ui.ftl. Translate the \
             missing keys (or remove them from EN if intentional):\n\n{}",
            report.join("\n\n"),
        );
    }

    #[test]
    fn no_orphan_keys_in_non_en_locales() {
        let en_keys = collect_keys_in_ftl(&locale_ftl_path("en"));
        assert!(!en_keys.is_empty());

        let mut report: Vec<String> = Vec::new();
        for locale in NON_EN_LOCALES {
            let locale_keys = collect_keys_in_ftl(&locale_ftl_path(locale));
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
            "Non-EN locales carry keys not present in en/ui.ftl. \
             These are leftovers from a rename or removal — drop them \
             from the locale file:\n\n{}",
            report.join("\n\n"),
        );
    }

    #[test]
    fn every_likely_i18n_key_literal_has_en_key() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let src_dir = Path::new(manifest_dir).join("src");
        let en_keys = collect_keys_in_ftl(&locale_ftl_path("en"));
        assert!(!en_keys.is_empty());

        let mut found: HashSet<String> = HashSet::new();
        collect_likely_i18n_literals(&src_dir, &en_keys, &mut found);

        let mut missing: Vec<String> = found.difference(&en_keys).cloned().collect();
        missing.sort();

        assert!(
            missing.is_empty(),
            "Likely i18n key literal(s) are missing from en/ui.ftl. \
             This catches indirect key references such as command metadata, \
             enum translation_key() methods, and helper tooltip keys. \
             Missing: {:#?}",
            missing,
        );
    }
}
