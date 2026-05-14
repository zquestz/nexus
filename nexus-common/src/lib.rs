//! Nexus Common Library
//!
//! Shared types, protocols, and utilities for the Nexus BBS system.

use std::time::Duration;

pub mod address;
mod error_kind;
pub mod fingerprint;
pub mod framing;
pub mod hash;
pub mod io;
#[cfg(feature = "logging")]
pub mod logging;
pub mod protocol;
pub mod secure_file;
pub mod time;
#[cfg(feature = "tls")]
pub mod tls;
pub mod tracker_protocol;
#[cfg(feature = "upnp")]
pub mod upnp;
pub mod validators;
pub mod version;
pub mod voice;
#[cfg(feature = "websocket")]
pub mod websocket;

pub use error_kind::{
    ERROR_KIND_CAPACITY, ERROR_KIND_CONFLICT, ERROR_KIND_EXISTS, ERROR_KIND_HASH_MISMATCH,
    ERROR_KIND_INVALID, ERROR_KIND_INVALID_PATH, ERROR_KIND_IO_ERROR, ERROR_KIND_NOT_FOUND,
    ERROR_KIND_PERMISSION, ERROR_KIND_PROTOCOL_ERROR, ERROR_KIND_RATE_LIMITED,
    ERROR_KIND_TRACKER_ADDRESS_INVALID, ERROR_KIND_TRACKER_CONNECTION_FAILED,
    ERROR_KIND_TRACKER_CONNECTION_LOST, ERROR_KIND_TRACKER_DB_FAILED,
    ERROR_KIND_TRACKER_FINGERPRINT_INTERCEPTED, ERROR_KIND_TRACKER_FINGERPRINT_MISMATCH,
    ERROR_KIND_TRACKER_HANDSHAKE_FAILED, ERROR_KIND_TRACKER_PROTOCOL_ERROR,
    ERROR_KIND_TRACKER_TLS_FAILED, ERROR_KIND_UNAUTHORIZED, ErrorKind, is_unrecoverable_error_kind,
    is_valid_error_kind,
};

/// Version information for the Nexus BBS protocol
pub const PROTOCOL_VERSION: &str = "0.8.4";

/// Version information for the Nexus tracker protocol
///
/// Versioned independently of `PROTOCOL_VERSION` so the BBS protocol and
/// tracker protocol can evolve at their own pace.
pub const TRACKER_PROTOCOL_VERSION: &str = "0.1.0";

/// Default port for Nexus BBS connections
pub const DEFAULT_PORT: u16 = 7500;

/// Default port for file transfers
pub const DEFAULT_TRANSFER_PORT: u16 = 7501;

/// Default port for WebSocket BBS connections
pub const DEFAULT_WEBSOCKET_PORT: u16 = 7502;

/// Default port for WebSocket file transfers
pub const DEFAULT_TRANSFER_WEBSOCKET_PORT: u16 = 7503;

/// Default port for tracker connections
pub const DEFAULT_TRACKER_PORT: u16 = 7510;

/// Minimum tracker refresh interval (seconds). Shared between the
/// tracker daemon (CLI floor in `nexus-tracker::args`) and the
/// BBS-side registration task (clamps received intervals so a buggy
/// or hostile tracker can't drive the publisher into a tight refresh
/// loop). Per `docs/protocol/18-trackers.md` § Refresh interval.
pub const MIN_REFRESH_INTERVAL_SECS: u32 = 120;

/// Default locale for daemons (English). Used as the fallback when a
/// peer-supplied locale is unknown or a translation key is missing in
/// the requested locale. Each daemon re-exports this from its own
/// `constants.rs` so internal call sites keep their existing import
/// paths.
pub const DEFAULT_LOCALE: &str = "en";

/// Owner-only directory mode for daemon data directories on Unix.
///
/// Used by `nexus-server` and `nexus-tracker` for both the data
/// directory itself and the `logs/` subdirectory inside it. The value is
/// `0o700` (read/write/execute for owner only) so directory listings
/// don't leak filenames or existence to other local users; the
/// per-file `0o600` modes inside cover the contents.
#[cfg(unix)]
pub const DATA_DIR_MODE: u32 = 0o700;

/// Prefix prepended to TLS handshake failures bubbling out of
/// `TlsAcceptor::accept`. The shared connection-error logger in each
/// daemon's `main.rs` checks for this substring to downgrade scanner /
/// incompatible-client noise to debug level. Producers compose with
/// `format!("{}{}", TLS_HANDSHAKE_FAILED_PREFIX, e)` — the trailing
/// `": "` is part of the constant.
pub const TLS_HANDSHAKE_FAILED_PREFIX: &str = "TLS handshake failed: ";

/// Prefix prepended to WebSocket handshake failures bubbling out of
/// `tokio_tungstenite::accept_async`. Used the same way as
/// [`TLS_HANDSHAKE_FAILED_PREFIX`].
pub const WS_HANDSHAKE_FAILED_PREFIX: &str = "WebSocket handshake failed: ";

/// RFC 6761-defined loopback hostname. Used by hostname-comparison
/// code paths (e.g. proxy-bypass detection) that need to recognize the
/// user-typed literal `"localhost"` as referring to the local machine
/// without a DNS lookup. Distinct in intent from [`SNI_SERVER_NAME`]
/// even though both currently evaluate to the same string.
pub const LOCALHOST_HOSTNAME: &str = "localhost";

/// Sentinel `ServerName` value used by every TLS-client path in the
/// workspace (BBS-port connect, transfer-port connect, tracker-task
/// connect, and the client-side tracker query). The rustls config
/// disables SNI (`enable_sni = false`) and we TOFU-pin certs via
/// `AcceptAnyVerifier`, so the `ServerName` is purely internal —
/// never sent on the wire and never compared against the cert. The
/// literal `"localhost"` keeps the value uniform across paths and
/// avoids per-host parsing (IPv6 brackets, IDN, zone IDs).
pub const SNI_SERVER_NAME: &str = "localhost";

/// `.expect()` message for `ServerName::try_from(SNI_SERVER_NAME)`.
/// `SNI_SERVER_NAME` resolves to a structurally valid DNS name, so the
/// parse never fails — but rustls's API returns `Result`, so callers
/// `.expect` with this fixed-string contract proof.
pub const EXPECT_SNI_SERVER_NAME_VALID_DNS: &str = "SNI_SERVER_NAME is a valid DNS name";

/// Maximum time to wait for the TLS handshake to complete on accept.
/// Slowloris defense: a peer that opens TCP and sends a partial
/// ClientHello (or stops mid-handshake) would otherwise hold a
/// connection task indefinitely. 30s is generous vs real TLS (<1s on
/// reasonable links, even over Yggdrasil/mobile) and short enough to
/// bound per-IP attacker exposure beyond the connection-rate budget.
pub const TLS_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum time to wait for the WebSocket HTTP-101 upgrade to complete
/// after TLS. Same slowloris-defense purpose as
/// [`TLS_HANDSHAKE_TIMEOUT`]; an upgrade after TLS takes <100ms in
/// practice, so 30s is purely budget for stalled / hostile peers.
pub const WS_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);

/// Error message used when [`TLS_HANDSHAKE_TIMEOUT`] elapses. Composed
/// to match the [`TLS_HANDSHAKE_FAILED_PREFIX`] format so
/// `log_connection_error` downgrades it to debug, same as other TLS
/// handshake failures.
pub const TLS_HANDSHAKE_TIMEOUT_MSG: &str = "TLS handshake failed: timeout";

/// Error message used when [`WS_HANDSHAKE_TIMEOUT`] elapses. Composed
/// to match the [`WS_HANDSHAKE_FAILED_PREFIX`] format.
pub const WS_HANDSHAKE_TIMEOUT_MSG: &str = "WebSocket handshake failed: timeout";

/// Backoff between `TcpListener::accept` retries after an `Err`.
///
/// Caps the accept-loop iteration rate at ~10/s while the failure
/// condition persists, which prevents a tight CPU loop on resource-
/// exhaustion errors (`EMFILE`, `ENFILE`, `ENOMEM`/`ENOBUFS`) where
/// the immediate retry would otherwise burn CPU and log bandwidth
/// while *making the recovery slower* (high CPU starves the processes
/// that need to release FDs / memory). The kernel's listen backlog
/// queues new connections during the sleep, so this only adds latency
/// in the error path, never in normal operation.
pub const ACCEPT_ERROR_BACKOFF: Duration = Duration::from_millis(100);

/// Default port for WebSocket tracker connections
pub const DEFAULT_TRACKER_WEBSOCKET_PORT: u16 = 7511;

/// Buffer size for SHA-256 hashing operations (1MB for fewer syscalls)
pub const HASH_BUFFER_SIZE: usize = 1024 * 1024;

/// How often to send keepalive notifications during transfers.
///
/// This interval (10 seconds) is chosen to be well under the typical idle timeout
/// (30 seconds) while not overwhelming the network with keepalive messages.
/// At 500 MB/s, a 100GB file takes ~200 seconds, generating ~20 keepalive messages.
pub const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(10);

/// Suffix for incomplete transfer files (.part)
pub const PART_SUFFIX: &str = ".part";

/// Fallback file name for keepalive messages when path has no file name
pub const FALLBACK_FILE_NAME: &str = "file";

/// Fallback file name for keepalive messages when .part path has no file name
/// This is `FALLBACK_FILE_NAME` + `PART_SUFFIX`.
pub const FALLBACK_PART_FILE_NAME: &str = "file.part";

/// Default port as a string for form fields and display.
///
/// This is the string representation of [`DEFAULT_PORT`], provided as a constant
/// because Rust doesn't support const string formatting.
pub const DEFAULT_PORT_STR: &str = "7500";

/// All available permissions in the Nexus protocol.
///
/// These permission strings are used by both client and server to manage
/// user access control. The list is maintained in alphabetical order.
///
/// Permission meanings:
/// - `ban_create`: Create/update IP bans
/// - `ban_delete`: Remove IP bans
/// - `ban_list`: View list of active bans
/// - `chat_create`: Create new chat channels
/// - `chat_join`: Join existing chat channels
/// - `chat_list`: View list of available channels
/// - `chat_receive`: Receive chat messages in chat channels
/// - `chat_secret`: Toggle secret mode on channels
/// - `chat_send`: Send chat messages to chat channels
/// - `chat_topic`: View the server topic
/// - `chat_topic_edit`: Edit the server topic
/// - `chat_unlimited`: Bypass chat rate limiting (flood protection)
/// - `file_copy`: Copy files and directories
/// - `file_create_dir`: Create directories anywhere in file area
/// - `file_delete`: Delete files and empty directories
/// - `file_download`: Download files from file area
/// - `file_info`: View detailed file/directory information
/// - `file_list`: Browse files and directories in user's area
/// - `file_move`: Move files and directories
/// - `file_rename`: Rename files and directories
/// - `file_root`: Browse entire file area from root (for admins/file managers)
/// - `file_upload`: Upload files to upload/dropbox folders
/// - `file_upload_anywhere`: Upload files to any directory, bypassing upload/dropbox folder restriction
/// - `group_create`: Create new account groups
/// - `group_delete`: Delete account groups
/// - `group_edit`: Edit account group name and permissions
/// - `news_create`: Create news posts
/// - `news_delete`: Delete any news post (without: only own posts)
/// - `news_edit`: Edit any news post (without: only own posts)
/// - `news_list`: View news posts
/// - `tracker_add`: Add a tracker to the server's tracker list
/// - `tracker_edit`: Edit a tracker's configuration (also gates fetching detail for the edit form, and accepting a Stage 1 pending fingerprint)
/// - `tracker_list`: View the server's configured trackers and their runtime status
/// - `tracker_remove`: Remove a tracker from the server's tracker list
/// - `trust_create`: Create/update trusted IPs
/// - `trust_delete`: Remove trusted IPs
/// - `trust_list`: View list of trusted IPs
/// - `user_broadcast`: Send broadcast messages to all users
/// - `user_create`: Create new user accounts
/// - `user_delete`: Delete user accounts
/// - `user_edit`: Edit user accounts
/// - `user_info`: View detailed user information
/// - `user_kick`: Kick/disconnect users
/// - `user_list`: View the list of connected users
/// - `user_message`: Send user messages
/// - `voice_listen`: Receive audio from others in voice chat
/// - `voice_talk`: Transmit audio in voice chat
pub const ALL_PERMISSIONS: &[&str] = &[
    "ban_create",
    "ban_delete",
    "ban_list",
    "chat_create",
    "chat_join",
    "chat_list",
    "chat_receive",
    "chat_secret",
    "chat_send",
    "chat_topic",
    "chat_topic_edit",
    "chat_unlimited",
    "connection_monitor",
    "file_copy",
    "file_create_dir",
    "file_delete",
    "file_download",
    "file_info",
    "file_list",
    "file_move",
    "file_reindex",
    "file_rename",
    "file_root",
    "file_search",
    "file_upload",
    "file_upload_anywhere",
    "group_create",
    "group_delete",
    "group_edit",
    "news_create",
    "news_delete",
    "news_edit",
    "news_list",
    "tracker_add",
    "tracker_edit",
    "tracker_list",
    "tracker_remove",
    "trust_create",
    "trust_delete",
    "trust_list",
    "user_broadcast",
    "user_create",
    "user_delete",
    "user_edit",
    "user_info",
    "user_kick",
    "user_list",
    "user_message",
    "voice_listen",
    "voice_talk",
];

/// Number of permissions in the system.
///
/// This is derived from `ALL_PERMISSIONS.len()` and provided as a const
/// for use in places that need the count without calling `.len()` repeatedly.
pub const PERMISSIONS_COUNT: usize = ALL_PERMISSIONS.len();

/// Permissions that can be granted to shared accounts.
///
/// Shared accounts are restricted to read-only operations and basic chat/file functionality.
/// They cannot perform any actions that modify database records (except sending messages
/// and uploading files to designated upload folders).
///
/// Allowed permissions:
/// - `ban_list`: View list of active bans
/// - `chat_create`: Create new chat channels
/// - `chat_join`: Join existing chat channels
/// - `chat_list`: View list of available channels
/// - `chat_receive`: Receive chat messages in chat channels
/// - `chat_secret`: Toggle secret mode on channels
/// - `chat_send`: Send chat messages to chat channels
/// - `chat_topic`: View the server topic (but not edit)
/// - `chat_unlimited`: Bypass chat rate limiting (flood protection)
/// - `file_download`: Download files from file area
/// - `file_info`: View detailed file/directory information
/// - `file_list`: Browse files and directories (read-only)
/// - `file_search`: Search files in the file area
/// - `file_upload`: Upload files to upload/dropbox folders
/// - `news_list`: View news posts (but not create/edit/delete)
/// - `trust_list`: View list of trusted IPs
/// - `user_info`: View detailed user information
/// - `user_list`: View the list of connected users
/// - `user_message`: Send user messages
/// - `voice_listen`: Receive audio from others in voice chat
/// - `voice_talk`: Transmit audio in voice chat
pub const SHARED_ACCOUNT_PERMISSIONS: &[&str] = &[
    "ban_list",
    "chat_create",
    "chat_join",
    "chat_list",
    "chat_receive",
    "chat_secret",
    "chat_send",
    "chat_topic",
    "chat_unlimited",
    "file_download",
    "file_info",
    "file_list",
    "file_search",
    "file_upload",
    "news_list",
    "trust_list",
    "user_info",
    "user_list",
    "user_message",
    "voice_listen",
    "voice_talk",
];

/// Check if a permission is allowed for shared accounts
///
/// Shared accounts have a restricted set of permissions. This function
/// returns `true` if the given permission string is in the allowed set.
///
/// # Arguments
///
/// * `permission` - The permission string to check (e.g., "chat_send")
///
/// # Returns
///
/// `true` if the permission is allowed for shared accounts, `false` otherwise.
pub fn is_shared_account_permission(permission: &str) -> bool {
    SHARED_ACCOUNT_PERMISSIONS.contains(&permission)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fallback_part_file_name_consistency() {
        let expected = format!("{}{}", FALLBACK_FILE_NAME, PART_SUFFIX);
        assert_eq!(
            FALLBACK_PART_FILE_NAME, expected,
            "FALLBACK_PART_FILE_NAME must equal FALLBACK_FILE_NAME + PART_SUFFIX"
        );
    }

    // Protocol-version semver validity is covered by `version::tests::test_protocol_version_parses`
    // and `test_tracker_protocol_version_parses`; not duplicated here.

    #[test]
    fn test_default_port() {
        // Verify default port is the expected value
        assert_eq!(DEFAULT_PORT, 7500);
    }

    #[test]
    fn test_default_transfer_port() {
        // Verify default transfer port is the expected value
        assert_eq!(DEFAULT_TRANSFER_PORT, 7501);
    }

    #[test]
    fn test_default_websocket_port() {
        // Verify default WebSocket port is the expected value
        assert_eq!(DEFAULT_WEBSOCKET_PORT, 7502);
    }

    #[test]
    fn test_default_transfer_websocket_port() {
        // Verify default WebSocket transfer port is the expected value
        assert_eq!(DEFAULT_TRANSFER_WEBSOCKET_PORT, 7503);
    }

    #[test]
    fn test_default_tracker_port() {
        // Verify default tracker port is the expected value
        assert_eq!(DEFAULT_TRACKER_PORT, 7510);
    }

    #[test]
    fn test_default_tracker_websocket_port() {
        // Verify default WebSocket tracker port is the expected value
        assert_eq!(DEFAULT_TRACKER_WEBSOCKET_PORT, 7511);
    }

    #[test]
    fn test_min_refresh_interval_matches_protocol_minimum() {
        // The tracker protocol mandates a server-side floor of ≥120s
        // (see docs/protocol/18-trackers.md § Refresh interval).
        // Both the tracker daemon (CLI floor in `nexus-tracker::args`)
        // and the BBS-side registration task floor on this constant;
        // a future bump in either side must update both, and this
        // assertion locks in the protocol-mandated value.
        assert_eq!(MIN_REFRESH_INTERVAL_SECS, 120);
    }

    #[test]
    fn test_default_port_str_matches() {
        // Verify DEFAULT_PORT_STR matches DEFAULT_PORT
        assert_eq!(DEFAULT_PORT_STR, DEFAULT_PORT.to_string());
    }

    #[test]
    fn test_all_permissions_count() {
        // Verify we have the expected number of permissions (50)
        assert_eq!(ALL_PERMISSIONS.len(), 50);
    }

    #[test]
    fn test_shared_account_permissions_count() {
        // Verify we have the expected number of shared account permissions (21)
        assert_eq!(SHARED_ACCOUNT_PERMISSIONS.len(), 21);
    }

    #[test]
    fn test_is_shared_account_permission() {
        // Allowed permissions
        assert!(is_shared_account_permission("ban_list"));
        assert!(is_shared_account_permission("chat_create"));
        assert!(is_shared_account_permission("chat_join"));
        assert!(is_shared_account_permission("chat_list"));
        assert!(is_shared_account_permission("chat_receive"));
        assert!(is_shared_account_permission("chat_secret"));
        assert!(is_shared_account_permission("chat_send"));
        assert!(is_shared_account_permission("chat_topic"));
        assert!(is_shared_account_permission("chat_unlimited"));
        assert!(is_shared_account_permission("file_download"));
        assert!(is_shared_account_permission("file_info"));
        assert!(is_shared_account_permission("file_list"));
        assert!(is_shared_account_permission("file_search"));
        assert!(is_shared_account_permission("file_upload"));
        assert!(is_shared_account_permission("news_list"));
        assert!(is_shared_account_permission("trust_list"));
        assert!(is_shared_account_permission("user_info"));
        assert!(is_shared_account_permission("user_list"));
        assert!(is_shared_account_permission("user_message"));
        assert!(is_shared_account_permission("voice_listen"));
        assert!(is_shared_account_permission("voice_talk"));

        // Forbidden permissions
        assert!(!is_shared_account_permission("ban_create"));
        assert!(!is_shared_account_permission("ban_delete"));
        assert!(!is_shared_account_permission("user_create"));
        assert!(!is_shared_account_permission("user_delete"));
        assert!(!is_shared_account_permission("user_edit"));
        assert!(!is_shared_account_permission("user_kick"));
        assert!(!is_shared_account_permission("user_broadcast"));
        assert!(!is_shared_account_permission("chat_topic_edit"));
        assert!(!is_shared_account_permission("news_create"));
        assert!(!is_shared_account_permission("news_edit"));
        assert!(!is_shared_account_permission("news_delete"));
        assert!(!is_shared_account_permission("file_root"));
        assert!(!is_shared_account_permission("file_copy"));
        assert!(!is_shared_account_permission("file_move"));
        assert!(!is_shared_account_permission("file_rename"));
        assert!(!is_shared_account_permission("file_delete"));
        assert!(!is_shared_account_permission("file_create_dir"));
        assert!(!is_shared_account_permission("file_upload_anywhere"));
        assert!(!is_shared_account_permission("file_reindex"));
        assert!(!is_shared_account_permission("group_create"));
        assert!(!is_shared_account_permission("group_delete"));
        assert!(!is_shared_account_permission("group_edit"));

        // Invalid permissions
        assert!(!is_shared_account_permission("invalid"));
        assert!(!is_shared_account_permission(""));
    }

    #[test]
    fn test_shared_account_permissions_sorted() {
        // Verify shared account permissions are in alphabetical order
        let mut sorted = SHARED_ACCOUNT_PERMISSIONS.to_vec();
        sorted.sort();
        assert_eq!(SHARED_ACCOUNT_PERMISSIONS, sorted.as_slice());
    }

    #[test]
    fn test_shared_account_permissions_subset() {
        // Verify all shared account permissions are valid permissions
        for perm in SHARED_ACCOUNT_PERMISSIONS {
            assert!(
                ALL_PERMISSIONS.contains(perm),
                "SHARED_ACCOUNT_PERMISSIONS contains '{}' which is not in ALL_PERMISSIONS",
                perm
            );
        }
    }

    #[test]
    fn test_all_permissions_sorted() {
        // Verify permissions are in alphabetical order
        let mut sorted = ALL_PERMISSIONS.to_vec();
        sorted.sort();
        assert_eq!(ALL_PERMISSIONS, sorted.as_slice());
    }

    #[test]
    fn test_all_permissions_no_duplicates() {
        // Verify no duplicate permissions
        let mut seen = std::collections::HashSet::new();
        for perm in ALL_PERMISSIONS {
            assert!(seen.insert(perm), "Duplicate permission: {}", perm);
        }
    }
}
