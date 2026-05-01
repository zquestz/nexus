//! Machine-readable error kinds for protocol messages
//!
//! These error kinds are serialized to strings in protocol messages,
//! allowing clients to make decisions based on the error type
//! (e.g., showing an overwrite dialog for "exists" errors).

use std::fmt;

// =============================================================================
// String Constants
// =============================================================================

/// Error kind string: destination already exists
pub const ERROR_KIND_EXISTS: &str = "exists";

/// Error kind string: source not found
pub const ERROR_KIND_NOT_FOUND: &str = "not_found";

/// Error kind string: permission denied
pub const ERROR_KIND_PERMISSION: &str = "permission";

/// Error kind string: invalid path
pub const ERROR_KIND_INVALID_PATH: &str = "invalid_path";

/// Error kind string: invalid input (generic validation failure)
pub const ERROR_KIND_INVALID: &str = "invalid";

/// Error kind string: I/O error (disk full, read/write failure)
pub const ERROR_KIND_IO_ERROR: &str = "io_error";

/// Error kind string: protocol error (unexpected message type, malformed data)
pub const ERROR_KIND_PROTOCOL_ERROR: &str = "protocol_error";

/// Error kind string: hash mismatch (SHA-256 verification failed)
pub const ERROR_KIND_HASH_MISMATCH: &str = "hash_mismatch";

/// Error kind string: upload conflict (another upload to same file in progress)
pub const ERROR_KIND_CONFLICT: &str = "conflict";

/// Error kind string: authentication failed (wrong or missing credentials)
pub const ERROR_KIND_UNAUTHORIZED: &str = "unauthorized";

/// Error kind string: rate limit exceeded
pub const ERROR_KIND_RATE_LIMITED: &str = "rate_limited";

/// Error kind string: service is at capacity
pub const ERROR_KIND_CAPACITY: &str = "capacity";

// ---- Tracker publisher kinds ----
//
// These describe internal states of a BBS server's per-tracker
// publisher task. They never appear in tracker protocol responses
// (the tracker uses the generic kinds above for those); they're set
// by the publisher itself and surfaced to the BBS admin via
// `TrackerInfo.last_error_kind`. The matching i18n keys live in the
// BBS server's `errors.ftl` as `err-tracker-{kind}` and are looked
// up at compose-time using the requesting admin's locale.

/// Publisher couldn't open the TCP connection to the tracker.
pub const ERROR_KIND_TRACKER_CONNECTION_FAILED: &str = "tracker_connection_failed";

/// Publisher's TLS handshake with the tracker failed.
pub const ERROR_KIND_TRACKER_TLS_FAILED: &str = "tracker_tls_failed";

/// Publisher's BBS-style handshake with the tracker failed (the
/// `Handshake` send/receive cycle, distinct from the TCP+TLS phase).
pub const ERROR_KIND_TRACKER_HANDSHAKE_FAILED: &str = "tracker_handshake_failed";

/// Publisher's connection to the tracker was lost mid-session
/// (peer closed, read error, register-response timeout, etc.).
pub const ERROR_KIND_TRACKER_CONNECTION_LOST: &str = "tracker_connection_lost";

/// Publisher hit a database error while updating local tracker state
/// (e.g. TOFU-pinning the fingerprint).
pub const ERROR_KIND_TRACKER_DB_FAILED: &str = "tracker_db_failed";

/// Stage 1 fingerprint mismatch: tracker's TLS cert disagrees with
/// the row's pinned fingerprint. Unrecoverable; admin must accept
/// the new fingerprint.
pub const ERROR_KIND_TRACKER_FINGERPRINT_MISMATCH: &str = "tracker_fingerprint_mismatch";

/// Stage 2 fingerprint mismatch: tracker's TLS-observed cert disagrees
/// with what the tracker self-reports in `HandshakeResponse`.
/// Unrecoverable; suggests active interception.
pub const ERROR_KIND_TRACKER_FINGERPRINT_INTERCEPTED: &str = "tracker_fingerprint_intercepted";

/// Whether an `error_kind` from the tracker (or our publisher's
/// internal categorization) is unrecoverable — i.e. the publisher
/// can't recover without external intervention (admin updating the
/// row, accepting a fingerprint, or the server restarting). Used by
/// both the publisher itself (to decide whether to exit vs. retry)
/// and the client UI (to render "needs your attention").
#[must_use]
pub fn is_unrecoverable_error_kind(kind: &str) -> bool {
    matches!(
        kind,
        // Stage 1 / Stage 2 fingerprint failures.
        ERROR_KIND_TRACKER_FINGERPRINT_MISMATCH
        | ERROR_KIND_TRACKER_FINGERPRINT_INTERCEPTED
        // Wrong / missing registration password.
        | ERROR_KIND_UNAUTHORIZED
        // Field validation rejection (malformed address etc.).
        | ERROR_KIND_INVALID
    )
}

// =============================================================================
// Enum
// =============================================================================

/// Error kinds for protocol messages
///
/// These are returned in various responses to help clients
/// decide how to handle the error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// Destination file/directory already exists
    ///
    /// Client may offer to retry with `overwrite: true`
    /// (if user has `file_delete` permission).
    Exists,

    /// Source file/directory not found
    ///
    /// Client should clear clipboard if this was a move/copy operation.
    NotFound,

    /// Permission denied
    ///
    /// User lacks required permission for the operation.
    Permission,

    /// Invalid path
    ///
    /// Path contains invalid characters, traversal attempts,
    /// or the operation is not allowed (e.g., copying file to itself).
    InvalidPath,

    /// Invalid input (generic validation failure)
    ///
    /// The request contained invalid data that failed validation.
    Invalid,

    /// I/O error
    ///
    /// A filesystem operation failed (disk full, read/write error, etc.).
    IoError,

    /// Protocol error
    ///
    /// The client sent an unexpected message type or malformed data.
    ProtocolError,

    /// Hash mismatch
    ///
    /// SHA-256 verification failed after file transfer.
    HashMismatch,

    /// Upload conflict
    ///
    /// Another upload to the same file is already in progress.
    Conflict,

    /// Authentication failed
    ///
    /// Wrong or missing credentials. Used by the tracker for
    /// password-gated registration or listing flows.
    Unauthorized,

    /// Request rejected due to rate limiting
    ///
    /// The peer has exceeded a per-IP rate limit. Used by the
    /// tracker for failed-auth, connection, and listing rate caps.
    RateLimited,

    /// Service is at capacity
    ///
    /// The tracker has reached its configured entry limit and cannot
    /// accept new registrations.
    Capacity,
}

impl ErrorKind {
    /// Convert to the string representation used in protocol messages
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Exists => ERROR_KIND_EXISTS,
            Self::NotFound => ERROR_KIND_NOT_FOUND,
            Self::Permission => ERROR_KIND_PERMISSION,
            Self::InvalidPath => ERROR_KIND_INVALID_PATH,
            Self::Invalid => ERROR_KIND_INVALID,
            Self::IoError => ERROR_KIND_IO_ERROR,
            Self::ProtocolError => ERROR_KIND_PROTOCOL_ERROR,
            Self::HashMismatch => ERROR_KIND_HASH_MISMATCH,
            Self::Conflict => ERROR_KIND_CONFLICT,
            Self::Unauthorized => ERROR_KIND_UNAUTHORIZED,
            Self::RateLimited => ERROR_KIND_RATE_LIMITED,
            Self::Capacity => ERROR_KIND_CAPACITY,
        }
    }

    /// Parse from string (for client-side handling)
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            ERROR_KIND_EXISTS => Some(Self::Exists),
            ERROR_KIND_NOT_FOUND => Some(Self::NotFound),
            ERROR_KIND_PERMISSION => Some(Self::Permission),
            ERROR_KIND_INVALID_PATH => Some(Self::InvalidPath),
            ERROR_KIND_INVALID => Some(Self::Invalid),
            ERROR_KIND_IO_ERROR => Some(Self::IoError),
            ERROR_KIND_PROTOCOL_ERROR => Some(Self::ProtocolError),
            ERROR_KIND_HASH_MISMATCH => Some(Self::HashMismatch),
            ERROR_KIND_CONFLICT => Some(Self::Conflict),
            ERROR_KIND_UNAUTHORIZED => Some(Self::Unauthorized),
            ERROR_KIND_RATE_LIMITED => Some(Self::RateLimited),
            ERROR_KIND_CAPACITY => Some(Self::Capacity),
            _ => None,
        }
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<ErrorKind> for String {
    fn from(kind: ErrorKind) -> Self {
        kind.as_str().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_as_str() {
        assert_eq!(ErrorKind::Exists.as_str(), "exists");
        assert_eq!(ErrorKind::NotFound.as_str(), "not_found");
        assert_eq!(ErrorKind::Permission.as_str(), "permission");
        assert_eq!(ErrorKind::InvalidPath.as_str(), "invalid_path");
        assert_eq!(ErrorKind::Invalid.as_str(), "invalid");
        assert_eq!(ErrorKind::IoError.as_str(), "io_error");
        assert_eq!(ErrorKind::ProtocolError.as_str(), "protocol_error");
        assert_eq!(ErrorKind::HashMismatch.as_str(), "hash_mismatch");
        assert_eq!(ErrorKind::Conflict.as_str(), "conflict");
        assert_eq!(ErrorKind::Unauthorized.as_str(), "unauthorized");
        assert_eq!(ErrorKind::RateLimited.as_str(), "rate_limited");
        assert_eq!(ErrorKind::Capacity.as_str(), "capacity");
    }

    #[test]
    fn test_parse() {
        assert_eq!(ErrorKind::parse("exists"), Some(ErrorKind::Exists));
        assert_eq!(ErrorKind::parse("not_found"), Some(ErrorKind::NotFound));
        assert_eq!(ErrorKind::parse("permission"), Some(ErrorKind::Permission));
        assert_eq!(
            ErrorKind::parse("invalid_path"),
            Some(ErrorKind::InvalidPath)
        );
        assert_eq!(ErrorKind::parse("invalid"), Some(ErrorKind::Invalid));
        assert_eq!(ErrorKind::parse("io_error"), Some(ErrorKind::IoError));
        assert_eq!(
            ErrorKind::parse("protocol_error"),
            Some(ErrorKind::ProtocolError)
        );
        assert_eq!(
            ErrorKind::parse("hash_mismatch"),
            Some(ErrorKind::HashMismatch)
        );
        assert_eq!(ErrorKind::parse("conflict"), Some(ErrorKind::Conflict));
        assert_eq!(
            ErrorKind::parse("unauthorized"),
            Some(ErrorKind::Unauthorized)
        );
        assert_eq!(
            ErrorKind::parse("rate_limited"),
            Some(ErrorKind::RateLimited)
        );
        assert_eq!(ErrorKind::parse("capacity"), Some(ErrorKind::Capacity));
        assert_eq!(ErrorKind::parse("unknown"), None);
        assert_eq!(ErrorKind::parse(""), None);
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", ErrorKind::Exists), "exists");
        assert_eq!(format!("{}", ErrorKind::NotFound), "not_found");
        assert_eq!(format!("{}", ErrorKind::IoError), "io_error");
        assert_eq!(format!("{}", ErrorKind::HashMismatch), "hash_mismatch");
    }

    #[test]
    fn test_into_string() {
        let s: String = ErrorKind::Exists.into();
        assert_eq!(s, "exists");
    }

    #[test]
    fn test_roundtrip() {
        for kind in [
            ErrorKind::Exists,
            ErrorKind::NotFound,
            ErrorKind::Permission,
            ErrorKind::InvalidPath,
            ErrorKind::Invalid,
            ErrorKind::IoError,
            ErrorKind::ProtocolError,
            ErrorKind::HashMismatch,
            ErrorKind::Conflict,
            ErrorKind::Unauthorized,
            ErrorKind::RateLimited,
            ErrorKind::Capacity,
        ] {
            assert_eq!(ErrorKind::parse(kind.as_str()), Some(kind));
        }
    }

    #[test]
    fn test_unrecoverable_tracker_error_kinds() {
        assert!(is_unrecoverable_error_kind(
            ERROR_KIND_TRACKER_FINGERPRINT_MISMATCH
        ));
        assert!(is_unrecoverable_error_kind(
            ERROR_KIND_TRACKER_FINGERPRINT_INTERCEPTED
        ));
        assert!(is_unrecoverable_error_kind(ERROR_KIND_UNAUTHORIZED));
        assert!(is_unrecoverable_error_kind(ERROR_KIND_INVALID));
    }

    #[test]
    fn test_recoverable_tracker_error_kinds() {
        assert!(!is_unrecoverable_error_kind(ERROR_KIND_RATE_LIMITED));
        assert!(!is_unrecoverable_error_kind(ERROR_KIND_CAPACITY));
        assert!(!is_unrecoverable_error_kind(
            ERROR_KIND_TRACKER_CONNECTION_FAILED
        ));
        assert!(!is_unrecoverable_error_kind(ERROR_KIND_TRACKER_TLS_FAILED));
        assert!(!is_unrecoverable_error_kind(
            ERROR_KIND_TRACKER_HANDSHAKE_FAILED
        ));
        assert!(!is_unrecoverable_error_kind(
            ERROR_KIND_TRACKER_CONNECTION_LOST
        ));
        assert!(!is_unrecoverable_error_kind(ERROR_KIND_TRACKER_DB_FAILED));
        assert!(!is_unrecoverable_error_kind(""));
    }

    #[test]
    fn test_constants_match_enum() {
        // Ensure constants are in sync with enum
        assert_eq!(ERROR_KIND_EXISTS, ErrorKind::Exists.as_str());
        assert_eq!(ERROR_KIND_NOT_FOUND, ErrorKind::NotFound.as_str());
        assert_eq!(ERROR_KIND_PERMISSION, ErrorKind::Permission.as_str());
        assert_eq!(ERROR_KIND_INVALID_PATH, ErrorKind::InvalidPath.as_str());
        assert_eq!(ERROR_KIND_INVALID, ErrorKind::Invalid.as_str());
        assert_eq!(ERROR_KIND_IO_ERROR, ErrorKind::IoError.as_str());
        assert_eq!(ERROR_KIND_PROTOCOL_ERROR, ErrorKind::ProtocolError.as_str());
        assert_eq!(ERROR_KIND_HASH_MISMATCH, ErrorKind::HashMismatch.as_str());
        assert_eq!(ERROR_KIND_CONFLICT, ErrorKind::Conflict.as_str());
        assert_eq!(ERROR_KIND_UNAUTHORIZED, ErrorKind::Unauthorized.as_str());
        assert_eq!(ERROR_KIND_RATE_LIMITED, ErrorKind::RateLimited.as_str());
        assert_eq!(ERROR_KIND_CAPACITY, ErrorKind::Capacity.as_str());
    }
}
