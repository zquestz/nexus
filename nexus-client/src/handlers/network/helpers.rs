//! Helper functions for network handlers

use crate::types::UserInfo;

/// Helper function to sort user list alphabetically by nickname (case-insensitive)
///
/// The nickname is always the display name - for regular accounts it equals the username,
/// for shared accounts it's the session-specific nickname.
pub fn sort_user_list(users: &mut [UserInfo]) {
    users.sort_by(|a, b| a.nickname.to_lowercase().cmp(&b.nickname.to_lowercase()));
}

/// Format session duration in human-readable form
pub fn format_duration(seconds: u64) -> String {
    if seconds < 60 {
        format!("{}s", seconds)
    } else if seconds < 3600 {
        format!("{}m", seconds / 60)
    } else {
        format!("{}h {}m", seconds / 3600, (seconds % 3600) / 60)
    }
}
