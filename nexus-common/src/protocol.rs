//! Protocol definitions for Nexus BBS
//!
//! ## Username vs Nickname
//!
//! - **Username**: Account identifier (database key). Used for admin operations.
//! - **Nickname**: Display name. Always populated; equals username for regular accounts.
//!
//! Rule: "Users type what they see" - user-facing commands use nicknames,
//! admin operations use usernames.

use serde::{Deserialize, Serialize};

fn default_locale() -> String {
    "en".to_string()
}

/// Client request messages
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    ChatSend {
        message: String,
    },
    ChatTopicUpdate {
        topic: String,
    },
    Handshake {
        version: String,
    },
    Login {
        username: String,
        password: String,
        features: Vec<String>,
        #[serde(default = "default_locale")]
        locale: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        avatar: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        nickname: Option<String>,
    },
    UserBroadcast {
        message: String,
    },
    UserCreate {
        username: String,
        password: String,
        is_admin: bool,
        #[serde(default)]
        is_shared: bool,
        enabled: bool,
        permissions: Vec<String>,
    },
    UserDelete {
        username: String,
    },
    UserEdit {
        username: String,
    },
    UserInfo {
        nickname: String,
    },
    UserKick {
        nickname: String,
    },
    UserList {
        #[serde(default)]
        all: bool,
    },
    UserMessage {
        to_nickname: String,
        message: String,
    },
    UserUpdate {
        username: String,
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
    ServerInfoUpdate {
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        max_connections_per_ip: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        image: Option<String>,
    },
    NewsList,
    NewsShow {
        id: i64,
    },
    NewsCreate {
        #[serde(skip_serializing_if = "Option::is_none")]
        body: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        image: Option<String>,
    },
    NewsEdit {
        id: i64,
    },
    NewsUpdate {
        id: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        body: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        image: Option<String>,
    },
    NewsDelete {
        id: i64,
    },
}

/// Server response messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    ChatMessage {
        session_id: u32,
        nickname: String,
        #[serde(default)]
        is_admin: bool,
        #[serde(default)]
        is_shared: bool,
        message: String,
    },
    ChatTopicUpdated {
        topic: String,
        username: String,
    },
    ChatTopicUpdateResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    Error {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        command: Option<String>,
    },
    HandshakeResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        version: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
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
    ServerBroadcast {
        session_id: u32,
        username: String,
        message: String,
    },
    UserConnected {
        user: UserInfo,
    },
    UserCreateResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        username: Option<String>,
    },
    UserDeleteResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        username: Option<String>,
    },
    UserEditResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        username: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_admin: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_shared: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        enabled: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        permissions: Option<Vec<String>>,
    },
    UserDisconnected {
        session_id: u32,
        nickname: String,
    },
    PermissionsUpdated {
        is_admin: bool,
        permissions: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        server_info: Option<ServerInfo>,
        #[serde(skip_serializing_if = "Option::is_none")]
        chat_info: Option<ChatInfo>,
    },
    ServerInfoUpdated {
        server_info: ServerInfo,
    },
    ServerInfoUpdateResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    UserBroadcastResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    UserInfoResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        user: Option<UserInfoDetailed>,
    },
    UserKickResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        nickname: Option<String>,
    },
    UserListResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        users: Option<Vec<UserInfo>>,
    },
    UserMessage {
        from_nickname: String,
        from_admin: bool,
        to_nickname: String,
        message: String,
    },
    UserMessageResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    UserUpdated {
        previous_username: String,
        user: UserInfo,
    },
    UserUpdateResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        username: Option<String>,
    },
    NewsListResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        items: Option<Vec<NewsItem>>,
    },
    NewsShowResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        news: Option<NewsItem>,
    },
    NewsCreateResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        news: Option<NewsItem>,
    },
    NewsEditResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        news: Option<NewsItem>,
    },
    NewsUpdateResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        news: Option<NewsItem>,
    },
    NewsDeleteResponse {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<i64>,
    },
    NewsUpdated {
        action: NewsAction,
        id: i64,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_connections_per_ip: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatInfo {
    pub topic: String,
    pub topic_set_by: String,
}

/// User info for lists. `nickname` is the display name (== username for regular accounts).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub username: String,
    pub nickname: String,
    pub login_time: i64,
    pub is_admin: bool,
    #[serde(default)]
    pub is_shared: bool,
    pub session_ids: Vec<u32>,
    pub locale: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NewsAction {
    Created,
    Updated,
    Deleted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsItem {
    pub id: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    pub author: String,
    pub author_is_admin: bool,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

/// Detailed user info. `nickname` is the display name (== username for regular accounts).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfoDetailed {
    pub username: String,
    pub nickname: String,
    pub login_time: i64,
    #[serde(default)]
    pub is_shared: bool,
    pub session_ids: Vec<u32>,
    pub features: Vec<String>,
    pub created_at: i64,
    pub locale: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_admin: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub addresses: Option<Vec<String>>,
}

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
            ClientMessage::UserInfo { nickname } => f
                .debug_struct("UserInfo")
                .field("nickname", nickname)
                .finish(),
            ClientMessage::UserKick { nickname } => f
                .debug_struct("UserKick")
                .field("nickname", nickname)
                .finish(),
            ClientMessage::UserList { all } => {
                f.debug_struct("UserList").field("all", all).finish()
            }
            ClientMessage::UserMessage {
                to_nickname,
                message,
            } => f
                .debug_struct("UserMessage")
                .field("to_nickname", to_nickname)
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
                assert_eq!(locale, "en");
                assert!(avatar.is_none());
                assert!(nickname.is_none());
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
        assert!(debug_output.contains("alice"));
        assert!(debug_output.contains("chat"));
        assert!(!debug_output.contains("super_secret_password"));
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
            nickname: "alice".to_string(),
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
            nickname: "alice".to_string(),
            login_time: 1234567890,
            is_admin: false,
            is_shared: false,
            session_ids: vec![1],
            locale: "en".to_string(),
            avatar: None,
        };
        let json = serde_json::to_string(&user_info).unwrap();
        assert!(!json.contains("\"avatar\""));
    }

    #[test]
    fn test_serialize_user_info_detailed_with_avatar() {
        let avatar_data = "data:image/png;base64,iVBORw0KGgo=".to_string();
        let user_info = UserInfoDetailed {
            username: "alice".to_string(),
            nickname: "alice".to_string(),
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
        assert!(debug_output.contains("..."));
        assert!(debug_output.contains("bytes"));
        assert!(!debug_output.contains(&large_avatar));
    }

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
        assert!(json.contains("\"type\":\"UserCreate\""));
        assert!(json.contains("\"is_shared\":true"));
        assert!(json.contains("\"is_admin\":false"));
    }

    #[test]
    fn test_deserialize_user_create_with_is_shared() {
        let json = r#"{"type":"UserCreate","username":"shared_acct","password":"secret","is_admin":false,"is_shared":true,"enabled":true,"permissions":["chat_send"]}"#;
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
    fn test_serialize_chat_message_with_is_admin_and_is_shared() {
        let msg = ServerMessage::ChatMessage {
            session_id: 1,
            nickname: "Nick1".to_string(),
            message: "Hello!".to_string(),
            is_admin: false,
            is_shared: true,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"ChatMessage\""));
        assert!(json.contains("\"nickname\":\"Nick1\""));
        assert!(json.contains("\"is_admin\":false"));
        assert!(json.contains("\"is_shared\":true"));
    }

    #[test]
    fn test_serialize_user_info_with_nickname_and_is_shared() {
        let user_info = UserInfo {
            username: "shared_acct".to_string(),
            nickname: "Nick1".to_string(),
            login_time: 1234567890,
            is_admin: false,
            is_shared: true,
            session_ids: vec![1],
            locale: "en".to_string(),
            avatar: None,
        };
        let json = serde_json::to_string(&user_info).unwrap();
        assert!(json.contains("\"username\":\"shared_acct\""));
        assert!(json.contains("\"nickname\":\"Nick1\""));
        assert!(json.contains("\"is_shared\":true"));
    }

    #[test]
    fn test_serialize_user_info_regular_user() {
        let user_info = UserInfo {
            username: "alice".to_string(),
            nickname: "alice".to_string(),
            login_time: 1234567890,
            is_admin: false,
            is_shared: false,
            session_ids: vec![1],
            locale: "en".to_string(),
            avatar: None,
        };
        let json = serde_json::to_string(&user_info).unwrap();
        assert!(json.contains("\"username\":\"alice\""));
        assert!(json.contains("\"nickname\":\"alice\""));
        assert!(json.contains("\"is_shared\":false"));
    }

    #[test]
    fn test_serialize_user_info_detailed_with_shared_fields() {
        let user_info = UserInfoDetailed {
            username: "shared_acct".to_string(),
            nickname: "Nick1".to_string(),
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
        assert!(json.contains("\"username\":\"shared_acct\""));
        assert!(json.contains("\"nickname\":\"Nick1\""));
        assert!(json.contains("\"is_shared\":true"));
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
        assert!(json.contains("\"type\":\"UserEditResponse\""));
        assert!(json.contains("\"is_shared\":true"));
    }

    #[test]
    fn test_serialize_user_edit_response_without_is_shared() {
        let msg = ServerMessage::UserEditResponse {
            success: true,
            error: None,
            username: Some("alice".to_string()),
            is_admin: Some(false),
            is_shared: Some(false),
            enabled: Some(true),
            permissions: Some(vec![]),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"UserEditResponse\""));
        assert!(json.contains("\"is_shared\":false"));
    }

    #[test]
    fn test_serialize_user_message_with_nicknames() {
        let msg = ServerMessage::UserMessage {
            from_nickname: "Nick1".to_string(),
            from_admin: false,
            to_nickname: "alice".to_string(),
            message: "Hello!".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"UserMessage\""));
        assert!(json.contains("\"from_nickname\":\"Nick1\""));
        assert!(json.contains("\"to_nickname\":\"alice\""));
    }

    #[test]
    fn test_serialize_user_disconnected_with_nickname() {
        let msg = ServerMessage::UserDisconnected {
            session_id: 1,
            nickname: "Nick1".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"UserDisconnected\""));
        assert!(json.contains("\"nickname\":\"Nick1\""));
    }

    #[test]
    fn test_serialize_user_kick_response_with_nickname() {
        let msg = ServerMessage::UserKickResponse {
            success: true,
            error: None,
            nickname: Some("Nick1".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"UserKickResponse\""));
        assert!(json.contains("\"nickname\":\"Nick1\""));
    }

    #[test]
    fn test_serialize_client_user_info_with_nickname() {
        let msg = ClientMessage::UserInfo {
            nickname: "Nick1".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"UserInfo\""));
        assert!(json.contains("\"nickname\":\"Nick1\""));
    }

    #[test]
    fn test_serialize_client_user_kick_with_nickname() {
        let msg = ClientMessage::UserKick {
            nickname: "Nick1".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"UserKick\""));
        assert!(json.contains("\"nickname\":\"Nick1\""));
    }

    #[test]
    fn test_serialize_client_user_message_with_to_nickname() {
        let msg = ClientMessage::UserMessage {
            to_nickname: "Nick1".to_string(),
            message: "Hello!".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"UserMessage\""));
        assert!(json.contains("\"to_nickname\":\"Nick1\""));
    }
}
