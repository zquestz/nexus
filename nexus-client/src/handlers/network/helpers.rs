//! Helper functions for network handlers

use crate::types::UserInfo;

/// Helper function to sort user list alphabetically by username (case-insensitive)
pub fn sort_user_list(users: &mut [UserInfo]) {
    users.sort_by(|a, b| a.username.to_lowercase().cmp(&b.username.to_lowercase()));
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