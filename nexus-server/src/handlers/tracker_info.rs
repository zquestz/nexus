//! `TrackerInfo` composition — DB row + runtime status → wire struct.
//!
//! Lives separately from the per-handler files because two handlers
//! (`tracker_list`, `tracker_edit`) call it. Translation of the
//! publisher's error-kind enum into the admin's locale also happens
//! here, so the wire struct's `last_error` field is always
//! pre-translated by the time it reaches the client.

use nexus_common::protocol::TrackerInfo;
use nexus_common::{
    ERROR_KIND_CAPACITY, ERROR_KIND_INVALID, ERROR_KIND_RATE_LIMITED,
    ERROR_KIND_TRACKER_CONNECTION_FAILED, ERROR_KIND_TRACKER_CONNECTION_LOST,
    ERROR_KIND_TRACKER_DB_FAILED, ERROR_KIND_TRACKER_FINGERPRINT_INTERCEPTED,
    ERROR_KIND_TRACKER_FINGERPRINT_MISMATCH, ERROR_KIND_TRACKER_HANDSHAKE_FAILED,
    ERROR_KIND_TRACKER_TLS_FAILED, ERROR_KIND_UNAUTHORIZED,
};

use crate::db::TrackerRecord;
use crate::tracker::TrackerStatus;

/// Compose a `TrackerInfo` wire struct from a DB row + optional
/// runtime status, translating the publisher's `last_error_kind` into
/// the requesting admin's locale.
///
/// When `status` is `None` (disabled tracker, or no task running yet),
/// runtime fields default to "not connected" — `connected: false` and
/// every other runtime field `None`. That accurately reflects the
/// truth: no live publisher task, no connection state to report.
#[must_use]
pub fn compose_tracker_info(
    record: TrackerRecord,
    status: Option<TrackerStatus>,
    locale: &str,
) -> TrackerInfo {
    let s = status.unwrap_or_default();
    let last_error = s
        .last_error_kind
        .as_ref()
        .map(|kind| translate_tracker_error_kind(locale, kind));
    TrackerInfo {
        id: record.id,
        address: record.address,
        port: record.port,
        fingerprint: record.fingerprint,
        password: record.password,
        name: record.name,
        enabled: record.enabled,
        created_at: record.created_at,
        updated_at: record.updated_at,
        connected: s.connected,
        last_connected_at: s.last_connected_at,
        last_error,
        last_error_kind: s.last_error_kind,
        pending_fingerprint: s.pending_fingerprint,
        refresh_interval: s.refresh_interval,
    }
}

/// Translate a publisher error kind to a localized human-readable
/// message using the requesting admin's locale.
///
/// The kind itself is a stable identifier (from
/// `nexus_common::error_kind`). Known kinds map to a fixed i18n key;
/// anything else falls back to the generic `err-tracker-unknown`
/// (e.g. a future tracker version that returns an `error_kind` string
/// we haven't taught the BBS to translate yet).
fn translate_tracker_error_kind(locale: &str, kind: &str) -> String {
    let i18n_key = match kind {
        ERROR_KIND_TRACKER_CONNECTION_FAILED => "err-tracker-connection-failed",
        ERROR_KIND_TRACKER_TLS_FAILED => "err-tracker-tls-failed",
        ERROR_KIND_TRACKER_HANDSHAKE_FAILED => "err-tracker-handshake-failed",
        ERROR_KIND_TRACKER_CONNECTION_LOST => "err-tracker-connection-lost",
        ERROR_KIND_TRACKER_DB_FAILED => "err-tracker-db-failed",
        ERROR_KIND_TRACKER_FINGERPRINT_MISMATCH => "err-tracker-fingerprint-mismatch",
        ERROR_KIND_TRACKER_FINGERPRINT_INTERCEPTED => "err-tracker-fingerprint-intercepted",
        ERROR_KIND_UNAUTHORIZED => "err-tracker-unauthorized",
        ERROR_KIND_RATE_LIMITED => "err-tracker-rate-limited",
        ERROR_KIND_CAPACITY => "err-tracker-capacity",
        ERROR_KIND_INVALID => "err-tracker-invalid",
        _ => "err-tracker-unknown",
    };
    crate::i18n::t(locale, i18n_key)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> TrackerRecord {
        TrackerRecord {
            id: 7,
            address: "tracker.example.com".to_string(),
            port: 7510,
            fingerprint: Some("AA:BB".to_string()),
            password: Some("hunter2".to_string()),
            name: "Public".to_string(),
            enabled: true,
            created_at: 100,
            updated_at: 200,
        }
    }

    #[test]
    fn none_status_yields_disconnected_defaults() {
        let info = compose_tracker_info(record(), None, "en");
        assert_eq!(info.id, 7);
        assert_eq!(info.address, "tracker.example.com");
        assert_eq!(info.port, 7510);
        assert_eq!(info.fingerprint.as_deref(), Some("AA:BB"));
        assert_eq!(info.password.as_deref(), Some("hunter2"));
        assert_eq!(info.name, "Public");
        assert!(info.enabled);
        assert_eq!(info.created_at, 100);
        assert_eq!(info.updated_at, 200);
        assert!(!info.connected);
        assert!(info.last_connected_at.is_none());
        assert!(info.last_error.is_none());
        assert!(info.last_error_kind.is_none());
        assert!(info.pending_fingerprint.is_none());
        assert!(info.refresh_interval.is_none());
    }

    #[test]
    fn some_connected_status_propagates_runtime_fields() {
        let status = TrackerStatus {
            connected: true,
            last_connected_at: Some(1234),
            last_error_kind: None,
            pending_fingerprint: None,
            refresh_interval: Some(300),
        };
        let info = compose_tracker_info(record(), Some(status), "en");
        assert!(info.connected);
        assert_eq!(info.last_connected_at, Some(1234));
        assert_eq!(info.refresh_interval, Some(300));
        // No kind set → no localized message either.
        assert!(info.last_error.is_none());
        assert!(info.last_error_kind.is_none());
    }

    #[test]
    fn known_kind_is_translated_in_requested_locale() {
        let status = TrackerStatus {
            connected: false,
            last_connected_at: None,
            last_error_kind: Some(
                nexus_common::ERROR_KIND_TRACKER_FINGERPRINT_MISMATCH.to_string(),
            ),
            pending_fingerprint: Some("CC:DD".to_string()),
            refresh_interval: None,
        };
        let info_en = compose_tracker_info(record(), Some(status.clone()), "en");
        assert_eq!(
            info_en.last_error.as_deref(),
            Some("Tracker certificate does not match the pinned fingerprint")
        );
        assert_eq!(
            info_en.last_error_kind.as_deref(),
            Some(nexus_common::ERROR_KIND_TRACKER_FINGERPRINT_MISMATCH)
        );
        assert_eq!(info_en.pending_fingerprint.as_deref(), Some("CC:DD"));

        // Different locale → different translation; kind stays stable.
        let info_fr = compose_tracker_info(record(), Some(status), "fr");
        assert_ne!(info_fr.last_error, info_en.last_error);
        assert_eq!(info_fr.last_error_kind, info_en.last_error_kind);
    }

    #[test]
    fn unknown_kind_falls_back_to_unknown_template() {
        let status = TrackerStatus {
            connected: false,
            last_connected_at: None,
            last_error_kind: Some("not_a_real_kind".to_string()),
            pending_fingerprint: None,
            refresh_interval: None,
        };
        let info = compose_tracker_info(record(), Some(status), "en");
        // Whatever the en `err-tracker-unknown` text is, it should
        // appear here.
        let expected = crate::i18n::t("en", "err-tracker-unknown");
        assert_eq!(info.last_error.as_deref(), Some(expected.as_str()));
        // Kind passed through verbatim for the client to match on.
        assert_eq!(info.last_error_kind.as_deref(), Some("not_a_real_kind"));
    }
}
