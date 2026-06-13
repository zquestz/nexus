//! Folder-type naming markers shared by the server and client.
//!
//! Folders advertise their type with a trailing marker (a required leading
//! space, then a bracketed token):
//!
//! - ` [NEXUS-UL]` — upload folder
//! - ` [NEXUS-DB]` — drop box (blind upload)
//! - ` [NEXUS-DB-<username>]` — user drop box
//!
//! Only the bracketed **delimiter tokens are ASCII**. The folder label before
//! the marker and the `<username>` inside a user drop box may be arbitrary
//! Unicode (usernames are Unicode), e.g. `"Café [NEXUS-DB-Ünïcödé]"`.
//!
//! Marker matching is therefore done with case-insensitive **ASCII** comparison
//! against the original string — never via [`str::to_uppercase`], which can
//! change a string's byte length (`ﬁ` → `FI`, `ß` → `SS`) and corrupt byte
//! offsets, producing non-char-boundary slices that panic. A successful ASCII
//! match guarantees the matched bytes are all ASCII, so the offsets it yields
//! are always valid char boundaries in the original string.

/// Upload-folder marker. Case-insensitive; includes the leading space.
pub const FOLDER_SUFFIX_UPLOAD: &str = " [NEXUS-UL]";
/// Drop-box marker. Case-insensitive; includes the leading space.
pub const FOLDER_SUFFIX_DROPBOX: &str = " [NEXUS-DB]";
/// User drop-box prefix (`<username>]` follows). Case-insensitive; includes the
/// leading space.
pub const FOLDER_SUFFIX_DROPBOX_PREFIX: &str = " [NEXUS-DB-";

/// If `name` ends with `suffix` (matched case-insensitively as ASCII), return
/// the byte index where the suffix begins, otherwise `None`.
///
/// The returned index is always a valid char boundary: a match means the
/// trailing `suffix.len()` bytes are all ASCII (an ASCII byte can never
/// `eq_ignore_ascii_case` a non-ASCII byte), so slicing `name[..start]` cannot
/// split a multi-byte character. `suffix` is expected to be ASCII.
#[must_use]
pub fn ascii_suffix_start(name: &str, suffix: &str) -> Option<usize> {
    let start = name.len().checked_sub(suffix.len())?;
    // `get` yields `None` (never panics) when `start` is not a char boundary.
    name.get(start..)?
        .eq_ignore_ascii_case(suffix)
        .then_some(start)
}

/// Byte index of the rightmost occurrence of `needle` in `name`, matched
/// case-insensitively as ASCII. `None` if absent or `needle` is empty.
///
/// The index is a valid char boundary because an ASCII match implies the
/// matched span is entirely ASCII.
fn rfind_ascii_ci(name: &str, needle: &str) -> Option<usize> {
    let needle = needle.as_bytes();
    if needle.is_empty() {
        return None;
    }
    name.as_bytes()
        .windows(needle.len())
        .rposition(|window| window.eq_ignore_ascii_case(needle))
}

/// Locate a trailing ` [NEXUS-DB-<username>]` user-drop-box marker.
///
/// Returns the byte index where the ` [NEXUS-DB-` prefix begins (a valid char
/// boundary) and the `<username>` slice (in its original case) between the
/// prefix and the final `]`.
///
/// This performs **no** validation of the username or the preceding label —
/// callers that gate permissions on the result must apply their own checks
/// (see the server's `parse_folder_type`).
#[must_use]
pub fn find_user_dropbox_marker(name: &str) -> Option<(usize, &str)> {
    if !name.ends_with(']') {
        return None;
    }
    let prefix_start = rfind_ascii_ci(name, FOLDER_SUFFIX_DROPBOX_PREFIX)?;
    let username_start = prefix_start + FOLDER_SUFFIX_DROPBOX_PREFIX.len();
    // The trailing `]` is ASCII, so `name.len() - 1` is a char boundary.
    let username_end = name.len() - 1;
    let username = name.get(username_start..username_end)?;
    Some((prefix_start, username))
}

/// Strip a trailing folder-type marker to get a display name, returning the
/// input unchanged when no recognized marker is present.
///
/// Lenient and cosmetic: it strips whatever recognized marker sits at the end
/// without validating the label or username. It is **not** an authority on
/// folder type — permission decisions use the server's stricter
/// `parse_folder_type`. Unicode-safe.
#[must_use]
pub fn strip_folder_suffix(name: &str) -> &str {
    if let Some((prefix_start, _username)) = find_user_dropbox_marker(name) {
        return &name[..prefix_start];
    }
    if let Some(start) = ascii_suffix_start(name, FOLDER_SUFFIX_DROPBOX) {
        return &name[..start];
    }
    if let Some(start) = ascii_suffix_start(name, FOLDER_SUFFIX_UPLOAD) {
        return &name[..start];
    }
    name
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_suffix_start_matches_case_insensitively() {
        assert_eq!(
            ascii_suffix_start("Uploads [NEXUS-UL]", FOLDER_SUFFIX_UPLOAD),
            Some(7)
        );
        assert_eq!(
            ascii_suffix_start("Uploads [nexus-ul]", FOLDER_SUFFIX_UPLOAD),
            Some(7)
        );
        assert_eq!(ascii_suffix_start("Uploads", FOLDER_SUFFIX_UPLOAD), None);
        // Suffix longer than the name must not underflow.
        assert_eq!(ascii_suffix_start("[UL]", FOLDER_SUFFIX_UPLOAD), None);
    }

    #[test]
    fn ascii_suffix_start_is_offset_safe_for_multibyte() {
        // "ﬁ" (U+FB01) is 3 bytes and would uppercase to "FI" (2 bytes); a
        // length-based offset would land mid-character. The returned index must
        // be a real char boundary, and a non-matching tail must yield None
        // rather than panic.
        let name = "ﬁ [NEXUS-UL]";
        let start = ascii_suffix_start(name, FOLDER_SUFFIX_UPLOAD).expect("matches");
        assert_eq!(&name[..start], "ﬁ");
        assert_eq!(ascii_suffix_start("ﬁ", FOLDER_SUFFIX_UPLOAD), None);
    }

    #[test]
    fn find_user_dropbox_marker_extracts_username() {
        assert_eq!(
            find_user_dropbox_marker("For Alice [NEXUS-DB-alice]"),
            Some((9, "alice"))
        );
        // Username case is preserved; the marker is matched case-insensitively.
        assert_eq!(
            find_user_dropbox_marker("For Alice [Nexus-DB-Alice]"),
            Some((9, "Alice"))
        );
        // Unicode username is returned intact.
        assert_eq!(
            find_user_dropbox_marker("Inbox [NEXUS-DB-Ünïcödé]"),
            Some((5, "Ünïcödé"))
        );
        // No trailing bracket, or a non-DB marker.
        assert_eq!(find_user_dropbox_marker("Inbox [NEXUS-DB-bob"), None);
        assert_eq!(find_user_dropbox_marker("Uploads [NEXUS-UL]"), None);
        // Empty username is returned as an empty slice (callers validate).
        assert_eq!(find_user_dropbox_marker("x [NEXUS-DB-]"), Some((1, "")));
    }

    #[test]
    fn find_user_dropbox_marker_is_offset_safe_for_multibyte_label() {
        // Length-changing char immediately before the marker — the H4 panic.
        assert_eq!(
            find_user_dropbox_marker("ﬁ [NEXUS-DB-bob]"),
            Some((3, "bob"))
        );
    }

    #[test]
    fn strip_reproduces_display_cases() {
        assert_eq!(strip_folder_suffix("My Documents"), "My Documents");
        assert_eq!(strip_folder_suffix("Uploads [NEXUS-UL]"), "Uploads");
        assert_eq!(strip_folder_suffix("Uploads [nexus-ul]"), "Uploads");
        assert_eq!(strip_folder_suffix("Uploads [Nexus-UL]"), "Uploads");
        assert_eq!(strip_folder_suffix("Admin Inbox [NEXUS-DB]"), "Admin Inbox");
        assert_eq!(strip_folder_suffix("Admin Inbox [nexus-db]"), "Admin Inbox");
        assert_eq!(
            strip_folder_suffix("For Alice [NEXUS-DB-alice]"),
            "For Alice"
        );
        assert_eq!(
            strip_folder_suffix("For Alice [Nexus-DB-Alice]"),
            "For Alice"
        );
        // Missing leading space, or marker not at the end — unchanged.
        assert_eq!(
            strip_folder_suffix("Uploads[NEXUS-UL]"),
            "Uploads[NEXUS-UL]"
        );
        assert_eq!(
            strip_folder_suffix("Files [NEXUS-UL] backup"),
            "Files [NEXUS-UL] backup"
        );
    }

    #[test]
    fn strip_is_offset_safe_for_multibyte() {
        // Each of these would panic under the old to_uppercase()+slice approach.
        assert_eq!(strip_folder_suffix("ﬁ [NEXUS-DB-bob]"), "ﬁ");
        assert_eq!(strip_folder_suffix("ﬁ [NEXUS-UL]"), "ﬁ");
        assert_eq!(strip_folder_suffix("Straße [NEXUS-DB]"), "Straße");
        assert_eq!(strip_folder_suffix("Café [NEXUS-DB-Ünïcödé]"), "Café");
        // A non-marker Unicode name is returned unchanged.
        assert_eq!(strip_folder_suffix("Сообщения"), "Сообщения");
    }

    #[test]
    fn strip_preserves_non_markers_and_malformed() {
        assert_eq!(strip_folder_suffix(""), "");
        // Brackets that aren't a recognized marker are left intact.
        assert_eq!(strip_folder_suffix("folder [other]"), "folder [other]");
        assert_eq!(strip_folder_suffix("[test] folder"), "[test] folder");
        // Incomplete markers (no closing bracket) are literal names.
        assert_eq!(strip_folder_suffix("folder [NEXUS-"), "folder [NEXUS-");
        assert_eq!(strip_folder_suffix("folder [NEXUS-UL"), "folder [NEXUS-UL");
        assert_eq!(strip_folder_suffix("folder [NEXUS-DB"), "folder [NEXUS-DB");
        assert_eq!(
            strip_folder_suffix("folder [NEXUS-DB-"),
            "folder [NEXUS-DB-"
        );
        assert_eq!(
            strip_folder_suffix("folder [NEXUS-DB-user"),
            "folder [NEXUS-DB-user"
        );
    }
}
