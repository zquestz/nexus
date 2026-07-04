//! Helper functions for network handlers

use nexus_common::names::fold_name;

use crate::types::UserInfo;

/// Helper function to sort user list alphabetically by nickname (case-insensitive)
///
/// The nickname is always the display name - for regular accounts it equals the username,
/// for shared accounts it's the session-specific nickname.
pub fn sort_user_list(users: &mut [UserInfo]) {
    users.sort_by_cached_key(|u| fold_name(&u.nickname));
}
