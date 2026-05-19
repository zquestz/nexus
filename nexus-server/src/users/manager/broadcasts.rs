//! Broadcast methods for UserManager

use nexus_common::protocol::ServerMessage;

use crate::handlers::{ServerInfoOptions, ServerInfoValues, build_server_info};

use super::UserManager;
use crate::db::Permission;

impl UserManager {
    /// Send a message to a specific session by session ID
    ///
    /// Returns true if the message was sent, false if the session doesn't exist
    /// or the channel is closed.
    pub async fn send_to_session(&self, session_id: u32, message: ServerMessage) -> bool {
        let users = self.users.read().await;
        if let Some(user) = users.get(&session_id) {
            user.tx.send((message, None)).is_ok()
        } else {
            false
        }
    }

    /// Broadcast a message to all connected users with proper disconnect notification
    ///
    /// Automatically removes users whose channels have closed and notifies other clients
    /// with user_list permission about the disconnection.
    pub async fn broadcast(&self, message: ServerMessage) {
        let mut disconnected = Vec::new();

        {
            let users = self.users.read().await;
            for user in users.values() {
                if user.tx.send((message.clone(), None)).is_err() {
                    disconnected.push(user.session_id);
                }
            }
        }

        self.remove_disconnected(disconnected).await;
    }

    /// Broadcast a message to all users with a specific feature and permission
    ///
    /// This method checks both that the user has requested the feature (client preference)
    /// and that they have permission to receive it (server enforcement).
    ///
    /// Optionally excludes a specific session_id (e.g., the originator of an action who
    /// already received an authoritative typed response and doesn't need the broadcast).
    ///
    /// Automatically removes users whose channels have closed (disconnected connections).
    pub async fn broadcast_to_feature(
        &self,
        feature: &str,
        message: ServerMessage,
        required_permission: Permission,
        exclude_session_id: Option<u32>,
    ) {
        let mut disconnected = Vec::new();

        {
            let users = self.users.read().await;
            for user in users.values() {
                // Skip excluded session
                if let Some(excluded) = exclude_session_id
                    && user.session_id == excluded
                {
                    continue;
                }

                // Check if user has the required feature
                if !user.has_feature(feature) {
                    continue;
                }

                // Check if user has the required permission (uses cached permissions, admin bypass)
                if !user.has_permission(required_permission) {
                    continue;
                }

                // Send message to this user
                if user.tx.send((message.clone(), None)).is_err() {
                    disconnected.push(user.session_id);
                }
            }
        }

        self.remove_disconnected(disconnected).await;
    }

    /// Broadcast to every session of `user_id`. Keyed on the
    /// immutable PK so a concurrent rename can't make the lookup miss
    /// sessions. Automatically removes sessions whose channels have
    /// closed (disconnected connections).
    pub async fn broadcast_to_user_id(&self, user_id: i64, message: &ServerMessage) {
        let mut disconnected = Vec::new();

        {
            let users = self.users.read().await;
            for user in users.values() {
                if user.user_id == user_id && user.tx.send((message.clone(), None)).is_err() {
                    disconnected.push(user.session_id);
                }
            }
        }

        self.remove_disconnected(disconnected).await;
    }

    /// Broadcast a message to all sessions with a specific nickname (case-insensitive)
    ///
    /// This works correctly for both regular and shared accounts:
    /// - Regular accounts: nickname == username, so all sessions of the user receive the message
    /// - Shared accounts: each session has a unique nickname, so only that session receives it
    ///
    /// Automatically removes users whose channels have closed (disconnected connections).
    pub async fn broadcast_to_nickname(&self, nickname: &str, message: &ServerMessage) {
        let mut disconnected = Vec::new();

        let nickname_lower = nickname.to_lowercase();

        {
            let users = self.users.read().await;
            for user in users.values() {
                if user.nickname.to_lowercase() == nickname_lower
                    && user.tx.send((message.clone(), None)).is_err()
                {
                    disconnected.push(user.session_id);
                }
            }
        }

        self.remove_disconnected(disconnected).await;
    }

    /// Broadcast a message to all users with a specific permission
    ///
    /// This method checks that users have the required permission (server enforcement).
    /// Used for broadcasting events like topic updates to users who have permission to see them.
    ///
    /// Automatically removes users whose channels have closed (disconnected connections).
    pub async fn broadcast_to_permission(
        &self,
        message: ServerMessage,
        required_permission: Permission,
    ) {
        let mut disconnected = Vec::new();

        {
            let users = self.users.read().await;
            for user in users.values() {
                // Check if user has the required permission (uses cached permissions, admin bypass)
                if !user.has_permission(required_permission) {
                    continue;
                }

                // Send message to this user
                if user.tx.send((message.clone(), None)).is_err() {
                    disconnected.push(user.session_id);
                }
            }
        }

        self.remove_disconnected(disconnected).await;
    }

    /// Broadcast a user event (UserConnected/UserDisconnected) to users with user_list permission
    ///
    /// This method should be used for broadcasting UserConnected and UserDisconnected messages
    /// to ensure only users with the user_list permission receive these updates.
    ///
    /// Optionally excludes a specific session_id (e.g., to not send UserConnected to the connecting user).
    ///
    /// Automatically removes users whose channels have closed (disconnected connections).
    pub async fn broadcast_user_event(
        &self,
        message: ServerMessage,
        exclude_session_id: Option<u32>,
    ) {
        let mut disconnected = Vec::new();

        {
            let users = self.users.read().await;
            for user in users.values() {
                // Skip excluded session
                if let Some(excluded) = exclude_session_id
                    && user.session_id == excluded
                {
                    continue;
                }

                // Check if user has user_list permission (uses cached permissions, admin bypass)
                if !user.has_permission(Permission::UserList) {
                    continue;
                }

                // Send message to this user
                if user.tx.send((message.clone(), None)).is_err() {
                    disconnected.push(user.session_id);
                }
            }
        }

        self.remove_disconnected(disconnected).await;
    }

    /// Broadcast ServerInfoUpdated to all connected users
    ///
    /// All users receive the full server info including connection/transfer limits.
    /// file_reindex_interval is only sent to admins or users with file_reindex permission.
    /// This is called when server configuration is updated via ServerUpdate.
    ///
    /// Automatically removes users whose channels have closed (disconnected connections).
    pub async fn broadcast_server_info_updated(&self, values: ServerInfoValues) {
        let mut disconnected = Vec::new();

        {
            let users = self.users.read().await;
            for user in users.values() {
                let options = ServerInfoOptions {
                    is_admin: user.is_admin,
                    has_file_reindex: user.has_permission(Permission::FileReindex),
                    has_chat_join: user.has_permission(Permission::ChatJoin),
                    include_image: true,
                };

                let server_info = build_server_info(&values, &options);
                let message = ServerMessage::ServerInfoUpdated { server_info };

                if user.tx.send((message, None)).is_err() {
                    disconnected.push(user.session_id);
                }
            }
        }

        self.remove_disconnected(disconnected).await;
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::net::SocketAddr;

    use tokio::sync::mpsc;

    use super::*;
    use crate::users::user::NewSessionParams;

    fn session_params(
        user_id: i64,
        username: &str,
        tx: mpsc::UnboundedSender<(ServerMessage, Option<nexus_common::framing::MessageId>)>,
    ) -> NewSessionParams {
        NewSessionParams {
            session_id: 0,
            user_id,
            username: username.to_string(),
            is_admin: false,
            is_shared: false,
            permissions: HashSet::new(),
            address: "127.0.0.1:12345".parse::<SocketAddr>().unwrap(),
            created_at: 0,
            tx,
            features: vec![],
            locale: "en".to_string(),
            avatar: None,
            nickname: username.to_string(),
            is_away: false,
            status: None,
            group_id: None,
            group_name: None,
            bandwidth_weight_override: None,
            last_activity: std::time::Instant::now(),
            bandwidth_weight: 1,
        }
    }

    /// `broadcast_to_user_id` delivers to every session of the given
    /// user_id (multi-session case) and skips other users. Pins the
    /// PK-keyed dispatch contract that the rename-race fix depends on.
    #[tokio::test]
    async fn test_broadcast_to_user_id_hits_all_sessions_of_one_user_only() {
        let manager = UserManager::new();

        // user_id=1 has two sessions (test fixture exercising the
        // multi-session-per-user fan-out — not a production state for
        // a regular account); user_id=2 has one.
        let (tx_a1, mut rx_a1) = mpsc::unbounded_channel();
        let (tx_a2, mut rx_a2) = mpsc::unbounded_channel();
        let (tx_b, mut rx_b) = mpsc::unbounded_channel();
        manager
            .add_user(session_params(1, "alice_one", tx_a1))
            .await
            .unwrap();
        manager
            .add_user(session_params(1, "alice_two", tx_a2))
            .await
            .unwrap();
        manager
            .add_user(session_params(2, "bob", tx_b))
            .await
            .unwrap();

        let payload = ServerMessage::Pong;
        manager.broadcast_to_user_id(1, &payload).await;

        assert!(rx_a1.try_recv().is_ok(), "alice's first session received");
        assert!(rx_a2.try_recv().is_ok(), "alice's second session received");
        assert!(
            rx_b.try_recv().is_err(),
            "bob (different user_id) must not receive"
        );
    }
}
