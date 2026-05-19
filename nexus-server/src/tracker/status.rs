//! Per-tracker runtime status tracked by the tracker manager.

/// Runtime state of a single tracker task.
///
/// Updated by the task across connect → handshake → register → refresh;
/// read by admin handlers via [`super::manager::TrackerManager::status_for`]
/// to compose `TrackerInfo`. Ephemeral — not persisted; recomputed after
/// restart.
///
/// **Why no human-readable error field:** the task has no request locale,
/// but the reading admin may be in any of 13. So the task stores only a
/// stable [`Self::last_error_kind`] (from `nexus_common::error_kind`) and
/// the handler translates it to the admin's locale at compose time. The
/// raw error flows to operator logs via `warn!(err = %e, …)`.
#[derive(Debug, Clone, Default)]
pub struct TrackerStatus {
    /// Healthy registration — `true` only between a successful refresh and
    /// the next failure (or admin teardown).
    pub connected: bool,
    /// Unix epoch secs of last successful register; `None` until first.
    pub last_connected_at: Option<i64>,
    /// Unix epoch secs of last attempt (any outcome), stamped before the
    /// TCP dial so the UI shows forward progress even while failing.
    pub last_attempted_at: Option<i64>,
    /// Machine-readable error kind (e.g. `"tracker_fingerprint_mismatch"`),
    /// translated to the admin's locale at handler compose time.
    pub last_error_kind: Option<String>,
    /// Fingerprint observed but not yet accepted (after a Stage 1
    /// mismatch); `None` in normal operation.
    pub pending_fingerprint: Option<String>,
    /// Tracker-supplied refresh interval (secs), set once registered.
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
