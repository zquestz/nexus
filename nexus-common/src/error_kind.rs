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

// ---- Tracker registration kinds ----
//
// These describe internal states of a BBS server's per-tracker
// registration task. They never appear in tracker protocol responses
// (the tracker uses the generic kinds above for those); they're set
// by the tracker task itself and surfaced to the BBS admin via
// `TrackerInfo.last_error_kind`. The matching i18n keys live in the
// BBS server's `errors.ftl` as `err-tracker-{kind}` and are looked
// up at compose-time using the requesting admin's locale.

/// Tracker task couldn't open the TCP connection to the tracker.
pub const ERROR_KIND_TRACKER_CONNECTION_FAILED: &str = "tracker_connection_failed";

/// Tracker task's TLS handshake with the tracker failed.
pub const ERROR_KIND_TRACKER_TLS_FAILED: &str = "tracker_tls_failed";

/// Tracker task's BBS-style handshake with the tracker failed (the
/// `Handshake` send/receive cycle, distinct from the TCP+TLS phase).
pub const ERROR_KIND_TRACKER_HANDSHAKE_FAILED: &str = "tracker_handshake_failed";

/// Tracker task's connection to the tracker was lost mid-session
/// (peer closed, read error, register-response timeout, etc.).
pub const ERROR_KIND_TRACKER_CONNECTION_LOST: &str = "tracker_connection_lost";

/// Tracker task hit a database error while updating local tracker state
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

/// Tracker sent an `error_kind` that violates the wire format
/// (non-snake_case, control chars, oversized, etc.). Distinct from
/// `protocol_error` so admin UI can attribute the violation to the
/// tracker rather than to a generic protocol mismatch. Unrecoverable;
/// the tracker is broken or malicious.
pub const ERROR_KIND_TRACKER_PROTOCOL_ERROR: &str = "tracker_protocol_error";

/// Tracker row's `address` field can't be resolved into a form the
/// system resolver / rustls accept (IDNA failure or empty string).
/// The validator at `TrackerCreate`/`TrackerUpdate` should have
/// already rejected this, so reaching this kind means the row is
/// structurally broken — admin must edit it. Unrecoverable.
pub const ERROR_KIND_TRACKER_ADDRESS_INVALID: &str = "tracker_address_invalid";

/// Whether `s` is a well-formed `error_kind` per the wire-format
/// rule that all canonical kinds follow: ASCII snake_case starting
/// with a lowercase letter, bounded by [`MAX_ERROR_KIND_LENGTH`].
///
/// Used at trust boundaries — primarily where a remote peer's
/// `error_kind` would otherwise flow verbatim into a status field
/// that is itself wire-visible (e.g. the BBS tracker task storing a
/// tracker-supplied kind in `TrackerInfo.last_error_kind`). Junk
/// kinds (control chars, embedded JSON, multi-line garbage) are
/// rejected so they can't be smuggled into our wire surface.
///
/// [`MAX_ERROR_KIND_LENGTH`]: crate::validators::MAX_ERROR_KIND_LENGTH
#[must_use]
pub fn is_valid_error_kind(s: &str) -> bool {
    if s.is_empty() || s.len() > crate::validators::MAX_ERROR_KIND_LENGTH {
        return false;
    }
    let mut bytes = s.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    if !first.is_ascii_lowercase() {
        return false;
    }
    bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// Whether an `error_kind` from the tracker (or our tracker task's
/// internal categorization) is unrecoverable — i.e. the tracker task
/// can't recover without external intervention (admin updating the
/// row, accepting a fingerprint, or the server restarting). Used by
/// both the tracker task itself (to decide whether to exit vs. retry)
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
        // Tracker shipped a malformed kind — the daemon is broken
        // or hostile; retrying isn't going to help.
        | ERROR_KIND_TRACKER_PROTOCOL_ERROR
        // The tracker row's address can't be resolved; admin must edit.
        | ERROR_KIND_TRACKER_ADDRESS_INVALID
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

    // ---- Tracker registration kinds ----
    //
    // Internal states of a BBS server's per-tracker registration task
    // (set by the tracker task itself, not echoed from the tracker
    // protocol). Surfaced to the BBS admin via
    // `TrackerInfo.last_error_kind`. The client-side tracker
    // management UI matches on these to render appropriate per-kind
    // affordances (e.g. "Accept new fingerprint?" for
    // `TrackerFingerprintMismatch`).
    /// Tracker task couldn't open the TCP connection to the tracker.
    TrackerConnectionFailed,

    /// Tracker task's TLS handshake with the tracker failed.
    TrackerTlsFailed,

    /// Tracker task's BBS-style handshake with the tracker failed
    /// (the `Handshake` send/receive cycle, distinct from TCP+TLS).
    TrackerHandshakeFailed,

    /// Tracker task's connection to the tracker was lost mid-session
    /// (peer closed, read error, register-response timeout, etc.).
    TrackerConnectionLost,

    /// Tracker task hit a database error while updating local tracker
    /// state (e.g. TOFU-pinning the fingerprint).
    TrackerDbFailed,

    /// Stage 1 fingerprint mismatch: tracker's TLS cert disagrees with
    /// the row's pinned fingerprint. Unrecoverable; admin must accept
    /// the new fingerprint.
    TrackerFingerprintMismatch,

    /// Stage 2 fingerprint mismatch: tracker's TLS-observed cert
    /// disagrees with what the tracker self-reports in
    /// `HandshakeResponse`. Unrecoverable; suggests active
    /// interception.
    TrackerFingerprintIntercepted,

    /// Tracker sent an `error_kind` that violates the wire format
    /// (non-snake_case, control chars, oversized, etc.). Distinct from
    /// `ProtocolError` so admin UI can attribute the violation to the
    /// tracker rather than to a generic protocol mismatch.
    TrackerProtocolError,

    /// Tracker row's `address` field can't be resolved into a form the
    /// system resolver / rustls accept (IDNA failure or empty string).
    /// Reaching this kind means the row is structurally broken — admin
    /// must edit it.
    TrackerAddressInvalid,
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
            Self::TrackerConnectionFailed => ERROR_KIND_TRACKER_CONNECTION_FAILED,
            Self::TrackerTlsFailed => ERROR_KIND_TRACKER_TLS_FAILED,
            Self::TrackerHandshakeFailed => ERROR_KIND_TRACKER_HANDSHAKE_FAILED,
            Self::TrackerConnectionLost => ERROR_KIND_TRACKER_CONNECTION_LOST,
            Self::TrackerDbFailed => ERROR_KIND_TRACKER_DB_FAILED,
            Self::TrackerFingerprintMismatch => ERROR_KIND_TRACKER_FINGERPRINT_MISMATCH,
            Self::TrackerFingerprintIntercepted => ERROR_KIND_TRACKER_FINGERPRINT_INTERCEPTED,
            Self::TrackerProtocolError => ERROR_KIND_TRACKER_PROTOCOL_ERROR,
            Self::TrackerAddressInvalid => ERROR_KIND_TRACKER_ADDRESS_INVALID,
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
            ERROR_KIND_TRACKER_CONNECTION_FAILED => Some(Self::TrackerConnectionFailed),
            ERROR_KIND_TRACKER_TLS_FAILED => Some(Self::TrackerTlsFailed),
            ERROR_KIND_TRACKER_HANDSHAKE_FAILED => Some(Self::TrackerHandshakeFailed),
            ERROR_KIND_TRACKER_CONNECTION_LOST => Some(Self::TrackerConnectionLost),
            ERROR_KIND_TRACKER_DB_FAILED => Some(Self::TrackerDbFailed),
            ERROR_KIND_TRACKER_FINGERPRINT_MISMATCH => Some(Self::TrackerFingerprintMismatch),
            ERROR_KIND_TRACKER_FINGERPRINT_INTERCEPTED => Some(Self::TrackerFingerprintIntercepted),
            ERROR_KIND_TRACKER_PROTOCOL_ERROR => Some(Self::TrackerProtocolError),
            ERROR_KIND_TRACKER_ADDRESS_INVALID => Some(Self::TrackerAddressInvalid),
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
        assert_eq!(
            ErrorKind::TrackerConnectionFailed.as_str(),
            "tracker_connection_failed"
        );
        assert_eq!(ErrorKind::TrackerTlsFailed.as_str(), "tracker_tls_failed");
        assert_eq!(
            ErrorKind::TrackerHandshakeFailed.as_str(),
            "tracker_handshake_failed"
        );
        assert_eq!(
            ErrorKind::TrackerConnectionLost.as_str(),
            "tracker_connection_lost"
        );
        assert_eq!(ErrorKind::TrackerDbFailed.as_str(), "tracker_db_failed");
        assert_eq!(
            ErrorKind::TrackerFingerprintMismatch.as_str(),
            "tracker_fingerprint_mismatch"
        );
        assert_eq!(
            ErrorKind::TrackerFingerprintIntercepted.as_str(),
            "tracker_fingerprint_intercepted"
        );
        assert_eq!(
            ErrorKind::TrackerProtocolError.as_str(),
            "tracker_protocol_error"
        );
        assert_eq!(
            ErrorKind::TrackerAddressInvalid.as_str(),
            "tracker_address_invalid"
        );
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
        assert_eq!(
            ErrorKind::parse("tracker_connection_failed"),
            Some(ErrorKind::TrackerConnectionFailed)
        );
        assert_eq!(
            ErrorKind::parse("tracker_tls_failed"),
            Some(ErrorKind::TrackerTlsFailed)
        );
        assert_eq!(
            ErrorKind::parse("tracker_handshake_failed"),
            Some(ErrorKind::TrackerHandshakeFailed)
        );
        assert_eq!(
            ErrorKind::parse("tracker_connection_lost"),
            Some(ErrorKind::TrackerConnectionLost)
        );
        assert_eq!(
            ErrorKind::parse("tracker_db_failed"),
            Some(ErrorKind::TrackerDbFailed)
        );
        assert_eq!(
            ErrorKind::parse("tracker_fingerprint_mismatch"),
            Some(ErrorKind::TrackerFingerprintMismatch)
        );
        assert_eq!(
            ErrorKind::parse("tracker_fingerprint_intercepted"),
            Some(ErrorKind::TrackerFingerprintIntercepted)
        );
        assert_eq!(
            ErrorKind::parse("tracker_protocol_error"),
            Some(ErrorKind::TrackerProtocolError)
        );
        assert_eq!(
            ErrorKind::parse("tracker_address_invalid"),
            Some(ErrorKind::TrackerAddressInvalid)
        );
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
            ErrorKind::TrackerConnectionFailed,
            ErrorKind::TrackerTlsFailed,
            ErrorKind::TrackerHandshakeFailed,
            ErrorKind::TrackerConnectionLost,
            ErrorKind::TrackerDbFailed,
            ErrorKind::TrackerFingerprintMismatch,
            ErrorKind::TrackerFingerprintIntercepted,
            ErrorKind::TrackerProtocolError,
            ErrorKind::TrackerAddressInvalid,
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
        assert!(is_unrecoverable_error_kind(
            ERROR_KIND_TRACKER_PROTOCOL_ERROR
        ));
        assert!(is_unrecoverable_error_kind(
            ERROR_KIND_TRACKER_ADDRESS_INVALID
        ));
    }

    #[test]
    fn is_valid_error_kind_accepts_canonical_kinds() {
        // Every kind we ourselves emit must pass the wire-format check.
        for kind in [
            ERROR_KIND_EXISTS,
            ERROR_KIND_NOT_FOUND,
            ERROR_KIND_PERMISSION,
            ERROR_KIND_INVALID_PATH,
            ERROR_KIND_INVALID,
            ERROR_KIND_IO_ERROR,
            ERROR_KIND_PROTOCOL_ERROR,
            ERROR_KIND_HASH_MISMATCH,
            ERROR_KIND_CONFLICT,
            ERROR_KIND_UNAUTHORIZED,
            ERROR_KIND_RATE_LIMITED,
            ERROR_KIND_CAPACITY,
            ERROR_KIND_TRACKER_CONNECTION_FAILED,
            ERROR_KIND_TRACKER_TLS_FAILED,
            ERROR_KIND_TRACKER_HANDSHAKE_FAILED,
            ERROR_KIND_TRACKER_CONNECTION_LOST,
            ERROR_KIND_TRACKER_DB_FAILED,
            ERROR_KIND_TRACKER_FINGERPRINT_MISMATCH,
            ERROR_KIND_TRACKER_FINGERPRINT_INTERCEPTED,
            ERROR_KIND_TRACKER_PROTOCOL_ERROR,
            ERROR_KIND_TRACKER_ADDRESS_INVALID,
        ] {
            assert!(is_valid_error_kind(kind), "rejected canonical {kind:?}");
        }
    }

    #[test]
    fn is_valid_error_kind_rejects_junk() {
        // Empty.
        assert!(!is_valid_error_kind(""));
        // Leading non-letter.
        assert!(!is_valid_error_kind("_leading_underscore"));
        assert!(!is_valid_error_kind("9starts_with_digit"));
        // Uppercase / mixed case.
        assert!(!is_valid_error_kind("Uppercase"));
        assert!(!is_valid_error_kind("camelCase"));
        // Hyphens / dots / spaces.
        assert!(!is_valid_error_kind("kebab-case"));
        assert!(!is_valid_error_kind("dot.notation"));
        assert!(!is_valid_error_kind("with space"));
        // Control / multibyte / injection attempts.
        assert!(!is_valid_error_kind("line\nbreak"));
        assert!(!is_valid_error_kind("nul\0byte"));
        assert!(!is_valid_error_kind("héllo"));
        assert!(!is_valid_error_kind("<script>"));
        // Too long (33 chars).
        assert!(!is_valid_error_kind(&"a".repeat(33)));
        // Exactly at the bound is OK.
        assert!(is_valid_error_kind(&"a".repeat(32)));
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
        assert_eq!(
            ERROR_KIND_TRACKER_CONNECTION_FAILED,
            ErrorKind::TrackerConnectionFailed.as_str()
        );
        assert_eq!(
            ERROR_KIND_TRACKER_TLS_FAILED,
            ErrorKind::TrackerTlsFailed.as_str()
        );
        assert_eq!(
            ERROR_KIND_TRACKER_HANDSHAKE_FAILED,
            ErrorKind::TrackerHandshakeFailed.as_str()
        );
        assert_eq!(
            ERROR_KIND_TRACKER_CONNECTION_LOST,
            ErrorKind::TrackerConnectionLost.as_str()
        );
        assert_eq!(
            ERROR_KIND_TRACKER_DB_FAILED,
            ErrorKind::TrackerDbFailed.as_str()
        );
        assert_eq!(
            ERROR_KIND_TRACKER_FINGERPRINT_MISMATCH,
            ErrorKind::TrackerFingerprintMismatch.as_str()
        );
        assert_eq!(
            ERROR_KIND_TRACKER_FINGERPRINT_INTERCEPTED,
            ErrorKind::TrackerFingerprintIntercepted.as_str()
        );
        assert_eq!(
            ERROR_KIND_TRACKER_PROTOCOL_ERROR,
            ErrorKind::TrackerProtocolError.as_str()
        );
        assert_eq!(
            ERROR_KIND_TRACKER_ADDRESS_INVALID,
            ErrorKind::TrackerAddressInvalid.as_str()
        );
    }
}
