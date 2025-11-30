//! Permission name translation

use super::bundle::get_bundle;
use super::locale::get_locale;

/// Translate a permission name to a user-friendly display string
///
/// Looks up the translation key "permission-{permission_name}" and returns
/// the localized string. Falls back to replacing underscores with spaces
/// if no translation is found.
///
/// # Arguments
/// * `permission` - The permission name (e.g., "user_list", "chat_send")
///
/// # Returns
/// The translated permission name, or a fallback with underscores replaced by spaces
///
/// # Example
/// ```ignore
/// let display = translate_permission("user_list"); // "User List" in English
/// ```
pub fn translate_permission(permission: &str) -> String {
    translate_permission_for_locale(permission, get_locale())
}

fn translate_permission_for_locale(permission: &str, locale: &str) -> String {
    let key = format!("permission-{}", permission);

    let bundle = get_bundle(locale);
    if let Some(msg) = bundle.get_message(&key).and_then(|m| m.value()) {
        let mut errors = vec![];
        let value = bundle.format_pattern(msg, None, &mut errors);
        return value.to_string();
    }

    // Fallback: replace underscores with spaces
    permission.replace('_', " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_translate_permission() {
        let result = translate_permission_for_locale("user_list", "en");
        assert!(!result.is_empty());
    }

    #[test]
    fn test_translate_permission_fallback() {
        let result = translate_permission_for_locale("nonexistent_permission", "en");
        assert_eq!(result, "nonexistent permission");
    }
}
