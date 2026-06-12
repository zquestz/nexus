//! Fluent bundle management

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use fluent_bundle::{FluentBundle, FluentResource};
use unic_langid::LanguageIdentifier;

use super::constants::*;
use crate::constants::ERR_DEFAULT_LOCALE_INVALID;

thread_local! {
    static BUNDLE_CACHE: RefCell<HashMap<String, Rc<FluentBundle<FluentResource>>>> =
        RefCell::new(HashMap::new());
}

/// Get a Fluent bundle for the specified locale
///
/// Loads the appropriate .ftl file and creates a bundle.
/// Falls back to English for unsupported locales.
///
/// Bundles are cached per thread because `FluentBundle` contains non-`Send`
/// internals. The GUI hot path stays on one thread, so this avoids reparsing
/// the locale file on every `t()` call without sharing bundles across threads.
pub(super) fn get_bundle(locale: &str) -> Rc<FluentBundle<FluentResource>> {
    BUNDLE_CACHE.with(|cache| {
        if let Some(bundle) = cache.borrow().get(locale).cloned() {
            return bundle;
        }

        let bundle = Rc::new(build_bundle(locale));
        cache
            .borrow_mut()
            .insert(locale.to_string(), Rc::clone(&bundle));
        bundle
    })
}

fn build_bundle(locale: &str) -> FluentBundle<FluentResource> {
    let lang: LanguageIdentifier = locale
        .parse()
        .unwrap_or_else(|_| DEFAULT_LOCALE.parse().expect(ERR_DEFAULT_LOCALE_INVALID));

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

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::*;

    #[test]
    fn test_get_bundle_caches_same_locale() {
        let first = get_bundle("en");
        let second = get_bundle("en");

        assert!(Rc::ptr_eq(&first, &second));
    }

    #[test]
    fn test_get_bundle_caches_locales_separately() {
        let english = get_bundle("en");
        let french = get_bundle("fr");

        assert!(!Rc::ptr_eq(&english, &french));
    }
}
