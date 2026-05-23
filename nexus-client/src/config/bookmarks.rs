//! Bookmark management methods for Config

use nexus_common::names::fold_name;
use uuid::Uuid;

use crate::types::ServerBookmark;

use super::Config;

impl Config {
    /// Add a new bookmark to the configuration
    pub fn add_bookmark(&mut self, bookmark: ServerBookmark) {
        self.bookmarks.push(bookmark);
    }

    /// Delete a bookmark by ID
    ///
    /// Does nothing if no bookmark with the given ID exists.
    pub fn delete_bookmark(&mut self, id: Uuid) {
        self.bookmarks.retain(|b| b.id != id);
    }

    /// Get a bookmark by ID
    ///
    /// Returns None if no bookmark with the given ID exists.
    pub fn get_bookmark(&self, id: Uuid) -> Option<&ServerBookmark> {
        self.bookmarks.iter().find(|b| b.id == id)
    }

    /// Get a mutable bookmark by ID
    ///
    /// Returns None if no bookmark with the given ID exists.
    pub fn get_bookmark_mut(&mut self, id: Uuid) -> Option<&mut ServerBookmark> {
        self.bookmarks.iter_mut().find(|b| b.id == id)
    }

    /// Update an existing bookmark by ID
    ///
    /// Does nothing if no bookmark with the given ID exists.
    pub fn update_bookmark(&mut self, id: Uuid, bookmark: ServerBookmark) {
        if let Some(existing) = self.bookmarks.iter_mut().find(|b| b.id == id) {
            *existing = bookmark;
        }
    }

    /// Find a bookmark whose connection params match the given values.
    ///
    /// String fields (`address`, `username`, `nickname`) are compared
    /// case-insensitively via `to_lowercase()` to handle IDN / Unicode input
    /// correctly — e.g. `Server.com` matches a bookmark stored as
    /// `server.com`. The port is compared exactly.
    ///
    /// **Nickname equivalence for regular accounts:** the server resolves
    /// nickname = username for regular accounts, so the stored bookmark
    /// nickname can be empty *or* a case-insensitive match for the username
    /// without changing what the connection means. We treat both as the
    /// same canonical "regular-account" form for matching, so a bookmark
    /// saved with explicit `nickname = "alice"` (= username) matches a
    /// connection attempt with an empty form-nickname, and vice versa.
    /// Shared-account nicknames (anything else) compare directly.
    ///
    /// Returns None if no bookmark matches. This is the canonical lookup
    /// used by the manual-form connect flow (resolving the stored
    /// fingerprint, the TOFU-save target, and the mismatch dialog's
    /// bookmark identity); URI flows match on a different shape and have
    /// their own inline lookups.
    pub fn find_bookmark_matching(
        &self,
        address: &str,
        port: u16,
        username: &str,
        nickname: &str,
    ) -> Option<&ServerBookmark> {
        let address_lc = address.to_lowercase();
        let username_lc = fold_name(username);
        let our_canonical_nick = canonical_nickname(nickname, username);
        self.bookmarks.iter().find(|b| {
            b.address.to_lowercase() == address_lc
                && b.port == port
                && fold_name(&b.username) == username_lc
                && canonical_nickname(&b.nickname, &b.username) == our_canonical_nick
        })
    }

    /// Find a bookmark by URI shape (host/port, optionally also username).
    ///
    /// `nexus://` URIs don't carry a nickname; the bookmark's nickname is
    /// whatever's stored locally. URIs without credentials match by
    /// `(host, port)` only; URIs with credentials add a username predicate.
    /// String fields compared case-insensitively via `to_lowercase()`.
    ///
    /// This is the canonical lookup for the URI-connect flow. Both the
    /// connect-time bookmark resolution (for credentials, display name,
    /// stored fingerprint) and the result-time identity lookup (for the
    /// fingerprint-interception dialog's `server_name`) go through here,
    /// so they always agree on which bookmark — if any — is associated
    /// with a given URI.
    pub fn find_bookmark_matching_uri(
        &self,
        address: &str,
        port: u16,
        username: Option<&str>,
    ) -> Option<&ServerBookmark> {
        let address_lc = address.to_lowercase();
        let username_lc = username.map(fold_name);
        self.bookmarks.iter().find(|b| {
            b.address.to_lowercase() == address_lc
                && b.port == port
                && match &username_lc {
                    Some(u) => &fold_name(&b.username) == u,
                    None => true,
                }
        })
    }
}

/// Reduce a nickname to its canonical form for bookmark matching.
///
/// Empty or username-equal (case-insensitive) → empty string.
/// Anything else → lowercased nickname.
///
/// Used by `find_bookmark_matching` so connection attempts that produce the
/// same server-side login resolve to the same bookmark regardless of which
/// equivalent form the nickname was saved or typed in.
fn canonical_nickname(nickname: &str, username: &str) -> String {
    let lc = fold_name(nickname);
    if lc.is_empty() || lc == fold_name(username) {
        String::new()
    } else {
        lc
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create a bookmark with just a name
    fn bookmark(name: &str) -> ServerBookmark {
        ServerBookmark {
            id: Uuid::new_v4(),
            name: name.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn test_add_bookmark() {
        let mut config = Config::default();
        config.add_bookmark(bookmark("Test Server"));

        assert_eq!(config.bookmarks.len(), 1);
        assert_eq!(config.bookmarks[0].name, "Test Server");
    }

    #[test]
    fn test_add_multiple_bookmarks() {
        let mut config = Config::default();
        config.add_bookmark(bookmark("Server 1"));
        config.add_bookmark(bookmark("Server 2"));

        assert_eq!(config.bookmarks.len(), 2);
        assert_eq!(config.bookmarks[0].name, "Server 1");
        assert_eq!(config.bookmarks[1].name, "Server 2");
    }

    #[test]
    fn test_delete_bookmark() {
        let mut config = Config::default();
        let bookmark1 = bookmark("Server 1");
        let bookmark2 = bookmark("Server 2");
        let id1 = bookmark1.id;
        config.add_bookmark(bookmark1);
        config.add_bookmark(bookmark2);

        config.delete_bookmark(id1);

        assert_eq!(config.bookmarks.len(), 1);
        assert_eq!(config.bookmarks[0].name, "Server 2");
    }

    #[test]
    fn test_delete_bookmark_nonexistent() {
        let mut config = Config::default();
        config.add_bookmark(bookmark("Server 1"));

        config.delete_bookmark(Uuid::new_v4());

        assert_eq!(config.bookmarks.len(), 1);
        assert_eq!(config.bookmarks[0].name, "Server 1");
    }

    #[test]
    fn test_get_bookmark() {
        let mut config = Config::default();
        let bm = bookmark("Test Server");
        let id = bm.id;
        config.add_bookmark(bm);

        let result = config.get_bookmark(id);
        assert_eq!(result.map(|b| b.name.as_str()), Some("Test Server"));
    }

    #[test]
    fn test_get_bookmark_nonexistent() {
        let mut config = Config::default();
        config.add_bookmark(bookmark("Test Server"));

        assert!(config.get_bookmark(Uuid::new_v4()).is_none());
    }

    #[test]
    fn test_update_bookmark() {
        let mut config = Config::default();
        let bm = bookmark("Original");
        let id = bm.id;
        config.add_bookmark(bm);

        config.update_bookmark(
            id,
            ServerBookmark {
                id,
                name: "Updated".to_string(),
                address: "200::2".to_string(),
                port: 8000,
                auto_connect: true,
                ..Default::default()
            },
        );

        assert_eq!(config.bookmarks.len(), 1);
        assert_eq!(config.bookmarks[0].name, "Updated");
        assert_eq!(config.bookmarks[0].address, "200::2");
        assert_eq!(config.bookmarks[0].port, 8000);
        assert!(config.bookmarks[0].auto_connect);
    }

    #[test]
    fn test_update_bookmark_nonexistent() {
        let mut config = Config::default();
        config.add_bookmark(bookmark("Server 1"));

        config.update_bookmark(Uuid::new_v4(), bookmark("Should Not Appear"));

        assert_eq!(config.bookmarks.len(), 1);
        assert_eq!(config.bookmarks[0].name, "Server 1");
    }

    /// Helper to build a bookmark with full connection params for matching tests.
    fn bookmark_with_params(
        name: &str,
        address: &str,
        port: u16,
        username: &str,
        nickname: &str,
    ) -> ServerBookmark {
        ServerBookmark {
            id: Uuid::new_v4(),
            name: name.to_string(),
            address: address.to_string(),
            port,
            username: username.to_string(),
            nickname: nickname.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn test_find_bookmark_matching_exact() {
        let mut config = Config::default();
        config.add_bookmark(bookmark_with_params(
            "Test",
            "server.example",
            7500,
            "alice",
            "Alice",
        ));

        let result = config.find_bookmark_matching("server.example", 7500, "alice", "Alice");
        assert_eq!(result.map(|b| b.name.as_str()), Some("Test"));
    }

    #[test]
    fn test_find_bookmark_matching_case_insensitive_address() {
        let mut config = Config::default();
        config.add_bookmark(bookmark_with_params(
            "Test",
            "server.example",
            7500,
            "alice",
            "Alice",
        ));

        // User typed mixed case — should still find the bookmark.
        let result = config.find_bookmark_matching("Server.Example", 7500, "alice", "Alice");
        assert_eq!(result.map(|b| b.name.as_str()), Some("Test"));
    }

    #[test]
    fn test_find_bookmark_matching_case_insensitive_username() {
        let mut config = Config::default();
        config.add_bookmark(bookmark_with_params(
            "Test",
            "server.example",
            7500,
            "alice",
            "Alice",
        ));

        let result = config.find_bookmark_matching("server.example", 7500, "ALICE", "Alice");
        assert_eq!(result.map(|b| b.name.as_str()), Some("Test"));
    }

    #[test]
    fn test_find_bookmark_matching_case_insensitive_nickname() {
        let mut config = Config::default();
        config.add_bookmark(bookmark_with_params(
            "Test",
            "server.example",
            7500,
            "alice",
            "Alice",
        ));

        let result = config.find_bookmark_matching("server.example", 7500, "alice", "alice");
        assert_eq!(result.map(|b| b.name.as_str()), Some("Test"));
    }

    #[test]
    fn test_find_bookmark_matching_unicode_case_folded() {
        // Non-ASCII case folding — confirms we use to_lowercase, not eq_ignore_ascii_case.
        // Turkish dotted-I example: Ä lowercases to ä.
        let mut config = Config::default();
        config.add_bookmark(bookmark_with_params(
            "Test",
            "Ärlich.example",
            7500,
            "alice",
            "Alice",
        ));

        let result = config.find_bookmark_matching("ärlich.example", 7500, "alice", "Alice");
        assert!(
            result.is_some(),
            "Unicode case folding must work for non-ASCII addresses"
        );
    }

    #[test]
    fn test_find_bookmark_matching_port_mismatch() {
        let mut config = Config::default();
        config.add_bookmark(bookmark_with_params(
            "Test",
            "server.example",
            7500,
            "alice",
            "Alice",
        ));

        let result = config.find_bookmark_matching("server.example", 7501, "alice", "Alice");
        assert!(result.is_none());
    }

    #[test]
    fn test_find_bookmark_matching_username_mismatch() {
        let mut config = Config::default();
        config.add_bookmark(bookmark_with_params(
            "Test",
            "server.example",
            7500,
            "alice",
            "Alice",
        ));

        let result = config.find_bookmark_matching("server.example", 7500, "bob", "Alice");
        assert!(result.is_none());
    }

    #[test]
    fn test_find_bookmark_matching_nickname_mismatch() {
        // Two shared-account bookmarks differing only by nickname must resolve
        // to the right one. This is the H1 regression case: missing nickname
        // predicate would let "alice/Alice" and "alice/Bob" collide.
        let mut config = Config::default();
        config.add_bookmark(bookmark_with_params(
            "AliceBM",
            "server.example",
            7500,
            "shared",
            "Alice",
        ));
        config.add_bookmark(bookmark_with_params(
            "BobBM",
            "server.example",
            7500,
            "shared",
            "Bob",
        ));

        let result = config.find_bookmark_matching("server.example", 7500, "shared", "Bob");
        assert_eq!(result.map(|b| b.name.as_str()), Some("BobBM"));
    }

    #[test]
    fn test_find_bookmark_matching_empty_nickname() {
        let mut config = Config::default();
        config.add_bookmark(bookmark_with_params(
            "Test",
            "server.example",
            7500,
            "alice",
            "",
        ));

        // Both bookmark and lookup carry an empty nickname — should match.
        let result = config.find_bookmark_matching("server.example", 7500, "alice", "");
        assert_eq!(result.map(|b| b.name.as_str()), Some("Test"));
    }

    #[test]
    fn test_find_bookmark_matching_no_bookmarks() {
        let config = Config::default();
        let result = config.find_bookmark_matching("server.example", 7500, "alice", "Alice");
        assert!(result.is_none());
    }

    // -- canonical-nickname equivalence (regular-account variants) ---------

    #[test]
    fn test_find_bookmark_matching_form_empty_bookmark_username() {
        // Bookmark saved with nickname == username; user typed empty nickname.
        // Server-side these resolve to the same regular-account login, so
        // the lookup must match.
        let mut config = Config::default();
        config.add_bookmark(bookmark_with_params(
            "Test",
            "server.example",
            7500,
            "alice",
            "alice", // bookmark nickname == username
        ));

        let result = config.find_bookmark_matching("server.example", 7500, "alice", "");
        assert_eq!(result.map(|b| b.name.as_str()), Some("Test"));
    }

    #[test]
    fn test_find_bookmark_matching_bookmark_empty_form_username() {
        // Symmetric to the above: bookmark saved without a nickname; user
        // typed nickname = username.
        let mut config = Config::default();
        config.add_bookmark(bookmark_with_params(
            "Test",
            "server.example",
            7500,
            "alice",
            "", // empty bookmark nickname
        ));

        let result = config.find_bookmark_matching("server.example", 7500, "alice", "alice");
        assert_eq!(result.map(|b| b.name.as_str()), Some("Test"));
    }

    #[test]
    fn test_find_bookmark_matching_canonical_handles_case() {
        // Bookmark stored with mixed-case username-as-nickname; form has
        // empty nickname. Both canonicalize to empty.
        let mut config = Config::default();
        config.add_bookmark(bookmark_with_params(
            "Test",
            "server.example",
            7500,
            "alice",
            "Alice", // case differs from username; still canonical-empty
        ));

        let result = config.find_bookmark_matching("server.example", 7500, "alice", "");
        assert_eq!(result.map(|b| b.name.as_str()), Some("Test"));
    }

    #[test]
    fn test_find_bookmark_matching_shared_account_distinct_nickname() {
        // Shared account: nickname is meaningful. A connection attempt with
        // a different nickname must NOT match.
        let mut config = Config::default();
        config.add_bookmark(bookmark_with_params(
            "BobBM",
            "server.example",
            7500,
            "shared",
            "Bob",
        ));

        let result = config.find_bookmark_matching("server.example", 7500, "shared", "Charlie");
        assert!(result.is_none());
    }

    #[test]
    fn test_find_bookmark_matching_canonical_first_match_wins() {
        // Two bookmarks both canonicalize to regular-account (empty nickname):
        // one stored with "" and one stored with username-as-nickname. Form
        // input matches both. Pin "first match wins" — Vec::iter().find()
        // returns insertion order, so the older bookmark wins. A storage-order
        // change (or switch to a different lookup primitive) would be caught.
        let mut config = Config::default();
        config.add_bookmark(bookmark_with_params(
            "First",
            "server.example",
            7500,
            "alice",
            "", // canonical-empty
        ));
        config.add_bookmark(bookmark_with_params(
            "Second",
            "server.example",
            7500,
            "alice",
            "alice", // also canonical-empty (= username)
        ));

        let result = config.find_bookmark_matching("server.example", 7500, "alice", "");
        assert_eq!(result.map(|b| b.name.as_str()), Some("First"));
    }

    #[test]
    fn test_find_bookmark_matching_whitespace_address_does_not_match() {
        // The helper itself is strict on address — caller is responsible for
        // trimming whitespace before lookup. In practice callers DO trim at
        // input boundaries (handle_save_bookmark and handle_connect_pressed),
        // so this case shouldn't fire in normal flows; pin the contract here.
        let mut config = Config::default();
        config.add_bookmark(bookmark_with_params(
            "Test",
            "server.example",
            7500,
            "alice",
            "Alice",
        ));

        let result = config.find_bookmark_matching("server.example ", 7500, "alice", "Alice");
        assert!(result.is_none());
    }

    // -- find_bookmark_matching_uri (URI-shape lookup) ---------------------

    #[test]
    fn test_find_bookmark_matching_uri_no_username() {
        // URI without creds: 2-tuple match on (host, port).
        let mut config = Config::default();
        config.add_bookmark(bookmark_with_params(
            "Test",
            "server.example",
            7500,
            "alice",
            "Alice",
        ));

        let result = config.find_bookmark_matching_uri("server.example", 7500, None);
        assert_eq!(result.map(|b| b.name.as_str()), Some("Test"));
    }

    #[test]
    fn test_find_bookmark_matching_uri_with_username() {
        // URI with creds: 3-tuple match on (host, port, username).
        let mut config = Config::default();
        config.add_bookmark(bookmark_with_params(
            "Test",
            "server.example",
            7500,
            "alice",
            "Alice",
        ));

        let result = config.find_bookmark_matching_uri("server.example", 7500, Some("alice"));
        assert_eq!(result.map(|b| b.name.as_str()), Some("Test"));
    }

    #[test]
    fn test_find_bookmark_matching_uri_username_mismatch() {
        let mut config = Config::default();
        config.add_bookmark(bookmark_with_params(
            "Test",
            "server.example",
            7500,
            "alice",
            "Alice",
        ));

        let result = config.find_bookmark_matching_uri("server.example", 7500, Some("bob"));
        assert!(result.is_none());
    }

    #[test]
    fn test_find_bookmark_matching_uri_case_insensitive() {
        let mut config = Config::default();
        config.add_bookmark(bookmark_with_params(
            "Test",
            "server.example",
            7500,
            "alice",
            "Alice",
        ));

        // Mixed case host AND username — both lowercased on match.
        let result = config.find_bookmark_matching_uri("Server.Example", 7500, Some("ALICE"));
        assert_eq!(result.map(|b| b.name.as_str()), Some("Test"));
    }

    #[test]
    fn test_find_bookmark_matching_uri_port_mismatch() {
        let mut config = Config::default();
        config.add_bookmark(bookmark_with_params(
            "Test",
            "server.example",
            7500,
            "alice",
            "Alice",
        ));

        let result = config.find_bookmark_matching_uri("server.example", 7501, None);
        assert!(result.is_none());
    }
}
