//! Global locale state and normalization

use once_cell::sync::Lazy;

use super::constants::{DEFAULT_LOCALE, LOCALE_CHINESE, LOCALE_CHINESE_CN, LOCALE_CHINESE_TW};

/// Global locale setting, detected at startup and used for all translations
static CURRENT_LOCALE: Lazy<String> = Lazy::new(detect_system_locale);

/// Detect the system locale and normalize it for our supported locales
///
/// Handles locales like "en-US" -> "en" but preserves regional variants
/// for languages with significant differences (zh-CN/zh-TW, pt-BR/pt-PT).
fn detect_system_locale() -> String {
    sys_locale::get_locale()
        .map(|loc| normalize_locale(&loc))
        .unwrap_or_else(|| DEFAULT_LOCALE.to_string())
}

/// Normalize a locale string to our supported format
///
/// - Preserves regional variants for Chinese and Portuguese
/// - Maps Chinese script subtags to supported Simplified/Traditional bundles
/// - Strips region for other languages (e.g., "en-US" -> "en")
pub(super) fn normalize_locale(locale: &str) -> String {
    let parts: Vec<&str> = locale.split('-').collect();
    if parts.len() >= 2 {
        let lang = parts[0].to_lowercase();
        if lang == LOCALE_CHINESE {
            return normalize_chinese_locale(&parts[1..]);
        }

        let region = parts[1].to_uppercase();
        // Keep region for languages with significant regional variants
        match lang.as_str() {
            "pt" => format!("{}-{}", lang, region),
            _ => lang,
        }
    } else {
        match locale.to_lowercase().as_str() {
            LOCALE_CHINESE => LOCALE_CHINESE_CN.to_string(),
            other => other.to_string(),
        }
    }
}

fn normalize_chinese_locale(subtags: &[&str]) -> String {
    for subtag in subtags {
        match subtag.to_ascii_lowercase().as_str() {
            "hans" => return LOCALE_CHINESE_CN.to_string(),
            "hant" => return LOCALE_CHINESE_TW.to_string(),
            _ => {}
        }
    }

    for subtag in subtags {
        match subtag.to_ascii_uppercase().as_str() {
            "TW" | "HK" | "MO" => return LOCALE_CHINESE_TW.to_string(),
            _ => {}
        }
    }

    LOCALE_CHINESE_CN.to_string()
}

/// Get the current locale
pub fn get_locale() -> &'static str {
    &CURRENT_LOCALE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_locale_english() {
        assert_eq!(normalize_locale("en-US"), "en");
        assert_eq!(normalize_locale("en-GB"), "en");
        assert_eq!(normalize_locale("en"), "en");
    }

    #[test]
    fn test_normalize_locale_chinese() {
        assert_eq!(normalize_locale("zh-CN"), "zh-CN");
        assert_eq!(normalize_locale("zh-TW"), "zh-TW");
        assert_eq!(normalize_locale("zh"), "zh-CN");
        assert_eq!(normalize_locale("zh-Hans"), "zh-CN");
        assert_eq!(normalize_locale("zh-Hans-CN"), "zh-CN");
        assert_eq!(normalize_locale("zh-Hant"), "zh-TW");
        assert_eq!(normalize_locale("zh-Hant-TW"), "zh-TW");
        assert_eq!(normalize_locale("zh-Hant-HK"), "zh-TW");
        assert_eq!(normalize_locale("zh-HK"), "zh-TW");
        assert_eq!(normalize_locale("zh-MO"), "zh-TW");
        assert_eq!(normalize_locale("zh-SG"), "zh-CN");
    }

    #[test]
    fn test_normalize_locale_strips_non_chinese_scripts() {
        assert_eq!(normalize_locale("sr-Latn-RS"), "sr");
    }

    #[test]
    fn test_normalize_locale_portuguese() {
        assert_eq!(normalize_locale("pt-BR"), "pt-BR");
        assert_eq!(normalize_locale("pt-PT"), "pt-PT");
    }
}
