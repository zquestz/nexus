//! Folder type detection based on naming suffix convention
//!
//! Folder types are determined by case-insensitive suffixes (space required before suffix):
//! - ` [NEXUS-UL]` - Upload folder (users can upload files)
//! - ` [NEXUS-DB]` - Drop box (blind upload, only admins see contents)
//! - ` [NEXUS-DB-username]` - User drop box (blind upload, user + admins see contents)
//! - No suffix - Default (read-only)

use crate::constants::{FOLDER_SUFFIX_DROPBOX, FOLDER_SUFFIX_DROPBOX_PREFIX, FOLDER_SUFFIX_UPLOAD};

/// Type of folder based on suffix convention
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FolderType {
    /// Default folder - read-only (no suffix)
    Default,

    /// Upload folder - users can upload files (`[NEXUS-UL]` suffix)
    /// Upload permission is inherited by subfolders
    Upload,

    /// Drop box - blind upload, only admins can see contents (`[NEXUS-DB]` suffix)
    DropBox,

    /// User drop box - blind upload, named user + admins can see contents
    /// (`[NEXUS-DB-username]` suffix)
    UserDropBox(String),
}

/// Parse folder type from a folder name
///
/// Suffix matching is case-insensitive. The suffix must be at the end of the name.
///
/// If a folder name contains multiple valid suffixes, only the one at the end
/// is considered. For example, `Folder [NEXUS-UL] [NEXUS-DB]` would be parsed
/// as a DropBox, not an Upload folder.
///
/// # Examples
///
/// ```ignore
/// assert_eq!(parse_folder_type("Documents"), FolderType::Default);
/// assert_eq!(parse_folder_type("Uploads [NEXUS-UL]"), FolderType::Upload);
/// assert_eq!(parse_folder_type("Uploads [nexus-ul]"), FolderType::Upload);
/// assert_eq!(parse_folder_type("Inbox [NEXUS-DB]"), FolderType::DropBox);
/// assert_eq!(parse_folder_type("For Alice [NEXUS-DB-alice]"), FolderType::UserDropBox("alice".to_string()));
/// ```
#[must_use]
pub fn parse_folder_type(name: &str) -> FolderType {
    let name_upper = name.to_uppercase();

    // Check for upload suffix first (exact match at end)
    // Note: FOLDER_SUFFIX_* constants are already uppercase ASCII, so direct comparison works
    // Require actual folder name before suffix (not just whitespace)
    if name_upper.ends_with(FOLDER_SUFFIX_UPLOAD) && name.len() > FOLDER_SUFFIX_UPLOAD.len() {
        let prefix_end = name.len() - FOLDER_SUFFIX_UPLOAD.len();
        if !name[..prefix_end].trim().is_empty() {
            return FolderType::Upload;
        }
    }

    // Check for user-specific drop box suffix: [NEXUS-DB-username]
    // This must end with ] and contain [NEXUS-DB- before the username
    if name_upper.ends_with(']') {
        // Find the last occurrence of the prefix
        if let Some(prefix_pos) = name_upper.rfind(FOLDER_SUFFIX_DROPBOX_PREFIX) {
            // The closing bracket should be at the end
            let bracket_pos = name.len() - 1;

            // Extract username: from after prefix to before closing bracket
            let username_start = prefix_pos + FOLDER_SUFFIX_DROPBOX_PREFIX.len();
            let username_end = bracket_pos;

            // Verify there's content between prefix and bracket, and no other brackets in between
            // Also require actual folder name before the suffix (not just whitespace)
            if username_start < username_end && prefix_pos > 0 {
                let username = &name[username_start..username_end];
                // Make sure the username doesn't contain brackets (which would indicate
                // this isn't actually a valid suffix)
                if !username.contains('[')
                    && !username.contains(']')
                    && !username.is_empty()
                    && !name[..prefix_pos].trim().is_empty()
                {
                    return FolderType::UserDropBox(username.to_string());
                }
            }
        }
    }

    // Check for generic drop box suffix (exact match at end)
    // Require actual folder name before suffix (not just whitespace)
    if name_upper.ends_with(FOLDER_SUFFIX_DROPBOX) && name.len() > FOLDER_SUFFIX_DROPBOX.len() {
        let prefix_end = name.len() - FOLDER_SUFFIX_DROPBOX.len();
        if !name[..prefix_end].trim().is_empty() {
            return FolderType::DropBox;
        }
    }

    FolderType::Default
}

/// Get the display name for a folder (strip the suffix)
///
/// Removes the folder type suffix if present, leaving just the descriptive name.
/// Trailing whitespace before the suffix is also removed.
///
/// # Examples
///
/// ```ignore
/// assert_eq!(display_name("Documents"), "Documents");
/// assert_eq!(display_name("Uploads [NEXUS-UL]"), "Uploads");
/// assert_eq!(display_name("For Alice [NEXUS-DB-alice]"), "For Alice");
/// ```
#[must_use]
pub fn display_name(name: &str) -> &str {
    let name_upper = name.to_uppercase();

    // Check for upload suffix
    // Note: FOLDER_SUFFIX_* constants are already uppercase ASCII, so direct comparison works
    // Only strip if there's actual non-whitespace content before the suffix
    if name_upper.ends_with(FOLDER_SUFFIX_UPLOAD) && name.len() > FOLDER_SUFFIX_UPLOAD.len() {
        let end = name.len() - FOLDER_SUFFIX_UPLOAD.len();
        let prefix = name[..end].trim_end();
        if !prefix.is_empty() {
            return prefix;
        }
    }

    // Check for user-specific drop box suffix: [NEXUS-DB-username]
    // Only strip if there's actual non-whitespace content before the suffix
    if name_upper.ends_with(']')
        && let Some(prefix_pos) = name_upper.rfind(FOLDER_SUFFIX_DROPBOX_PREFIX)
        && prefix_pos > 0
    {
        let username_start = prefix_pos + FOLDER_SUFFIX_DROPBOX_PREFIX.len();
        let username_end = name.len() - 1;

        if username_start < username_end {
            let username = &name[username_start..username_end];
            if !username.contains('[') && !username.contains(']') && !username.is_empty() {
                let prefix = name[..prefix_pos].trim_end();
                if !prefix.is_empty() {
                    return prefix;
                }
            }
        }
    }

    // Check for generic drop box suffix
    // Only strip if there's actual non-whitespace content before the suffix
    if name_upper.ends_with(FOLDER_SUFFIX_DROPBOX) && name.len() > FOLDER_SUFFIX_DROPBOX.len() {
        let end = name.len() - FOLDER_SUFFIX_DROPBOX.len();
        let prefix = name[..end].trim_end();
        if !prefix.is_empty() {
            return prefix;
        }
    }

    name
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==========================================================================
    // parse_folder_type tests
    // ==========================================================================

    #[test]
    fn test_default_folder() {
        assert_eq!(parse_folder_type("Documents"), FolderType::Default);
        assert_eq!(parse_folder_type("My Files"), FolderType::Default);
        assert_eq!(parse_folder_type(""), FolderType::Default);
    }

    #[test]
    fn test_upload_folder() {
        assert_eq!(parse_folder_type("Uploads [NEXUS-UL]"), FolderType::Upload);
    }

    #[test]
    fn test_suffix_only_is_default() {
        // Suffix-only names (no folder name before suffix) are treated as Default
        // Without leading space
        assert_eq!(parse_folder_type("[NEXUS-UL]"), FolderType::Default);
        assert_eq!(parse_folder_type("[NEXUS-DB]"), FolderType::Default);
        assert_eq!(parse_folder_type("[NEXUS-DB-alice]"), FolderType::Default);
        // With leading space (matches the constant, but no actual folder name)
        assert_eq!(parse_folder_type(" [NEXUS-UL]"), FolderType::Default);
        assert_eq!(parse_folder_type(" [NEXUS-DB]"), FolderType::Default);
        assert_eq!(parse_folder_type(" [NEXUS-DB-alice]"), FolderType::Default);
        // With multiple leading spaces (only whitespace before suffix)
        assert_eq!(parse_folder_type("   [NEXUS-UL]"), FolderType::Default);
        assert_eq!(parse_folder_type("   [NEXUS-DB]"), FolderType::Default);
        assert_eq!(parse_folder_type("  [NEXUS-DB-alice]"), FolderType::Default);
    }

    #[test]
    fn test_upload_case_insensitive() {
        assert_eq!(parse_folder_type("Uploads [nexus-ul]"), FolderType::Upload);
        assert_eq!(parse_folder_type("Uploads [Nexus-UL]"), FolderType::Upload);
        assert_eq!(parse_folder_type("Uploads [NEXUS-ul]"), FolderType::Upload);
    }

    #[test]
    fn test_dropbox_folder() {
        assert_eq!(parse_folder_type("Inbox [NEXUS-DB]"), FolderType::DropBox);
    }

    #[test]
    fn test_dropbox_case_insensitive() {
        assert_eq!(parse_folder_type("Inbox [nexus-db]"), FolderType::DropBox);
        assert_eq!(parse_folder_type("Inbox [Nexus-DB]"), FolderType::DropBox);
    }

    #[test]
    fn test_user_dropbox_folder() {
        assert_eq!(
            parse_folder_type("For Alice [NEXUS-DB-alice]"),
            FolderType::UserDropBox("alice".to_string())
        );
    }

    #[test]
    fn test_user_dropbox_case_insensitive_prefix() {
        // Prefix is case-insensitive, but username case is preserved
        assert_eq!(
            parse_folder_type("For Alice [nexus-db-Alice]"),
            FolderType::UserDropBox("Alice".to_string())
        );
        assert_eq!(
            parse_folder_type("For Bob [Nexus-DB-Bob]"),
            FolderType::UserDropBox("Bob".to_string())
        );
    }

    #[test]
    fn test_user_dropbox_preserves_username_case() {
        assert_eq!(
            parse_folder_type("Files [NEXUS-DB-AlIcE]"),
            FolderType::UserDropBox("AlIcE".to_string())
        );
    }

    #[test]
    fn test_empty_user_dropbox_is_default() {
        // [NEXUS-DB-] with no username should not match as UserDropBox
        assert_eq!(parse_folder_type("Files [NEXUS-DB-]"), FolderType::Default);
    }

    #[test]
    fn test_suffix_must_be_at_end() {
        // Suffix in the middle should not match
        assert_eq!(
            parse_folder_type("[NEXUS-UL] Documents"),
            FolderType::Default
        );
        assert_eq!(parse_folder_type("[NEXUS-DB] Inbox"), FolderType::Default);
    }

    #[test]
    fn test_no_space_before_suffix_is_default() {
        // Suffix without leading space should NOT match (space is required)
        assert_eq!(parse_folder_type("Uploads[NEXUS-UL]"), FolderType::Default);
        assert_eq!(parse_folder_type("Inbox[NEXUS-DB]"), FolderType::Default);
        assert_eq!(
            parse_folder_type("For Alice[NEXUS-DB-alice]"),
            FolderType::Default
        );
    }

    #[test]
    fn test_user_dropbox_with_extra_brackets_is_rejected() {
        // This was a bug: username should not contain brackets
        // "Folder [NEXUS-DB-alice] extra]" should NOT match as UserDropBox
        assert_eq!(
            parse_folder_type("Folder [NEXUS-DB-alice] extra]"),
            FolderType::Default
        );
        assert_eq!(
            parse_folder_type("Folder [NEXUS-DB-al[ice]"),
            FolderType::Default
        );
    }

    #[test]
    fn test_multiple_suffixes_last_wins() {
        // If multiple valid suffixes, the one at the end determines type
        assert_eq!(
            parse_folder_type("Folder [NEXUS-UL] [NEXUS-DB]"),
            FolderType::DropBox
        );
        assert_eq!(
            parse_folder_type("Folder [NEXUS-DB] [NEXUS-UL]"),
            FolderType::Upload
        );
    }

    // ==========================================================================
    // display_name tests
    // ==========================================================================

    #[test]
    fn test_display_name_default() {
        assert_eq!(display_name("Documents"), "Documents");
        assert_eq!(display_name("My Files"), "My Files");
    }

    #[test]
    fn test_display_name_upload() {
        assert_eq!(display_name("Uploads [NEXUS-UL]"), "Uploads");
        // No space before suffix - returns full name (treated as Default folder)
        assert_eq!(display_name("Uploads[NEXUS-UL]"), "Uploads[NEXUS-UL]");
        // Suffix-only returns full name (treated as Default folder)
        assert_eq!(display_name("[NEXUS-UL]"), "[NEXUS-UL]");
        assert_eq!(display_name(" [NEXUS-UL]"), " [NEXUS-UL]");
        // Whitespace-only before suffix returns full name
        assert_eq!(display_name("   [NEXUS-UL]"), "   [NEXUS-UL]");
    }

    #[test]
    fn test_display_name_dropbox() {
        assert_eq!(display_name("Inbox [NEXUS-DB]"), "Inbox");
        // No space before suffix - returns full name (treated as Default folder)
        assert_eq!(display_name("Inbox[NEXUS-DB]"), "Inbox[NEXUS-DB]");
        // Suffix-only returns full name (treated as Default folder)
        assert_eq!(display_name("[NEXUS-DB]"), "[NEXUS-DB]");
        assert_eq!(display_name(" [NEXUS-DB]"), " [NEXUS-DB]");
        // Whitespace-only before suffix returns full name
        assert_eq!(display_name("   [NEXUS-DB]"), "   [NEXUS-DB]");
    }

    #[test]
    fn test_display_name_user_dropbox() {
        assert_eq!(display_name("For Alice [NEXUS-DB-alice]"), "For Alice");
        // No space before suffix - returns full name (treated as Default folder)
        assert_eq!(
            display_name("For Alice[NEXUS-DB-alice]"),
            "For Alice[NEXUS-DB-alice]"
        );
        // Suffix-only returns full name (treated as Default folder)
        assert_eq!(display_name("[NEXUS-DB-bob]"), "[NEXUS-DB-bob]");
        assert_eq!(display_name(" [NEXUS-DB-bob]"), " [NEXUS-DB-bob]");
        // Whitespace-only before suffix returns full name
        assert_eq!(display_name("  [NEXUS-DB-bob]"), "  [NEXUS-DB-bob]");
    }

    #[test]
    fn test_display_name_case_insensitive() {
        assert_eq!(display_name("Uploads [nexus-ul]"), "Uploads");
        assert_eq!(display_name("Inbox [nexus-db]"), "Inbox");
        assert_eq!(display_name("For Alice [nexus-db-alice]"), "For Alice");
    }

    #[test]
    fn test_display_name_trims_trailing_space() {
        assert_eq!(display_name("Uploads   [NEXUS-UL]"), "Uploads");
        assert_eq!(display_name("Inbox  [NEXUS-DB]"), "Inbox");
    }

    #[test]
    fn test_display_name_malformed_user_dropbox() {
        // Malformed suffix should return original name
        assert_eq!(
            display_name("Folder [NEXUS-DB-alice] extra]"),
            "Folder [NEXUS-DB-alice] extra]"
        );
    }
}
