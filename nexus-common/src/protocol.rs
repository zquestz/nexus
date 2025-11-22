//! Protocol definitions for Nexus BBS
//!
//! All messages are sent as newline-delimited JSON.
//!
//! ## Password Security
//!
//! Clients send passwords in plaintext in Login messages. The Yggdrasil network
//! provides end-to-end encryption, so passwords are secure in transit.
//!
//! The server hashes passwords using Argon2id with per-user salts before storing them.

use serde::{Deserialize, Serialize};

/// Client request messages
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    /// Handshake - must be sent first
    Handshake { version: String },
    /// Login request
    Login {
        username: String,
        password: String,
        features: Vec<String>,
    },
    /// Request list of connected users
    UserList,
    /// Send a chat message to #server
    ChatSend { message: String },
}

/// Server response messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    /// Handshake response
    HandshakeResponse {
        success: bool,
        version: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// Login response
    LoginResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        session_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// User list response
    UserListResponse { users: Vec<UserInfo> },
    /// User connected event
    UserConnected { user: UserInfo },
    /// User disconnected event
    UserDisconnected { user_id: u32, username: String },
    /// Chat message from #server
    ChatMessage {
        user_id: u32,
        username: String,
        message: String,
    },
    /// Generic error message (usually followed by disconnection)
    Error {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        command: Option<String>,
    },
}

/// Information about a connected user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub id: u32,
    pub username: String,
    pub login_time: u64,
}

// Custom Debug implementation that redacts passwords
impl std::fmt::Debug for ClientMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientMessage::Handshake { version } => f
                .debug_struct("Handshake")
                .field("version", version)
                .finish(),
            ClientMessage::Login {
                username,
                password: _,
                features,
            } => f
                .debug_struct("Login")
                .field("username", username)
                .field("password", &"<REDACTED>")
                .field("features", features)
                .finish(),
            ClientMessage::UserList => f.debug_struct("UserList").finish(),
            ClientMessage::ChatSend { message } => f
                .debug_struct("ChatSend")
                .field("message", message)
                .finish(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_login() {
        let msg = ClientMessage::Login {
            username: "alice".to_string(),
            password: "secret".to_string(),
            features: vec!["chat".to_string()],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"Login\""));
        assert!(json.contains("\"username\":\"alice\""));
        assert!(json.contains("\"features\""));
    }

    #[test]
    fn test_deserialize_login() {
        let json = r#"{"type":"Login","username":"alice","password":"secret","features":["chat"]}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::Login {
                username,
                password,
                features,
            } => {
                assert_eq!(username, "alice");
                assert_eq!(password, "secret");
                assert_eq!(features, vec!["chat".to_string()]);
            }
            _ => panic!("Expected Login message"),
        }
    }

    #[test]
    fn test_debug_redacts_password() {
        let msg = ClientMessage::Login {
            username: "alice".to_string(),
            password: "super_secret_password".to_string(),
            features: vec!["chat".to_string()],
        };
        let debug_output = format!("{:?}", msg);

        // Should contain username and features
        assert!(debug_output.contains("alice"));
        assert!(debug_output.contains("chat"));

        // Should NOT contain the actual password
        assert!(!debug_output.contains("super_secret_password"));

        // Should contain the redaction marker
        assert!(debug_output.contains("REDACTED"));
    }

    #[test]
    fn test_serialize_login_response() {
        let msg = ServerMessage::LoginResponse {
            success: true,
            session_id: Some("abc123".to_string()),
            error: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"LoginResponse\""));
        assert!(json.contains("\"success\":true"));
    }

    #[test]
    fn test_serialize_login_error() {
        let msg = ServerMessage::LoginResponse {
            success: false,
            session_id: None,
            error: Some("Invalid credentials".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"success\":false"));
        assert!(json.contains("\"error\""));
    }
}
