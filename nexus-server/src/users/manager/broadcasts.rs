//! Broadcast methods for UserManager

use nexus_common::names::fold_name;
use nexus_common::protocol::ServerMessage;

use crate::handlers::{ServerInfoOptions, ServerInfoValues, build_server_info};

use super::UserManager;
use crate::db::Permission;

impl UserManager {
    /// Send a message to a specific session. Returns false if the session
    /// doesn't exist or the channel is closed.
    pub async fn send_to_session(&self, session_id: u32, message: ServerMessage) -> bool {
        let users = self.users.read().await;
        if let Some(user) = users.get(&session_id) {
            user.tx.send_message(message, None).is_ok()
        } else {
            false
        }
    }

    /// Broadcast to all connected users. Removes users whose channels have
    /// closed and notifies user_list clients of the disconnect.
    pub async fn broadcast(&self, message: ServerMessage) {
        let mut disconnected = Vec::new();

        {
            let users = self.users.read().await;
            for user in users.values() {
                if user.tx.send_message(message.clone(), None).is_err() {
                    disconnected.push(user.session_id);
                }
            }
        }

        self.remove_disconnected(disconnected).await;
    }

    /// Broadcast to users who both requested `feature` (client preference)
    /// and hold `required_permission` (server enforcement). Optionally excludes
    /// one session (e.g. the originator who already got a typed response).
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
                if let Some(excluded) = exclude_session_id
                    && user.session_id == excluded
                {
                    continue;
                }

                if !user.has_feature(feature) {
                    continue;
                }

                // Cached permissions, admin bypass.
                if !user.has_permission(required_permission) {
                    continue;
                }

                if user.tx.send_message(message.clone(), None).is_err() {
                    disconnected.push(user.session_id);
                }
            }
        }

        self.remove_disconnected(disconnected).await;
    }

    /// Broadcast to every session of `user_id`. Keyed on the immutable PK so a
    /// concurrent rename can't make the lookup miss sessions.
    pub async fn broadcast_to_user_id(&self, user_id: i64, message: &ServerMessage) {
        let mut disconnected = Vec::new();

        {
            let users = self.users.read().await;
            for user in users.values() {
                if user.user_id == user_id && user.tx.send_message(message.clone(), None).is_err() {
                    disconnected.push(user.session_id);
                }
            }
        }

        self.remove_disconnected(disconnected).await;
    }

    /// Broadcast to all sessions with `nickname` (case-insensitive). Regular
    /// accounts (nickname == username) reach every session; shared accounts have
    /// unique per-session nicknames so only that one session matches.
    pub async fn broadcast_to_nickname(&self, nickname: &str, message: &ServerMessage) {
        let mut disconnected = Vec::new();

        let nickname_lower = fold_name(nickname);

        {
            let users = self.users.read().await;
            for user in users.values() {
                if fold_name(&user.nickname) == nickname_lower
                    && user.tx.send_message(message.clone(), None).is_err()
                {
                    disconnected.push(user.session_id);
                }
            }
        }

        self.remove_disconnected(disconnected).await;
    }

    /// Broadcast to all users holding `required_permission` (e.g. topic updates
    /// to those allowed to see them).
    pub async fn broadcast_to_permission(
        &self,
        message: ServerMessage,
        required_permission: Permission,
    ) {
        let mut disconnected = Vec::new();

        {
            let users = self.users.read().await;
            for user in users.values() {
                // Cached permissions, admin bypass.
                if !user.has_permission(required_permission) {
                    continue;
                }

                if user.tx.send_message(message.clone(), None).is_err() {
                    disconnected.push(user.session_id);
                }
            }
        }

        self.remove_disconnected(disconnected).await;
    }

    /// Broadcast a UserConnected/UserDisconnected event to user_list holders.
    /// Optionally excludes one session (e.g. the connecting user).
    pub async fn broadcast_user_event(
        &self,
        message: ServerMessage,
        exclude_session_id: Option<u32>,
    ) {
        let mut disconnected = Vec::new();

        {
            let users = self.users.read().await;
            for user in users.values() {
                if let Some(excluded) = exclude_session_id
                    && user.session_id == excluded
                {
                    continue;
                }

                // Cached permissions, admin bypass.
                if !user.has_permission(Permission::UserList) {
                    continue;
                }

                if user.tx.send_message(message.clone(), None).is_err() {
                    disconnected.push(user.session_id);
                }
            }
        }

        self.remove_disconnected(disconnected).await;
    }

    /// Broadcast ServerInfoUpdated to all users. Everyone gets full server info;
    /// file_reindex_interval is included only for admins / file_reindex holders.
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

                if user.tx.send_message(message, None).is_err() {
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

    use super::*;
    use crate::users::user::{ConnectionWriter, NewSessionParams};

    fn session_params(user_id: i64, username: &str, tx: ConnectionWriter) -> NewSessionParams {
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

    /// Pins the PK-keyed dispatch contract the rename-race fix depends on:
    /// `broadcast_to_user_id` hits every session of the target user_id only.
    #[tokio::test]
    async fn test_broadcast_to_user_id_hits_all_sessions_of_one_user_only() {
        let manager = UserManager::new();

        // user_id=1 has two sessions (fixture-only multi-session fan-out, not a
        // production state for a regular account); user_id=2 has one.
        let (tx_a1, mut rx_a1) = ConnectionWriter::channel();
        let (tx_a2, mut rx_a2) = ConnectionWriter::channel();
        let (tx_b, mut rx_b) = ConnectionWriter::channel();
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
