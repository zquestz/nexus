//! Client-side tracker types
//!
//! A [`ClientTracker`] is a user-managed entry in the local config that
//! describes one tracker the client is willing to query for server discovery.
//! It is *not* a connection target in the BBS sense — it carries a tracker
//! registration password (a shared invite code, when the tracker is gated)
//! rather than a per-user login credential, and its TLS pin is bound to the
//! tracker's certificate, not a BBS server's.

use nexus_common::DEFAULT_TRACKER_PORT;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::fingerprint::deserialize_normalized_fingerprint;

fn default_tracker_port() -> u16 {
    DEFAULT_TRACKER_PORT
}

/// One configured tracker the client may query for the list of servers.
///
/// Persisted in `config.json` under `client_trackers`. Identity is the
/// stable `id`; address/port/etc. are mutable via the panel's Edit flow.
#[derive(Clone, Serialize, Deserialize)]
pub struct ClientTracker {
    /// Stable identifier for this tracker entry. Used everywhere the panel
    /// references a row (cache key, dropdown selection, edit/remove
    /// targeting) so reordering / deletion can't shift the identity.
    #[serde(default = "Uuid::new_v4")]
    pub id: Uuid,
    /// Display name for the tracker as shown in the dropdown and forms.
    pub name: String,
    /// Tracker hostname or IP. Stored as the user typed it (Option C: IDN
    /// stays Unicode in storage; Punycode encoding happens only at
    /// connection time, matching `ServerBookmark.address`).
    pub address: String,
    /// Tracker TCP port. Defaults to [`DEFAULT_TRACKER_PORT`] for hand-edited
    /// or pre-port configs.
    #[serde(default = "default_tracker_port")]
    pub port: u16,
    /// Optional tracker *registration password*. Shared invite code scoped
    /// to the tracker's listing role, not a personal login credential.
    /// `None` = open tracker; `Some("…")` = gated tracker.
    #[serde(default)]
    pub password: Option<String>,
    /// TOFU-pinned TLS certificate fingerprint. `None` until the first
    /// successful query commits the observed fingerprint. Subsequent
    /// queries Stage-1-check against this value; mismatch routes to the
    /// AcceptFingerprint dialog.
    ///
    /// Deserialized via [`deserialize_normalized_fingerprint`] so an empty
    /// value from a hand-edited config collapses to `None` rather than reaching
    /// Stage 1 with an empty `expected`.
    #[serde(default, deserialize_with = "deserialize_normalized_fingerprint")]
    pub certificate_fingerprint: Option<String>,
}

impl Default for ClientTracker {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            name: String::new(),
            address: String::new(),
            port: DEFAULT_TRACKER_PORT,
            password: None,
            certificate_fingerprint: None,
        }
    }
}

impl std::fmt::Debug for ClientTracker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientTracker")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("address", &self.address)
            .field("port", &self.port)
            .field(
                "password",
                &self.password.as_ref().map(|_| nexus_common::REDACTED),
            )
            .field("certificate_fingerprint", &self.certificate_fingerprint)
            .finish()
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_uses_default_tracker_port() {
        let t = ClientTracker::default();
        assert_eq!(t.port, DEFAULT_TRACKER_PORT);
        assert!(t.password.is_none());
        assert!(t.certificate_fingerprint.is_none());
    }

    #[test]
    fn test_debug_redacts_password() {
        let t = ClientTracker {
            password: Some("super-secret".to_string()),
            ..Default::default()
        };
        let debug = format!("{:?}", t);
        assert!(!debug.contains("super-secret"));
        assert!(debug.contains(nexus_common::REDACTED));
    }

    #[test]
    fn test_deserialize_normalizes_empty_fingerprint() {
        let json = r#"{
            "id": "00000000-0000-0000-0000-000000000001",
            "name": "T",
            "address": "tracker.example",
            "port": 7510,
            "certificate_fingerprint": ""
        }"#;
        let t: ClientTracker = serde_json::from_str(json).expect("deserialize");
        assert!(
            t.certificate_fingerprint.is_none(),
            "empty string fingerprint must collapse to None on load"
        );
    }

    #[test]
    fn test_deserialize_preserves_whitespace_fingerprint() {
        // Whitespace is preserved; only an empty string collapses to None.
        let json = r#"{
            "id": "00000000-0000-0000-0000-000000000001",
            "name": "T",
            "address": "tracker.example",
            "port": 7510,
            "certificate_fingerprint": "   "
        }"#;
        let t: ClientTracker = serde_json::from_str(json).expect("deserialize");
        assert_eq!(t.certificate_fingerprint, Some("   ".to_string()));
    }

    #[test]
    fn test_deserialize_default_port_when_missing() {
        // Hand-edited config without a port should default rather than fail.
        let json = r#"{
            "id": "00000000-0000-0000-0000-000000000001",
            "name": "T",
            "address": "tracker.example"
        }"#;
        let t: ClientTracker = serde_json::from_str(json).expect("deserialize");
        assert_eq!(t.port, DEFAULT_TRACKER_PORT);
    }

    #[test]
    fn test_deserialize_default_id_when_missing() {
        // Hand-edited config without an id should get a fresh UUID rather
        // than fail. (The Vec<ClientTracker> position then becomes the
        // de-facto identity until next save round-trips a real id.)
        let json = r#"{
            "name": "T",
            "address": "tracker.example",
            "port": 7510
        }"#;
        let t: ClientTracker = serde_json::from_str(json).expect("deserialize");
        assert_ne!(t.id, Uuid::nil());
    }

    #[test]
    fn test_serialize_roundtrip_preserves_pin() {
        let original = ClientTracker {
            id: Uuid::new_v4(),
            name: "Tracker One".to_string(),
            address: "tracker.example".to_string(),
            port: 7510,
            password: Some("invite".to_string()),
            certificate_fingerprint: Some(
                "AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99"
                    .to_string(),
            ),
        };
        let json = serde_json::to_string(&original).expect("serialize");
        let parsed: ClientTracker = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.id, original.id);
        assert_eq!(parsed.name, original.name);
        assert_eq!(parsed.address, original.address);
        assert_eq!(parsed.port, original.port);
        assert_eq!(parsed.password, original.password);
        assert_eq!(
            parsed.certificate_fingerprint,
            original.certificate_fingerprint
        );
    }
}
