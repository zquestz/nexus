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
    /// Send a chat message to #server
    ChatSend { message: String },
    /// Handshake - must be sent first
    Handshake { version: String },
    /// Login request
    Login {
        username: String,
        password: String,
        features: Vec<String>,
    },
    /// Broadcast a message to all connected users
    UserBroadcast { message: String },
    /// Create a new user account
    UserCreate {
        username: String,
        password: String,
        is_admin: bool,
        permissions: Vec<String>,
    },
    /// Delete a user account
    UserDelete { username: String },
    /// Edit a user account
    UserEdit {
        username: String,
        requested_username: Option<String>,
        requested_password: Option<String>,
        requested_is_admin: Option<bool>,
        requested_permissions: Option<Vec<String>>,
    },
    /// Request information about a specific user
    UserInfo { session_id: u32 },
    /// Request list of connected users
    UserList,
}

/// Server response messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    /// Chat message
    ChatMessage {
        session_id: u32,
        username: String,
        message: String,
    },
    /// Error message
    Error {
        message: String,
        command: Option<String>,
    },
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
        is_admin: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        permissions: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// Broadcast message from another user
    ServerBroadcast {
        session_id: u32,
        username: String,
        message: String,
    },
    /// User connected event
    UserConnected { user: UserInfo },
    /// User create response
    UserCreateResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// User delete response
    UserDeleteResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// User edit response
    UserEditResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// User disconnected event
    UserDisconnected { session_id: u32, username: String },
    /// User broadcast reply
    UserBroadcastReply {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// User information response
    UserInfoResponse {
        user: Option<UserInfoDetailed>,
        error: Option<String>,
    },
    /// User list response
    UserListResponse { users: Vec<UserInfo> },
}

/// Information about a connected user (basic info for lists)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub session_id: u32,
    pub username: String,
    pub login_time: u64,
}

/// Detailed information about a user (for UserInfo command)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfoDetailed {
    pub session_id: u32,
    pub username: String,
    pub login_time: u64,
    pub features: Vec<String>,
    /// When the account was created (Unix timestamp)
    pub created_at: i64,
    /// Only included for admins viewing the info
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_admin: Option<bool>,
    /// Only included for admins viewing the info
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
}

// Custom Debug implementation that redacts passwords
impl std::fmt::Debug for ClientMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientMessage::ChatSend { message } => f
                .debug_struct("ChatSend")
                .field("message", message)
                .finish(),
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
            ClientMessage::UserBroadcast { message } => f
                .debug_struct("UserBroadcast")
                .field("message", message)
                .finish(),
            ClientMessage::UserCreate {
                username,
                is_admin,
                permissions,
                ..
            } => f
                .debug_struct("UserCreate")
                .field("username", username)
                .field("is_admin", is_admin)
                .field("permissions", permissions)
                .field("password", &"<REDACTED>")
                .finish(),
            ClientMessage::UserDelete { username } => f
                .debug_struct("UserDelete")
                .field("username", username)
                .finish(),
            ClientMessage::UserEdit {
                username,
                requested_username,
                requested_password: _,
                requested_is_admin,
                requested_permissions,
            } => f
                .debug_struct("UserEdit")
                .field("username", username)
                .field("requested_username", requested_username)
                .field("requested_password", &"<REDACTED>")
                .field("requested_is_admin", requested_is_admin)
                .field("requested_permissions", requested_permissions)
                .finish(),
            ClientMessage::UserInfo { session_id } => f
                .debug_struct("UserInfo")
                .field("session_id", session_id)
                .finish(),
            ClientMessage::UserList => f.debug_struct("UserList").finish(),
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
            is_admin: Some(false),
            permissions: Some(vec!["user_list".to_string()]),
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
            is_admin: None,
            permissions: None,
            error: Some("Invalid credentials".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"success\":false"));
        assert!(json.contains("\"error\""));
    }

    #[test]
    fn test_serialize_login_response_admin() {
        let msg = ServerMessage::LoginResponse {
            success: true,
            session_id: Some("admin123".to_string()),
            is_admin: Some(true),
            permissions: Some(vec![]),
            error: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"LoginResponse\""));
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("\"is_admin\":true"));
        assert!(json.contains("\"permissions\":[]"));
    }

    #[test]
    fn test_serialize_login_response_with_permissions() {
        let msg = ServerMessage::LoginResponse {
            success: true,
            session_id: Some("user123".to_string()),
            is_admin: Some(false),
            permissions: Some(vec![
                "user_list".to_string(),
                "chat_send".to_string(),
            ]),
            error: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"LoginResponse\""));
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("\"is_admin\":false"));
        assert!(json.contains("\"user_list\""));
        assert!(json.contains("\"chat_send\""));
    }
}
