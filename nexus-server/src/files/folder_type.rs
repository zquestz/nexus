//! Folder type detection based on naming suffix convention
//!
//! Folder types are determined by case-insensitive suffixes (space required before suffix):
//! - ` [NEXUS-UL]` - Upload folder (users can upload files)
//! - ` [NEXUS-DB]` - Drop box (blind upload, only admins see contents)
//! - ` [NEXUS-DB-username]` - User drop box (blind upload, user + admins see contents)
//! - No suffix - Default (read-only)

use std::path::Path;

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
    ///
    /// The named user implicitly gains delete and rename permissions for
    /// files inside their own drop box (but not the folder itself), even
    /// without the global `file_delete` / `file_rename` permissions. Move
    /// and copy are deliberately excluded — they can target paths outside
    /// the drop box. See `in_owned_dropbox`.
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

    // FOLDER_SUFFIX_* constants are uppercase ASCII, so comparison against
    // name_upper works. Require an actual folder name before the suffix (not
    // just whitespace) so a bare `[NEXUS-UL]` isn't treated as writable.
    if name_upper.ends_with(FOLDER_SUFFIX_UPLOAD) && name.len() > FOLDER_SUFFIX_UPLOAD.len() {
        let prefix_end = name.len() - FOLDER_SUFFIX_UPLOAD.len();
        if !name[..prefix_end].trim().is_empty() {
            return FolderType::Upload;
        }
    }

    // User-specific drop box `[NEXUS-DB-username]`: must end with `]` and
    // contain `[NEXUS-DB-` before the username.
    if name_upper.ends_with(']')
        && let Some(prefix_pos) = name_upper.rfind(FOLDER_SUFFIX_DROPBOX_PREFIX)
    {
        let bracket_pos = name.len() - 1;

        let username_start = prefix_pos + FOLDER_SUFFIX_DROPBOX_PREFIX.len();
        let username_end = bracket_pos;

        // Require username content between prefix and bracket, and an
        // actual folder name (not just whitespace) before the suffix.
        if username_start < username_end && prefix_pos > 0 {
            let username = &name[username_start..username_end];
            // Reject brackets in the username — that means this isn't a
            // well-formed suffix (e.g. `[NEXUS-DB-alice] extra]`).
            if !username.contains('[')
                && !username.contains(']')
                && !username.is_empty()
                && !name[..prefix_pos].trim().is_empty()
            {
                return FolderType::UserDropBox(username.to_string());
            }
        }
    }

    // Generic drop box: require an actual folder name before the suffix.
    if name_upper.ends_with(FOLDER_SUFFIX_DROPBOX) && name.len() > FOLDER_SUFFIX_DROPBOX.len() {
        let prefix_end = name.len() - FOLDER_SUFFIX_DROPBOX.len();
        if !name[..prefix_end].trim().is_empty() {
            return FolderType::DropBox;
        }
    }

    FolderType::Default
}

/// Returns true if `target` lies strictly *inside* a `[NEXUS-DB-username]`
/// folder owned by `username`. The walk uses **innermost-wins** semantics:
/// the first drop-box ancestor encountered defines the scope, and the
/// caller only gets the bypass if that ancestor is their own user drop
/// box. Hitting a generic `[NEXUS-DB]` or another user's drop box first
/// returns `false` — that ancestor's scope wins, not anything outside it.
///
/// `target` and `area_root` should both be virtual paths under the user's
/// area root — pass the candidate path (pre-canonicalization) so that
/// drop-box folders implemented as symlinks are still recognized by their
/// suffix.
///
/// The username comparison is case-insensitive (drop-box suffixes preserve
/// case but ownership is by identity, not display).
///
/// Returns false when `target == area_root`, when `target` is the drop
/// box folder itself, when the innermost drop-box ancestor isn't owned by
/// the requester (or is a generic drop box with no owner), or when no
/// drop-box ancestor exists at all.
#[must_use]
pub fn in_owned_dropbox(target: &Path, area_root: &Path, username: &str) -> bool {
    let username_lower = username.to_lowercase();
    let mut path = target.parent();

    while let Some(current) = path {
        if current == area_root {
            return false;
        }
        if let Some(name) = current.file_name().and_then(|n| n.to_str()) {
            match parse_folder_type(name) {
                FolderType::UserDropBox(owner) => {
                    // Innermost-wins: the first drop-box ancestor defines
                    // the scope. Bypass only applies if that ancestor is
                    // ours.
                    return owner.to_lowercase() == username_lower;
                }
                FolderType::DropBox => {
                    // Generic drop box has no owner — scope is admin-only.
                    return false;
                }
                _ => {}
            }
        }
        path = current.parent();
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // A bare suffix with no real folder name before it must NOT be a
        // writable folder type — regardless of leading whitespace.
        assert_eq!(parse_folder_type("[NEXUS-UL]"), FolderType::Default);
        assert_eq!(parse_folder_type("[NEXUS-DB]"), FolderType::Default);
        assert_eq!(parse_folder_type("[NEXUS-DB-alice]"), FolderType::Default);
        assert_eq!(parse_folder_type(" [NEXUS-UL]"), FolderType::Default);
        assert_eq!(parse_folder_type(" [NEXUS-DB]"), FolderType::Default);
        assert_eq!(parse_folder_type(" [NEXUS-DB-alice]"), FolderType::Default);
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
        // `[NEXUS-DB-]` with no username must not match as UserDropBox.
        assert_eq!(parse_folder_type("Files [NEXUS-DB-]"), FolderType::Default);
    }

    #[test]
    fn test_suffix_must_be_at_end() {
        // A suffix anywhere but the end must not match.
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
        // A username containing brackets means the suffix is malformed and
        // must not match as UserDropBox.
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
        // With multiple valid suffixes, the trailing one determines the type.
        assert_eq!(
            parse_folder_type("Folder [NEXUS-UL] [NEXUS-DB]"),
            FolderType::DropBox
        );
        assert_eq!(
            parse_folder_type("Folder [NEXUS-DB] [NEXUS-UL]"),
            FolderType::Upload
        );
    }

    #[test]
    fn test_in_owned_dropbox_direct_child() {
        let area_root = Path::new("/area");
        let target = Path::new("/area/For Alice [NEXUS-DB-alice]/file.txt");
        assert!(in_owned_dropbox(target, area_root, "alice"));
    }

    #[test]
    fn test_in_owned_dropbox_nested() {
        let area_root = Path::new("/area");
        let target = Path::new("/area/For Alice [NEXUS-DB-alice]/sub/deep/file.txt");
        assert!(in_owned_dropbox(target, area_root, "alice"));
    }

    #[test]
    fn test_in_owned_dropbox_case_insensitive_username() {
        let area_root = Path::new("/area");
        let target = Path::new("/area/For Alice [NEXUS-DB-Alice]/file.txt");
        assert!(in_owned_dropbox(target, area_root, "alice"));
        assert!(in_owned_dropbox(target, area_root, "ALICE"));
    }

    #[test]
    fn test_in_owned_dropbox_other_owner_rejected() {
        let area_root = Path::new("/area");
        let target = Path::new("/area/For Bob [NEXUS-DB-bob]/file.txt");
        assert!(!in_owned_dropbox(target, area_root, "alice"));
    }

    #[test]
    fn test_in_owned_dropbox_folder_itself_excluded() {
        // The drop-box folder itself is not "inside" its own dropbox.
        // Owner cannot delete the folder, only its contents.
        let area_root = Path::new("/area");
        let target = Path::new("/area/For Alice [NEXUS-DB-alice]");
        assert!(!in_owned_dropbox(target, area_root, "alice"));
    }

    #[test]
    fn test_in_owned_dropbox_outside_dropbox() {
        let area_root = Path::new("/area");
        let target = Path::new("/area/Documents/file.txt");
        assert!(!in_owned_dropbox(target, area_root, "alice"));
    }

    #[test]
    fn test_in_owned_dropbox_generic_dropbox_not_owned() {
        // [NEXUS-DB] (no username) doesn't grant ownership to anyone.
        let area_root = Path::new("/area");
        let target = Path::new("/area/Submissions [NEXUS-DB]/file.txt");
        assert!(!in_owned_dropbox(target, area_root, "alice"));
    }

    #[test]
    fn test_in_owned_dropbox_target_is_area_root() {
        let area_root = Path::new("/area");
        let target = Path::new("/area");
        assert!(!in_owned_dropbox(target, area_root, "alice"));
    }

    /// Nested drop boxes: alice's drop box contains bob's drop box.
    /// Innermost-wins: the inner box (bob's) defines the scope, regardless
    /// of who owns the outer. Bob gets bypass; alice does not, even though
    /// her dropbox is an ancestor.
    #[test]
    fn test_in_owned_dropbox_nested_innermost_wins() {
        let area_root = Path::new("/area");
        let target = Path::new("/area/For Alice [NEXUS-DB-alice]/inner [NEXUS-DB-bob]/file.txt");
        // Bob owns the innermost — bypass applies.
        assert!(in_owned_dropbox(target, area_root, "bob"));
        // Alice owns the outer but the innermost is bob's — bypass does
        // NOT apply. Bob's scope is bob's, not alice's.
        assert!(!in_owned_dropbox(target, area_root, "alice"));
    }

    /// Innermost-wins also stops at a generic `[NEXUS-DB]`. Even if a
    /// user-dropbox ancestor exists outside the generic dropbox, the
    /// generic dropbox's scope wins and the bypass does not apply.
    #[test]
    fn test_in_owned_dropbox_generic_dropbox_blocks_outer_match() {
        let area_root = Path::new("/area");
        let target = Path::new("/area/For Alice [NEXUS-DB-alice]/Submissions [NEXUS-DB]/file.txt");
        // Alice's outer dropbox is shadowed by the inner generic dropbox.
        assert!(!in_owned_dropbox(target, area_root, "alice"));
    }
}
