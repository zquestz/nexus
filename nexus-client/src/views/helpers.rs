//! Shared helper functions for view rendering

/// Convenience wrapper for `crate::i18n::t_args` to avoid verbose imports in view modules
pub fn t_args(key: &str, args: &[(&str, &str)]) -> String {
    crate::i18n::t_args(key, args)
}
