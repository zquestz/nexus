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
///
/// **Why no human-readable error field:** the publisher runs without
/// a request locale; the admin who reads the status may be in any of
/// 13 locales. Storing pre-formatted English here would force every
/// admin to read English. Instead the publisher sets only
/// [`Self::last_error_kind`] — a stable identifier from
/// `nexus_common::error_kind` — and the handler that composes
/// `TrackerInfo` translates it to the requesting admin's locale at
/// compose time. The raw underlying error (`io::Error`, `sqlx::Error`,
/// etc.) flows to operator logs via `warn!(err = %e, …)` instead.
#[derive(Debug, Clone, Default)]
pub struct TrackerStatus {
    /// Whether the task currently has a healthy registration with the
    /// tracker. `true` only between a successful refresh and the next
    /// failure (or admin teardown).
    pub connected: bool,
    /// Unix epoch seconds when the task last successfully registered.
    /// `None` if it's never connected since this task started.
    pub last_connected_at: Option<i64>,
    /// Unix epoch seconds when the task last started a connection
    /// attempt, regardless of outcome. Stamped at the top of each
    /// cycle (before the TCP dial), so the admin UI can show that the
    /// publisher is making forward progress even when every recent
    /// attempt has failed. `None` until the first attempt.
    pub last_attempted_at: Option<i64>,
    /// Most recent error kind in machine-readable form
    /// (e.g. `"tracker_fingerprint_mismatch"`, `"unauthorized"`).
    /// Translated to a human-readable message at handler compose time
    /// using the requesting admin's locale.
    pub last_error_kind: Option<String>,
    /// Fingerprint the task observed but the admin hasn't accepted
    /// yet (after a Stage 1 mismatch). `None` in normal operation.
    pub pending_fingerprint: Option<String>,
    /// Tracker-supplied refresh interval in seconds, populated once
    /// the task has successfully registered.
    pub refresh_interval: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_status_is_disconnected_clean() {
        let s = TrackerStatus::default();
        assert!(!s.connected);
        assert!(s.last_connected_at.is_none());
        assert!(s.last_attempted_at.is_none());
        assert!(s.last_error_kind.is_none());
        assert!(s.pending_fingerprint.is_none());
        assert!(s.refresh_interval.is_none());
    }
}
