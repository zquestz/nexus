//! Canonical case-insensitive folding for user-facing names (usernames and
//! nicknames).
//!
//! Usernames and nicknames share one identity namespace, so every
//! case-insensitive treatment of them — uniqueness, lookup, message routing,
//! and incidental folding alike — must use the *same* rule, or two names that
//! fold to the same key get treated as distinct (or vice versa). [`fold_name`]
//! is that single rule: route all username/nickname lowercasing through it. A
//! bare `to_lowercase()` on a name is a bug.

/// Returns the canonical case-insensitive comparison key for a user-facing
/// name (username or nickname).
///
/// Currently full Unicode lowercase ([`str::to_lowercase`]) — unlike SQLite's
/// `COLLATE NOCASE` / `lower()`, which fold ASCII only. If this ever needs to
/// grow (e.g. NFC normalization), it changes here and every caller folds
/// identically.
///
/// Not for non-name strings — IP addresses, file paths, channel names, etc.
/// keep their own folding.
pub fn fold_name(name: &str) -> String {
    name.to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_ascii_case() {
        assert_eq!(fold_name("Admin"), "admin");
        assert_eq!(fold_name("ADMIN"), "admin");
    }

    #[test]
    fn folds_unicode_case() {
        // The whole point: non-ASCII case folds too, which SQLite NOCASE / ASCII
        // `lower()` would not do.
        assert_eq!(fold_name("CAFÉ"), "café");
        assert_eq!(fold_name("Café"), "café");
    }

    #[test]
    fn fold_is_idempotent() {
        let once = fold_name("CAFÉ");
        assert_eq!(fold_name(&once), once);
    }

    #[test]
    fn case_variants_share_a_key_distinct_letters_do_not() {
        assert_eq!(fold_name("CAFÉ"), fold_name("café"));
        // `e` and `é` are different letters, not case variants — distinct keys.
        assert_ne!(fold_name("cafe"), fold_name("café"));
    }
}
