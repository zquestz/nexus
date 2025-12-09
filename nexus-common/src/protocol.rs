//! Protocol definitions for Nexus BBS
//!
//! All messages are sent as newline-delimited JSON over TLS.
//!
//! ## Password Security
//!
//! Clients send passwords in plaintext in Login messages. TLS encryption
//! ensures passwords are secure in transit.
//!
//! The server hashes passwords using Argon2id with per-user salts before storing them.

use serde::{Deserialize, Serialize};

/// Default locale for backwards compatibility with old clients
fn default_locale() -> String {
    "en".to_string()
}

/// Client request messages
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    /// Send a chat message to #server
    ChatSend { message: String },
    /// Update the chat topic
    ChatTopicUpdate { topic: String },
    /// Handshake - must be sent first
    Handshake { version: String },
    /// Login request
    Login {
        username: String,
        password: String,
        features: Vec<String>,
        #[serde(default = "default_locale")]
        locale: String,
        /// User's avatar as a data URI (e.g., "data:image/png;base64,...")
        #[serde(default, skip_serializing_if = "Option::is_none")]
        avatar: Option<String>,
    },
    /// Broadcast a message to all connected users
    UserBroadcast { message: String },
    /// Create a new user account
    UserCreate {
        username: String,
        password: String,
        is_admin: bool,
        enabled: bool,
        permissions: Vec<String>,
    },
    /// Delete a user account
    UserDelete { username: String },
    /// Request user details for editing (returns admin status and permissions)
    UserEdit { username: String },
    /// Request information about a specific user
    UserInfo { username: String },
    /// Kick/disconnect a user
    UserKick { username: String },
    /// Request list of connected users
    UserList,
    /// Send a private message to a user
    UserMessage {
        to_username: String,
        message: String,
    },
    /// Update a user account
    UserUpdate {
        username: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        requested_username: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        requested_password: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        requested_is_admin: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        requested_enabled: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        requested_permissions: Option<Vec<String>>,
    },
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
    /// Chat topic broadcast (sent to users with ChatTopic permission when topic changes)
    ChatTopic { topic: String, username: String },
    /// Chat topic update response
    ChatTopicUpdateResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// Error message
    Error {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        command: Option<String>,
    },
    /// Handshake response
    HandshakeResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        version: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// Login response
    LoginResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        session_id: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_admin: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        permissions: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        server_info: Option<ServerInfo>,
        #[serde(skip_serializing_if = "Option::is_none")]
        locale: Option<String>,
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
    /// User edit response (returns current user details for editing)
    UserEditResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        username: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_admin: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        enabled: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        permissions: Option<Vec<String>>,
    },
    /// User disconnected event
    UserDisconnected { session_id: u32, username: String },
    /// Permissions updated notification (sent to user when their permissions change)
    PermissionsUpdated {
        is_admin: bool,
        permissions: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        server_info: Option<ServerInfo>,
    },
    /// User broadcast response
    UserBroadcastResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// User information response
    UserInfoResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        user: Option<UserInfoDetailed>,
    },
    /// User kick response
    UserKickResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// User list response
    UserListResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        users: Option<Vec<UserInfo>>,
    },
    /// Private message (broadcast to all sessions of sender and receiver)
    UserMessage {
        from_username: String,
        from_admin: bool,
        to_username: String,
        message: String,
    },
    /// User message response
    UserMessageResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// User updated event (broadcast when user's admin status or username changes)
    UserUpdated {
        previous_username: String,
        user: UserInfo,
    },
    /// User update response
    UserUpdateResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
}

/// Server information sent to clients on login
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    /// Server name
    pub name: String,
    /// Server description (empty string if not set)
    pub description: String,
    /// Server version
    pub version: String,
    /// Current chat topic (empty string if not set)
    pub chat_topic: String,
    /// Username who set the current topic (empty string if never set)
    pub chat_topic_set_by: String,
    /// Maximum connections allowed per IP address (admin only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_connections_per_ip: Option<u32>,
}

/// Information about a connected user (basic info for lists)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub username: String,
    pub login_time: i64,
    pub is_admin: bool,
    pub session_ids: Vec<u32>,
    pub locale: String,
    /// User's avatar as a data URI (ephemeral, from most recent login)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
}

/// Detailed information about a user (for UserInfo command)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfoDetailed {
    pub username: String,
    pub login_time: i64,
    pub session_ids: Vec<u32>,
    pub features: Vec<String>,
    /// When the account was created (Unix timestamp)
    pub created_at: i64,
    /// User's preferred locale
    pub locale: String,
    /// User's avatar as a data URI (ephemeral, from most recent login)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    /// Only included for admins viewing the info
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_admin: Option<bool>,
    /// Only included for admins viewing the info (one per session)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub addresses: Option<Vec<String>>,
}

// Custom Debug implementation that redacts passwords
impl std::fmt::Debug for ClientMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientMessage::ChatSend { message } => f
                .debug_struct("ChatSend")
                .field("message", message)
                .finish(),
            ClientMessage::ChatTopicUpdate { topic } => f
                .debug_struct("ChatTopicUpdate")
                .field("topic", topic)
                .finish(),
            ClientMessage::Handshake { version } => f
                .debug_struct("Handshake")
                .field("version", version)
                .finish(),
            ClientMessage::Login {
                username,
                password: _,
                features,
                locale,
                avatar,
            } => f
                .debug_struct("Login")
                .field("username", username)
                .field("password", &"<REDACTED>")
                .field("features", features)
                .field("locale", locale)
                .field(
                    "avatar",
                    &avatar.as_ref().map(|a| {
                        if a.len() > 50 {
                            format!("{}...<{} bytes>", &a[..50], a.len())
                        } else {
                            a.clone()
                        }
                    }),
                )
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
            ClientMessage::UserEdit { username } => f
                .debug_struct("UserEdit")
                .field("username", username)
                .finish(),
            ClientMessage::UserInfo { username } => f
                .debug_struct("UserInfo")
                .field("username", username)
                .finish(),
            ClientMessage::UserKick { username } => f
                .debug_struct("UserKick")
                .field("username", username)
                .finish(),
            ClientMessage::UserList => f.debug_struct("UserList").finish(),
            ClientMessage::UserMessage {
                to_username,
                message,
            } => f
                .debug_struct("UserMessage")
                .field("to_username", to_username)
                .field("message", message)
                .finish(),
            ClientMessage::UserUpdate {
                username,
                requested_username,
                requested_password: _,
                requested_is_admin,
                requested_enabled,
                requested_permissions,
            } => f
                .debug_struct("UserUpdate")
                .field("username", username)
                .field("requested_username", requested_username)
                .field("requested_password", &"<REDACTED>")
                .field("requested_is_admin", requested_is_admin)
                .field("requested_enabled", requested_enabled)
                .field("requested_permissions", requested_permissions)
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
            locale: "en".to_string(),
            avatar: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"Login\""));
        assert!(json.contains("\"username\":\"alice\""));
        assert!(json.contains("\"features\""));
        assert!(json.contains("\"locale\":\"en\""));
        // avatar is None so should not be serialized
        assert!(!json.contains("\"avatar\""));
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
                locale,
                avatar,
            } => {
                assert_eq!(username, "alice");
                assert_eq!(password, "secret");
                assert_eq!(features, vec!["chat".to_string()]);
                assert_eq!(locale, "en"); // Default locale
                assert!(avatar.is_none()); // Default avatar
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
            locale: "en".to_string(),
            avatar: None,
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
            session_id: Some(12345),
            is_admin: Some(false),
            permissions: Some(vec!["user_list".to_string()]),
            server_info: None,
            locale: Some("en".to_string()),
            error: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"LoginResponse\""));
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("\"session_id\":12345"));
    }

    #[test]
    fn test_serialize_login_error() {
        let msg = ServerMessage::LoginResponse {
            success: false,
            session_id: None,
            is_admin: None,
            permissions: None,
            server_info: None,
            locale: None,
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
            session_id: Some(99999),
            is_admin: Some(true),
            permissions: Some(vec![]),
            server_info: None,
            locale: Some("en".to_string()),
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
            session_id: Some(67890),
            is_admin: Some(false),
            permissions: Some(vec!["user_list".to_string(), "chat_send".to_string()]),
            server_info: None,
            locale: Some("en".to_string()),
            error: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"LoginResponse\""));
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("\"is_admin\":false"));
        assert!(json.contains("\"user_list\""));
        assert!(json.contains("\"chat_send\""));
    }

    // =========================================================================
    // Avatar serialization tests
    // =========================================================================

    #[test]
    fn test_serialize_login_with_avatar() {
        let avatar_data = "data:image/png;base64,iVBORw0KGgo=".to_string();
        let msg = ClientMessage::Login {
            username: "alice".to_string(),
            password: "secret".to_string(),
            features: vec!["chat".to_string()],
            locale: "en".to_string(),
            avatar: Some(avatar_data.clone()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"avatar\""));
        assert!(json.contains(&avatar_data));
    }

    #[test]
    fn test_deserialize_login_with_avatar() {
        let json = r#"{"type":"Login","username":"alice","password":"secret","features":["chat"],"locale":"en","avatar":"data:image/png;base64,abc123"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::Login { avatar, .. } => {
                assert_eq!(avatar, Some("data:image/png;base64,abc123".to_string()));
            }
            _ => panic!("Expected Login message"),
        }
    }

    #[test]
    fn test_serialize_user_info_with_avatar() {
        let avatar_data = "data:image/png;base64,iVBORw0KGgo=".to_string();
        let user_info = UserInfo {
            username: "alice".to_string(),
            login_time: 1234567890,
            is_admin: false,
            session_ids: vec![1],
            locale: "en".to_string(),
            avatar: Some(avatar_data.clone()),
        };
        let json = serde_json::to_string(&user_info).unwrap();
        assert!(json.contains("\"avatar\""));
        assert!(json.contains(&avatar_data));
    }

    #[test]
    fn test_serialize_user_info_without_avatar() {
        let user_info = UserInfo {
            username: "alice".to_string(),
            login_time: 1234567890,
            is_admin: false,
            session_ids: vec![1],
            locale: "en".to_string(),
            avatar: None,
        };
        let json = serde_json::to_string(&user_info).unwrap();
        // avatar should not be in JSON when None (skip_serializing_if)
        assert!(!json.contains("\"avatar\""));
    }

    #[test]
    fn test_serialize_user_info_detailed_with_avatar() {
        let avatar_data = "data:image/png;base64,iVBORw0KGgo=".to_string();
        let user_info = UserInfoDetailed {
            username: "alice".to_string(),
            login_time: 1234567890,
            session_ids: vec![1, 2],
            features: vec!["chat".to_string()],
            created_at: 1234567800,
            locale: "en".to_string(),
            avatar: Some(avatar_data.clone()),
            is_admin: Some(false),
            addresses: None,
        };
        let json = serde_json::to_string(&user_info).unwrap();
        assert!(json.contains("\"avatar\""));
        assert!(json.contains(&avatar_data));
    }

    #[test]
    fn test_debug_login_truncates_large_avatar() {
        // Create a large avatar string
        let large_avatar = format!("data:image/png;base64,{}", "A".repeat(1000));
        let msg = ClientMessage::Login {
            username: "alice".to_string(),
            password: "secret".to_string(),
            features: vec![],
            locale: "en".to_string(),
            avatar: Some(large_avatar.clone()),
        };
        let debug_output = format!("{:?}", msg);

        // Should truncate the avatar and show byte count
        assert!(debug_output.contains("..."));
        assert!(debug_output.contains("bytes"));
        // Should NOT contain the full avatar
        assert!(!debug_output.contains(&large_avatar));
    }
}
