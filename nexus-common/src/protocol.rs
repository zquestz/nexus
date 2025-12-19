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
        /// Nickname for shared accounts (required for shared, silently ignored for regular)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        nickname: Option<String>,
    },
    /// Broadcast a message to all connected users
    UserBroadcast { message: String },
    /// Create a new user account
    UserCreate {
        username: String,
        password: String,
        is_admin: bool,
        /// Whether this is a shared account (allows multiple logins with nicknames)
        #[serde(default)]
        is_shared: bool,
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
    UserList {
        /// If true, include all users from database (not just online)
        /// Requires user_list AND (user_edit OR user_delete) permissions
        #[serde(default)]
        all: bool,
    },
    /// Send a private message to a user
    UserMessage {
        to_username: String,
        message: String,
    },
    /// Update a user account
    UserUpdate {
        username: String,
        /// Current password (required when user is changing their own password)
        #[serde(skip_serializing_if = "Option::is_none")]
        current_password: Option<String>,
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
    /// Update server configuration (admin only)
    ServerInfoUpdate {
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        max_connections_per_ip: Option<u32>,
        /// Server image (logo/banner) as base64-encoded data URI
        #[serde(skip_serializing_if = "Option::is_none")]
        image: Option<String>,
    },
    /// Request list of all news items (oldest to newest)
    NewsList,
    /// Request a single news item by ID
    NewsShow { id: i64 },
    /// Create a new news item (requires news_create permission)
    NewsCreate {
        /// Markdown body text (optional if image is provided)
        #[serde(skip_serializing_if = "Option::is_none")]
        body: Option<String>,
        /// Image as base64-encoded data URI (optional if body is provided)
        #[serde(skip_serializing_if = "Option::is_none")]
        image: Option<String>,
    },
    /// Request news item details for editing
    NewsEdit { id: i64 },
    /// Update a news item
    NewsUpdate {
        id: i64,
        /// Markdown body text (optional if image is provided)
        #[serde(skip_serializing_if = "Option::is_none")]
        body: Option<String>,
        /// Image as base64-encoded data URI (optional if body is provided)
        #[serde(skip_serializing_if = "Option::is_none")]
        image: Option<String>,
    },
    /// Delete a news item
    NewsDelete { id: i64 },
}

/// Server response messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    /// Chat message
    ChatMessage {
        session_id: u32,
        /// Display name (nickname for shared accounts, username for regular)
        username: String,
        /// Whether the sender is a shared account user (for client-side coloring)
        #[serde(default)]
        is_shared: bool,
        message: String,
    },
    /// Chat topic updated broadcast (sent to users with ChatTopic permission when topic changes)
    ChatTopicUpdated { topic: String, username: String },
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
        chat_info: Option<ChatInfo>,
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
        #[serde(skip_serializing_if = "Option::is_none")]
        username: Option<String>,
    },
    /// User delete response
    UserDeleteResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        username: Option<String>,
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
        /// Whether this is a shared account (read-only, cannot be changed)
        #[serde(skip_serializing_if = "Option::is_none")]
        is_shared: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        enabled: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        permissions: Option<Vec<String>>,
    },
    /// User disconnected event
    UserDisconnected {
        session_id: u32,
        /// Display name (nickname for shared accounts, username for regular)
        username: String,
    },
    /// Permissions updated notification (sent to user when their permissions change)
    PermissionsUpdated {
        is_admin: bool,
        permissions: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        server_info: Option<ServerInfo>,
        #[serde(skip_serializing_if = "Option::is_none")]
        chat_info: Option<ChatInfo>,
    },
    /// Server configuration updated (broadcast to all connected users)
    ServerInfoUpdated { server_info: ServerInfo },
    /// Server info update response
    ServerInfoUpdateResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
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
        #[serde(skip_serializing_if = "Option::is_none")]
        username: Option<String>,
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
        #[serde(skip_serializing_if = "Option::is_none")]
        username: Option<String>,
    },
    /// News list response
    NewsListResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        items: Option<Vec<NewsItem>>,
    },
    /// News show response (single item)
    NewsShowResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        news: Option<NewsItem>,
    },
    /// News create response
    NewsCreateResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        news: Option<NewsItem>,
    },
    /// News edit response (returns current item for editing)
    NewsEditResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        news: Option<NewsItem>,
    },
    /// News update response
    NewsUpdateResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        news: Option<NewsItem>,
    },
    /// News delete response
    NewsDeleteResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<i64>,
    },
    /// News updated broadcast (sent to users with news_list permission)
    NewsUpdated { action: NewsAction, id: i64 },
}

/// Server information sent to clients on login
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerInfo {
    /// Server name
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Server description
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Server version
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Maximum connections allowed per IP address (admin only)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_connections_per_ip: Option<u32>,
    /// Server image (logo/banner) as base64-encoded data URI
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
}

/// Chat room information (topic, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatInfo {
    /// Current chat topic (empty string if not set)
    pub topic: String,
    /// Username who set the current topic (empty string if never set)
    pub topic_set_by: String,
}

/// Information about a connected user (basic info for lists)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub username: String,
    /// Nickname for shared account users (None for regular users)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,
    pub login_time: i64,
    pub is_admin: bool,
    /// Whether this is a shared account user
    #[serde(default)]
    pub is_shared: bool,
    pub session_ids: Vec<u32>,
    pub locale: String,
    /// User's avatar as a data URI (ephemeral, from most recent login)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
}

/// News action type for NewsUpdated broadcast
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NewsAction {
    Created,
    Updated,
    Deleted,
}

/// A news item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsItem {
    pub id: i64,
    /// Markdown body text (None if image-only)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// Image as base64-encoded data URI (None if text-only)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    /// Username of the author
    pub author: String,
    /// Whether the author is an admin (for display purposes)
    pub author_is_admin: bool,
    /// Creation timestamp (ISO 8601)
    pub created_at: String,
    /// Last update timestamp (ISO 8601), None if never edited
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

/// Detailed information about a user (for UserInfo command)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfoDetailed {
    pub username: String,
    /// Nickname for shared account users (None for regular users)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,
    pub login_time: i64,
    /// Whether this is a shared account user
    #[serde(default)]
    pub is_shared: bool,
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
                nickname,
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
                .field("nickname", nickname)
                .finish(),
            ClientMessage::UserBroadcast { message } => f
                .debug_struct("UserBroadcast")
                .field("message", message)
                .finish(),
            ClientMessage::UserCreate {
                username,
                is_admin,
                is_shared,
                permissions,
                ..
            } => f
                .debug_struct("UserCreate")
                .field("username", username)
                .field("is_admin", is_admin)
                .field("is_shared", is_shared)
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
            ClientMessage::UserList { all } => {
                f.debug_struct("UserList").field("all", all).finish()
            }
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
                current_password: _,
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
            ClientMessage::ServerInfoUpdate {
                name,
                description,
                max_connections_per_ip,
                image,
            } => {
                let mut s = f.debug_struct("ServerInfoUpdate");
                s.field("name", name)
                    .field("description", description)
                    .field("max_connections_per_ip", max_connections_per_ip);
                // Truncate large images in debug output
                if let Some(img) = image {
                    if img.len() > 100 {
                        s.field(
                            "image",
                            &format!("{}... ({} bytes)", &img[..100], img.len()),
                        );
                    } else {
                        s.field("image", &Some(img));
                    }
                } else {
                    s.field("image", &None::<String>);
                }
                s.finish()
            }
            ClientMessage::NewsList => f.debug_struct("NewsList").finish(),
            ClientMessage::NewsShow { id } => f.debug_struct("NewsShow").field("id", id).finish(),
            ClientMessage::NewsCreate { body, image } => {
                let mut s = f.debug_struct("NewsCreate");
                s.field("body", body);
                // Truncate large images in debug output
                if let Some(img) = image {
                    if img.len() > 100 {
                        s.field(
                            "image",
                            &format!("{}... ({} bytes)", &img[..100], img.len()),
                        );
                    } else {
                        s.field("image", &Some(img));
                    }
                } else {
                    s.field("image", &None::<String>);
                }
                s.finish()
            }
            ClientMessage::NewsEdit { id } => f.debug_struct("NewsEdit").field("id", id).finish(),
            ClientMessage::NewsUpdate { id, body, image } => {
                let mut s = f.debug_struct("NewsUpdate");
                s.field("id", id).field("body", body);
                // Truncate large images in debug output
                if let Some(img) = image {
                    if img.len() > 100 {
                        s.field(
                            "image",
                            &format!("{}... ({} bytes)", &img[..100], img.len()),
                        );
                    } else {
                        s.field("image", &Some(img));
                    }
                } else {
                    s.field("image", &None::<String>);
                }
                s.finish()
            }
            ClientMessage::NewsDelete { id } => {
                f.debug_struct("NewsDelete").field("id", id).finish()
            }
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
            nickname: None,
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
                nickname,
            } => {
                assert_eq!(username, "alice");
                assert_eq!(password, "secret");
                assert_eq!(features, vec!["chat".to_string()]);
                assert_eq!(locale, "en"); // Default locale
                assert!(avatar.is_none()); // Default avatar
                assert!(nickname.is_none()); // Default nickname
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
            nickname: None,
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
            chat_info: None,
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
            chat_info: None,
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
            chat_info: None,
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
            chat_info: None,
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
            nickname: None,
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
            nickname: None,
            login_time: 1234567890,
            is_admin: false,
            is_shared: false,
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
            nickname: None,
            login_time: 1234567890,
            is_admin: false,
            is_shared: false,
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
            nickname: None,
            login_time: 1234567890,
            is_shared: false,
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
            nickname: None,
        };
        let debug_output = format!("{:?}", msg);

        // Should truncate the avatar and show byte count
        assert!(debug_output.contains("..."));
        assert!(debug_output.contains("bytes"));
        // Should NOT contain the full avatar
        assert!(!debug_output.contains(&large_avatar));
    }

    // =========================================================================
    // Shared account serialization tests
    // =========================================================================

    #[test]
    fn test_serialize_login_with_nickname() {
        let msg = ClientMessage::Login {
            username: "shared_acct".to_string(),
            password: "secret".to_string(),
            features: vec![],
            locale: "en".to_string(),
            avatar: None,
            nickname: Some("Nick1".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"nickname\":\"Nick1\""));
    }

    #[test]
    fn test_serialize_login_without_nickname() {
        let msg = ClientMessage::Login {
            username: "alice".to_string(),
            password: "secret".to_string(),
            features: vec![],
            locale: "en".to_string(),
            avatar: None,
            nickname: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        // nickname should not be in JSON when None (skip_serializing_if)
        assert!(!json.contains("\"nickname\""));
    }

    #[test]
    fn test_deserialize_login_with_nickname() {
        let json = r#"{"type":"Login","username":"shared_acct","password":"secret","features":[],"locale":"en","nickname":"Nick1"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::Login { nickname, .. } => {
                assert_eq!(nickname, Some("Nick1".to_string()));
            }
            _ => panic!("Expected Login message"),
        }
    }

    #[test]
    fn test_serialize_user_create_with_is_shared() {
        let msg = ClientMessage::UserCreate {
            username: "shared_acct".to_string(),
            password: "secret".to_string(),
            is_admin: false,
            is_shared: true,
            enabled: true,
            permissions: vec!["chat_send".to_string()],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"is_shared\":true"));
    }

    #[test]
    fn test_deserialize_user_create_with_is_shared() {
        let json = r#"{"type":"UserCreate","username":"shared_acct","password":"secret","is_admin":false,"is_shared":true,"enabled":true,"permissions":[]}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::UserCreate { is_shared, .. } => {
                assert!(is_shared);
            }
            _ => panic!("Expected UserCreate message"),
        }
    }

    #[test]
    fn test_deserialize_user_create_defaults_is_shared_false() {
        // Old clients may not send is_shared field - should default to false
        let json = r#"{"type":"UserCreate","username":"alice","password":"secret","is_admin":false,"enabled":true,"permissions":[]}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::UserCreate { is_shared, .. } => {
                assert!(!is_shared);
            }
            _ => panic!("Expected UserCreate message"),
        }
    }

    #[test]
    fn test_serialize_chat_message_with_is_shared() {
        let msg = ServerMessage::ChatMessage {
            session_id: 123,
            username: "Nick1".to_string(),
            is_shared: true,
            message: "Hello!".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"is_shared\":true"));
        assert!(json.contains("\"username\":\"Nick1\""));
    }

    #[test]
    fn test_serialize_user_info_with_nickname_and_is_shared() {
        let user_info = UserInfo {
            username: "shared_acct".to_string(),
            nickname: Some("Nick1".to_string()),
            login_time: 1234567890,
            is_admin: false,
            is_shared: true,
            session_ids: vec![1],
            locale: "en".to_string(),
            avatar: None,
        };
        let json = serde_json::to_string(&user_info).unwrap();
        assert!(json.contains("\"nickname\":\"Nick1\""));
        assert!(json.contains("\"is_shared\":true"));
        assert!(json.contains("\"username\":\"shared_acct\""));
    }

    #[test]
    fn test_serialize_user_info_without_nickname() {
        let user_info = UserInfo {
            username: "alice".to_string(),
            nickname: None,
            login_time: 1234567890,
            is_admin: false,
            is_shared: false,
            session_ids: vec![1],
            locale: "en".to_string(),
            avatar: None,
        };
        let json = serde_json::to_string(&user_info).unwrap();
        // nickname should not be in JSON when None (skip_serializing_if)
        assert!(!json.contains("\"nickname\""));
        // is_shared should still be present (no skip_serializing_if)
        assert!(json.contains("\"is_shared\":false"));
    }

    #[test]
    fn test_deserialize_user_info_with_shared_fields() {
        let json = r#"{"username":"shared_acct","nickname":"Nick1","login_time":1234567890,"is_admin":false,"is_shared":true,"session_ids":[1],"locale":"en"}"#;
        let user_info: UserInfo = serde_json::from_str(json).unwrap();
        assert_eq!(user_info.username, "shared_acct");
        assert_eq!(user_info.nickname, Some("Nick1".to_string()));
        assert!(user_info.is_shared);
    }

    #[test]
    fn test_deserialize_user_info_defaults_shared_fields() {
        // Old servers may not send nickname or is_shared - should default
        let json = r#"{"username":"alice","login_time":1234567890,"is_admin":false,"session_ids":[1],"locale":"en"}"#;
        let user_info: UserInfo = serde_json::from_str(json).unwrap();
        assert_eq!(user_info.nickname, None);
        assert!(!user_info.is_shared);
    }

    #[test]
    fn test_serialize_user_info_detailed_with_shared_fields() {
        let user_info = UserInfoDetailed {
            username: "shared_acct".to_string(),
            nickname: Some("Nick1".to_string()),
            login_time: 1234567890,
            is_shared: true,
            session_ids: vec![1, 2],
            features: vec!["chat".to_string()],
            created_at: 1234567800,
            locale: "en".to_string(),
            avatar: None,
            is_admin: Some(false),
            addresses: None,
        };
        let json = serde_json::to_string(&user_info).unwrap();
        assert!(json.contains("\"nickname\":\"Nick1\""));
        assert!(json.contains("\"is_shared\":true"));
    }

    #[test]
    fn test_deserialize_user_info_detailed_defaults_shared_fields() {
        // Old servers may not send nickname or is_shared - should default
        let json = r#"{"username":"alice","login_time":1234567890,"session_ids":[1],"features":[],"created_at":1234567800,"locale":"en"}"#;
        let user_info: UserInfoDetailed = serde_json::from_str(json).unwrap();
        assert_eq!(user_info.nickname, None);
        assert!(!user_info.is_shared);
    }

    #[test]
    fn test_serialize_user_edit_response_with_is_shared() {
        let msg = ServerMessage::UserEditResponse {
            success: true,
            error: None,
            username: Some("shared_acct".to_string()),
            is_admin: Some(false),
            is_shared: Some(true),
            enabled: Some(true),
            permissions: Some(vec!["chat_send".to_string()]),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"is_shared\":true"));
    }

    #[test]
    fn test_serialize_user_edit_response_without_is_shared() {
        let msg = ServerMessage::UserEditResponse {
            success: false,
            error: Some("User not found".to_string()),
            username: None,
            is_admin: None,
            is_shared: None,
            enabled: None,
            permissions: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        // is_shared should not be in JSON when None (skip_serializing_if)
        assert!(!json.contains("\"is_shared\""));
    }
}
