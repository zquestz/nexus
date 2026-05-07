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
    // `locales/en/ui.ftl`. The fallback path in `translate()` panics on
    // missing-in-English, so a stale or typo'd literal is a release bug
    // — we'd rather catch it at `cargo test` than in production.
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

    /// Pull every `t("…")` / `t_args("…", ...)` literal-string key out of a
    /// Rust source file's text. Only matches `t` / `t_args` as bare
    /// function calls (preceded by a non-identifier character) so we
    /// don't catch `format!(...)` or other unrelated names ending in
    /// `t`. Skips empty keys, keys containing `{` (`format!`-style
    /// interpolation), and escape sequences (`\n` etc., which can't be
    /// Fluent keys anyway). Line comments (`// ...`) are stripped
    /// before scanning so docstring examples don't trip the check.
    fn extract_keys_from_source(content: &str, found: &mut HashSet<String>) {
        // Scan line-by-line and strip line comments so doc-comment
        // examples like `// Use t("key") for ...` don't produce false
        // positives. This is a conservative approximation — block
        // comments and `//` inside string literals aren't handled —
        // but the false-positive surface is small in practice.
        for line in content.lines() {
            let scan = match line.find("//") {
                Some(idx) => &line[..idx],
                None => line,
            };
            for prefix in &["t(\"", "t_args(\""] {
                let mut pos = 0;
                while let Some(idx) = scan[pos..].find(prefix) {
                    let abs = pos + idx;
                    let before = if abs == 0 {
                        None
                    } else {
                        scan[..abs].chars().last()
                    };
                    // Bare-name guard: the char immediately before `t`
                    // (or `t_args`) must NOT be an identifier char.
                    // Otherwise we'd match `_t("…")` or the like.
                    let bare_call = before.is_none_or(|c| !c.is_alphanumeric() && c != '_');
                    if bare_call {
                        let start = abs + prefix.len();
                        if let Some(end) = scan[start..].find('"') {
                            let key = &scan[start..start + end];
                            if !key.is_empty() && !key.contains('{') && !key.contains('\\') {
                                found.insert(key.to_string());
                            }
                        }
                    }
                    pos = abs + prefix.len();
                }
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
    /// keys defined in it. Skips comments, blank lines, attribute
    /// continuations, and any line whose left-of-`=` doesn't look like a
    /// Fluent identifier.
    fn collect_keys_in_ftl(path: &Path) -> HashSet<String> {
        let mut keys: HashSet<String> = HashSet::new();
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return keys,
        };
        for line in content.lines() {
            if line.is_empty() {
                continue;
            }
            // Comments
            if let Some(first) = line.chars().next()
                && (first == '#' || first.is_whitespace())
            {
                continue;
            }
            // Find the `=`
            let Some(eq) = line.find('=') else {
                continue;
            };
            let key = line[..eq].trim();
            // Fluent identifier shape: starts with a letter, otherwise
            // letters / digits / `-` / `_`. Reject anything else.
            let mut chars = key.chars();
            let valid = match chars.next() {
                Some(c) if c.is_alphabetic() => {
                    chars.all(|c| c.is_alphanumeric() || c == '-' || c == '_')
                }
                _ => false,
            };
            if valid {
                keys.insert(key.to_string());
            }
        }
        keys
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
}
