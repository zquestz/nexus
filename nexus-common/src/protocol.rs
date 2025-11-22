//! Protocol definitions for Nexus BBS
//!
//! All messages are sent as newline-delimited JSON.

use serde::{Deserialize, Serialize};

/// Client request messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    /// Handshake - must be sent first
    Handshake {
        version: String,
    },
    /// Login request
    Login {
        username: String,
        password: String,
    },
    /// Request list of connected users
    UserList,
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
    UserListResponse {
        users: Vec<UserInfo>,
    },
    /// User connected event
    UserConnected {
        user: UserInfo,
    },
    /// User disconnected event
    UserDisconnected {
        user_id: u32,
        username: String,
    },
}

/// Information about a connected user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub id: u32,
    pub username: String,
    pub login_time: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_login() {
        let msg = ClientMessage::Login {
            username: "alice".to_string(),
            password: "secret".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"Login\""));
        assert!(json.contains("\"username\":\"alice\""));
    }

    #[test]
    fn test_deserialize_login() {
        let json = r#"{"type":"Login","username":"alice","password":"secret"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::Login { username, password } => {
                assert_eq!(username, "alice");
                assert_eq!(password, "secret");
            }
        }
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