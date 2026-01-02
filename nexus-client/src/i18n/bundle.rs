//! Fluent bundle management

use fluent_bundle::{FluentBundle, FluentResource};
use unic_langid::LanguageIdentifier;

use super::constants::*;

/// Get a Fluent bundle for the specified locale
///
/// Loads the appropriate .ftl file and creates a bundle.
/// Falls back to English for unsupported locales.
///
/// Note: Currently creates a new bundle on each call. FluentBundle contains
/// non-Send types (RefCell, TypeMap) which prevent safe caching across threads.
/// For a GUI client, this performance trade-off is acceptable.
pub(super) fn get_bundle(locale: &str) -> FluentBundle<FluentResource> {
    let lang: LanguageIdentifier = locale
        .parse()
        .unwrap_or_else(|_| DEFAULT_LOCALE.parse().expect("'en' is a valid locale"));

    let mut bundle = FluentBundle::new(vec![lang]);

    // Normalize locale to actual file location
    // Generic locales (pt, zh) map to their default variants
    let normalized_locale = match locale {
        LOCALE_CHINESE => LOCALE_CHINESE_CN,       // "zh" -> "zh-CN"
        LOCALE_PORTUGUESE => LOCALE_PORTUGUESE_BR, // "pt" -> "pt-BR"
        other => other,
    };

    // Load ui.ftl for this locale (fallback to English)
    let ftl_string = match normalized_locale {
        LOCALE_CHINESE_CN => include_str!("../../locales/zh-CN/ui.ftl"),
        LOCALE_CHINESE_TW => include_str!("../../locales/zh-TW/ui.ftl"),
        LOCALE_DUTCH => include_str!("../../locales/nl/ui.ftl"),
        LOCALE_FRENCH => include_str!("../../locales/fr/ui.ftl"),
        LOCALE_GERMAN => include_str!("../../locales/de/ui.ftl"),
        LOCALE_ITALIAN => include_str!("../../locales/it/ui.ftl"),
        LOCALE_JAPANESE => include_str!("../../locales/ja/ui.ftl"),
        LOCALE_KOREAN => include_str!("../../locales/ko/ui.ftl"),
        LOCALE_PORTUGUESE_BR => include_str!("../../locales/pt-BR/ui.ftl"),
        LOCALE_PORTUGUESE_PT => include_str!("../../locales/pt-PT/ui.ftl"),
        LOCALE_RUSSIAN => include_str!("../../locales/ru/ui.ftl"),
        LOCALE_SPANISH => include_str!("../../locales/es/ui.ftl"),
        _ => include_str!("../../locales/en/ui.ftl"),
    };

    let resource = FluentResource::try_new(ftl_string.to_string()).expect(ERR_PARSE_FTL);

    bundle.add_resource(resource).expect(ERR_ADD_RESOURCE);

    bundle
}
