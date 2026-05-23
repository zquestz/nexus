//! Canonical case-insensitive folding for user-facing names — usernames,
//! display nicknames, channel names, group names, tracker names, and ban/trust
//! nickname annotations.
//!
//! Every case-insensitive treatment of a name — uniqueness, lookup, message
//! routing, sorting, dedup, and incidental folding alike — must use the *same*
//! rule, or two names that fold to one key get treated as distinct (or vice
//! versa). [`fold_name`] is that single rule: a bare `to_lowercase()` on a name
//! is a bug.
//!
//! Usernames and display nicknames additionally share *one* identity namespace
//! — a username must not collide with an active nickname (and vice versa), so
//! both fold identically. Channel, group, and tracker names are each their own
//! single namespace (case-insensitive uniqueness + lookup). Ban/trust nicknames
//! fold only for case-insensitive lookup/delete — they annotate a ban, they
//! aren't a unique identity. All route through [`fold_name`] so the whole name
//! surface shares one case rule.

/// Returns the canonical case-insensitive comparison key for a user-facing
/// name (username, nickname, channel / group / tracker name, or ban/trust
/// nickname).
///
/// Currently full Unicode lowercase ([`str::to_lowercase`]) — unlike SQLite's
/// `COLLATE NOCASE` / `lower()`, which fold ASCII only. If this ever needs to
/// grow (e.g. NFC normalization), it changes here and every caller folds
/// identically.
///
/// Not for non-name strings — IP addresses, file paths, search terms, etc.
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
