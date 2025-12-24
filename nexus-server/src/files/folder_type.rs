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
}
