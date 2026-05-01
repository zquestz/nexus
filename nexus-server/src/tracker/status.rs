//! Per-tracker runtime status tracked by the publisher manager.

/// Runtime state of a single publisher task.
///
/// Updated by the task as it progresses through connect → handshake →
/// register → refresh, and read by admin handlers via
/// [`super::manager::TrackerManager::status_for`] to compose
/// `TrackerInfo` responses.
///
/// The data here is ephemeral — not persisted to the DB. After a
/// server restart, each task reconnects and recomputes its status.
#[derive(Debug, Clone, Default)]
pub struct TrackerStatus {
    /// Whether the task currently has a healthy registration with the
    /// tracker. `true` only between a successful refresh and the next
    /// failure (or admin teardown).
    pub connected: bool,
    /// Unix epoch seconds when the task last successfully registered.
    /// `None` if it's never connected since this task started.
    pub last_connected_at: Option<i64>,
    /// Most recent error message, locale-translated by the tracker.
    pub last_error: Option<String>,
    /// Most recent error kind in machine-readable form
    /// (e.g. `"fingerprint_mismatch"`, `"unauthorized"`).
    pub last_error_kind: Option<String>,
    /// Fingerprint the task observed but the admin hasn't accepted
    /// yet (after a Stage 1 mismatch). `None` in normal operation.
    pub pending_fingerprint: Option<String>,
    /// Tracker-supplied refresh interval in seconds, populated once
    /// the task has successfully registered.
    pub refresh_interval: Option<u32>,
}

/// Whether an `error_kind` from the tracker (or our task's internal
/// categorization) is unrecoverable — i.e. the task can't recover
/// without external intervention (admin updating the row, or server
/// restart). Used by both the task itself (to decide whether to exit
/// vs. retry) and the client UI (to render "needs your attention").
#[must_use]
pub fn is_unrecoverable_error_kind(kind: &str) -> bool {
    matches!(
        kind,
        // Stage 1 fingerprint mismatch (pinned ≠ TLS-observed).
        "fingerprint_mismatch"
        // Stage 2 fingerprint mismatch (TLS-observed ≠ server-reported).
        | "fingerprint_intercepted"
        // Wrong / missing registration password.
        | "unauthorized"
        // Field validation rejection (malformed address etc.).
        | "invalid"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unrecoverable_error_kinds() {
        assert!(is_unrecoverable_error_kind("fingerprint_mismatch"));
        assert!(is_unrecoverable_error_kind("fingerprint_intercepted"));
        assert!(is_unrecoverable_error_kind("unauthorized"));
        assert!(is_unrecoverable_error_kind("invalid"));
    }

    #[test]
    fn recoverable_error_kinds() {
        // Transient / retryable.
        assert!(!is_unrecoverable_error_kind("rate_limited"));
        assert!(!is_unrecoverable_error_kind("capacity"));
        assert!(!is_unrecoverable_error_kind("connection_failed"));
        assert!(!is_unrecoverable_error_kind("tls_failed"));
        assert!(!is_unrecoverable_error_kind("handshake_failed"));
        assert!(!is_unrecoverable_error_kind(""));
    }

    #[test]
    fn default_status_is_disconnected_clean() {
        let s = TrackerStatus::default();
        assert!(!s.connected);
        assert!(s.last_connected_at.is_none());
        assert!(s.last_error.is_none());
        assert!(s.last_error_kind.is_none());
        assert!(s.pending_fingerprint.is_none());
        assert!(s.refresh_interval.is_none());
    }
}
