//! Tracker protocol types
//!
//! Message types for the Nexus tracker discovery protocol. Versioned
//! independently of the BBS protocol — see [`crate::TRACKER_PROTOCOL_VERSION`].
//!
//! Tracker connections share the BBS handshake ([`crate::protocol::ClientMessage::Handshake`]
//! and [`crate::protocol::ServerMessage::HandshakeResponse`]) and frame format,
//! but use this dedicated protocol for the post-handshake flows. Two flows
//! are defined:
//!
//! - **Server registration**: long-lived connection sending `TrackerServerRegister`
//!   on a refresh interval, replaced idempotently each time.
//! - **Client listing**: short-lived connection sending one `TrackerServerList` and
//!   receiving one `TrackerServerListResponse`, then closing.
//!
//! The first post-handshake message locks the connection's role for its
//! lifetime; mixing message types is a protocol violation that triggers an
//! `Error` and disconnect.

use serde::{Deserialize, Serialize};

fn default_locale() -> String {
    crate::DEFAULT_LOCALE.to_string()
}

/// A registered server in a tracker's listing.
///
/// Returned to clients as elements of [`TrackerServerMessage::TrackerServerListResponse::servers`].
/// The tracker resolves the connecting-IP fallback before listing, so
/// `address` is always populated even when the registering server omitted it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerEntry {
    /// Server display name.
    pub name: String,
    /// Free-form description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Public hostname or IP, resolved by the tracker.
    pub address: String,
    /// BBS TCP port.
    pub port: u16,
    /// BBS WebSocket port (when the server has WebSocket enabled).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub websocket_port: Option<u16>,
    /// Server software version (e.g., `"0.8.3"`).
    pub version: String,
    /// TLS certificate fingerprint, canonical form (32 uppercase hex bytes
    /// separated by colons, 95 bytes total).
    pub fingerprint: String,
    /// Distinct online users (matches the user list).
    pub user_count: u32,
    /// Whether the guest account is enabled.
    pub allows_guest: bool,
}

/// Tracker client request messages.
///
/// The first one of these sent on a connection establishes the role:
/// `TrackerServerRegister` → server connection, `TrackerServerList` → client connection.
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TrackerClientMessage {
    /// Request the current list of registered servers.
    ///
    /// Sent by clients on a short-lived connection. The tracker responds
    /// with [`TrackerServerMessage::TrackerServerListResponse`] and closes.
    TrackerServerList {
        /// Listing password (if the tracker is gated).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        password: Option<String>,
        /// BCP-47 language tag for translated text in responses.
        #[serde(default = "default_locale")]
        locale: String,
        /// Sender's `CARGO_PKG_VERSION`, used by the tracker to filter
        /// the response to entries whose registered `version` is
        /// semver-compatible with the requesting client. Required —
        /// the tracker rejects requests with a missing, empty,
        /// over-cap, or unparseable version.
        version: String,
    },
    /// Register or refresh this server's entry.
    ///
    /// Sent by servers on a long-lived connection. The same structure is
    /// used for the initial registration and every subsequent refresh; the
    /// tracker replaces the stored entry idempotently.
    TrackerServerRegister {
        /// Registration password (if the tracker is gated).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        password: Option<String>,
        /// BCP-47 language tag for translated text in responses.
        #[serde(default = "default_locale")]
        locale: String,
        /// Server display name.
        name: String,
        /// Free-form description.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        /// Public hostname or IP. If omitted, the tracker substitutes the
        /// remote IP of the registering connection.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        address: Option<String>,
        /// BBS TCP port.
        port: u16,
        /// BBS WebSocket port (when the server has WebSocket enabled).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        websocket_port: Option<u16>,
        /// Server software version (e.g., `"0.8.3"`).
        version: String,
        /// TLS certificate fingerprint, canonical form (32 uppercase hex
        /// bytes separated by colons, 95 bytes total). The tracker MUST
        /// validate this format and reject non-canonical values.
        fingerprint: String,
        /// Distinct online users (matches the user list).
        user_count: u32,
        /// Whether the guest account is enabled.
        allows_guest: bool,
    },
}

/// Tracker server response messages.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TrackerServerMessage {
    /// Generic error sent when the tracker must abort the connection
    /// without a typed flow response (frame-level violations, role
    /// mismatches, unknown message types).
    Error {
        /// Human-readable description, localized per the request `locale`
        /// when known. Falls back to the implementation's default locale
        /// when `Error` is sent before any locale has been received.
        message: String,
        /// The offending message type, if known.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        command: Option<String>,
    },
    /// Response to [`TrackerClientMessage::TrackerServerList`].
    ///
    /// On success, `servers` contains entries sorted alphabetically by
    /// `name` (case-insensitive ascending). On failure, `error` and
    /// `error_kind` are populated and the tracker closes the connection.
    TrackerServerListResponse {
        /// Whether the listing request was accepted.
        success: bool,
        /// Array of registered servers, sorted alphabetically by `name`
        /// (case-insensitive ascending). Empty on the error path; an
        /// empty list on success means no servers are currently
        /// registered (not an error).
        servers: Vec<ServerEntry>,
        /// Human-readable explanation, localized per the request `locale`
        /// (present on failure).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        /// Machine-readable error category (present on failure).
        /// See `nexus_common::error_kind` for the canonical strings.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error_kind: Option<String>,
    },
    /// Response to [`TrackerClientMessage::TrackerServerRegister`].
    ///
    /// On success, `refresh_interval` instructs the server how long to
    /// wait before sending the next refresh. On failure, `error` and
    /// `error_kind` are populated and the tracker closes the connection.
    TrackerServerRegisterResponse {
        /// Whether the registration was accepted.
        success: bool,
        /// Seconds the server should wait before sending the next
        /// `TrackerServerRegister` (present on success).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        refresh_interval: Option<u32>,
        /// Human-readable explanation, localized per the request `locale`
        /// (present on failure).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        /// Machine-readable error category (present on failure).
        /// See `nexus_common::error_kind` for the canonical strings.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error_kind: Option<String>,
    },
}

// Manual `Debug` impl that redacts the `password` fields, matching the
// pattern used by `protocol::ClientMessage` for `Login` and `UserCreate`.
impl std::fmt::Debug for TrackerClientMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrackerClientMessage::TrackerServerList {
                password,
                locale,
                version,
            } => f
                .debug_struct("TrackerServerList")
                .field("password", &password.as_ref().map(|_| "<REDACTED>"))
                .field("locale", locale)
                .field("version", version)
                .finish(),
            TrackerClientMessage::TrackerServerRegister {
                password,
                locale,
                name,
                description,
                address,
                port,
                websocket_port,
                version,
                fingerprint,
                user_count,
                allows_guest,
            } => f
                .debug_struct("TrackerServerRegister")
                .field("password", &password.as_ref().map(|_| "<REDACTED>"))
                .field("locale", locale)
                .field("name", name)
                .field("description", description)
                .field("address", address)
                .field("port", port)
                .field("websocket_port", websocket_port)
                .field("version", version)
                .field("fingerprint", fingerprint)
                .field("user_count", user_count)
                .field("allows_guest", allows_guest)
                .finish(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracker_server_register_serialization() {
        let msg = TrackerClientMessage::TrackerServerRegister {
            password: None,
            locale: "en".to_string(),
            name: "My BBS".to_string(),
            description: Some("Welcome".to_string()),
            address: Some("bbs.example.com".to_string()),
            port: 7500,
            websocket_port: Some(7502),
            version: "0.8.3".to_string(),
            fingerprint: "AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99".to_string(),
            user_count: 12,
            allows_guest: true,
        };
        let json = serde_json::to_string(&msg).expect("serialize");
        assert!(json.contains(r#""type":"TrackerServerRegister""#));
        assert!(json.contains(r#""name":"My BBS""#));
        // Optional `password` is omitted when None
        assert!(!json.contains(r#""password""#));
    }

    #[test]
    fn test_tracker_server_list_serialization() {
        let msg = TrackerClientMessage::TrackerServerList {
            password: Some("secret".to_string()),
            locale: "en".to_string(),
            version: "0.8.3".to_string(),
        };
        let json = serde_json::to_string(&msg).expect("serialize");
        assert!(json.contains(r#""type":"TrackerServerList""#));
        assert!(json.contains(r#""password":"secret""#));
        assert!(json.contains(r#""locale":"en""#));
        assert!(json.contains(r#""version":"0.8.3""#));
    }

    #[test]
    fn test_tracker_server_register_response_success() {
        let msg = TrackerServerMessage::TrackerServerRegisterResponse {
            success: true,
            refresh_interval: Some(300),
            error: None,
            error_kind: None,
        };
        let json = serde_json::to_string(&msg).expect("serialize");
        assert!(json.contains(r#""type":"TrackerServerRegisterResponse""#));
        assert!(json.contains(r#""success":true"#));
        assert!(json.contains(r#""refresh_interval":300"#));
        assert!(!json.contains(r#""error""#));
    }

    #[test]
    fn test_tracker_server_register_response_failure() {
        let msg = TrackerServerMessage::TrackerServerRegisterResponse {
            success: false,
            refresh_interval: None,
            error: Some("Invalid password".to_string()),
            error_kind: Some("unauthorized".to_string()),
        };
        let json = serde_json::to_string(&msg).expect("serialize");
        assert!(json.contains(r#""success":false"#));
        assert!(json.contains(r#""error":"Invalid password""#));
        assert!(json.contains(r#""error_kind":"unauthorized""#));
    }

    #[test]
    fn test_tracker_server_list_response_empty() {
        let msg = TrackerServerMessage::TrackerServerListResponse {
            success: true,
            servers: Vec::new(),
            error: None,
            error_kind: None,
        };
        let json = serde_json::to_string(&msg).expect("serialize");
        assert!(json.contains(r#""servers":[]"#));
    }

    #[test]
    fn test_tracker_error_serialization() {
        let msg = TrackerServerMessage::Error {
            message: "Role violation".to_string(),
            command: Some("TrackerServerRegister".to_string()),
        };
        let json = serde_json::to_string(&msg).expect("serialize");
        assert!(json.contains(r#""type":"Error""#));
        assert!(json.contains(r#""message":"Role violation""#));
        assert!(json.contains(r#""command":"TrackerServerRegister""#));
    }

    #[test]
    fn test_password_redacted_in_debug() {
        let msg = TrackerClientMessage::TrackerServerList {
            password: Some("super-secret".to_string()),
            locale: "en".to_string(),
            version: "0.8.3".to_string(),
        };
        let debug = format!("{:?}", msg);
        assert!(!debug.contains("super-secret"));
        assert!(debug.contains("<REDACTED>"));
    }

    #[test]
    fn test_default_locale_on_deserialize() {
        // When `locale` is omitted, deserialization uses default "en".
        // `version` is required, so the JSON must include it.
        let json = r#"{"type":"TrackerServerList","version":"0.8.3"}"#;
        let msg: TrackerClientMessage = serde_json::from_str(json).expect("deserialize");
        match msg {
            TrackerClientMessage::TrackerServerList {
                locale,
                password,
                version,
            } => {
                assert_eq!(locale, "en");
                assert!(password.is_none());
                assert_eq!(version, "0.8.3");
            }
            _ => panic!("expected TrackerServerList"),
        }
    }

    #[test]
    fn test_missing_version_fails_deserialize() {
        // `version` is required — omitting it should cause deserialization
        // to fail. This is intentional: a request without a version is a
        // protocol violation, and falling through to "no filter" would
        // silently weaken compat enforcement.
        let json = r#"{"type":"TrackerServerList","locale":"en"}"#;
        let result: Result<TrackerClientMessage, _> = serde_json::from_str(json);
        assert!(result.is_err(), "missing version must fail to deserialize");
    }

    #[test]
    fn test_server_entry_roundtrip() {
        let entry = ServerEntry {
            name: "BBS".to_string(),
            description: None,
            address: "192.0.2.1".to_string(),
            port: 7500,
            websocket_port: None,
            version: "0.8.3".to_string(),
            fingerprint: "AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99".to_string(),
            user_count: 0,
            allows_guest: false,
        };
        let json = serde_json::to_string(&entry).expect("serialize");
        let parsed: ServerEntry = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(entry, parsed);
    }
}
