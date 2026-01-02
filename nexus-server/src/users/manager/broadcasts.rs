//! Broadcast methods for UserManager

use nexus_common::protocol::{ServerInfo, ServerMessage};

use super::UserManager;
use crate::db::{Permission, UserDb};

/// Parameters for broadcasting server info updates
pub struct ServerInfoBroadcastParams {
    pub name: String,
    pub description: String,
    pub version: String,
    pub max_connections_per_ip: u32,
    pub max_transfers_per_ip: u32,
    pub image: String,
    pub transfer_port: u16,
}

impl UserManager {
    /// Broadcast a message to all connected users with proper disconnect notification
    ///
    /// Automatically removes users whose channels have closed and notifies other clients
    /// with user_list permission about the disconnection.
    pub async fn broadcast(&self, message: ServerMessage, user_db: &UserDb) {
        let mut disconnected = Vec::new();

        {
            let users = self.users.read().await;
            for user in users.values() {
                if user.tx.send((message.clone(), None)).is_err() {
                    disconnected.push(user.session_id);
                }
            }
        }

        self.remove_disconnected(disconnected, user_db).await;
    }

    /// Broadcast a message to all users with a specific feature and permission
    ///
    /// This method checks both that the user has requested the feature (client preference)
    /// and that they have permission to receive it (server enforcement).
    ///
    /// Automatically removes users whose channels have closed (disconnected connections).
    pub async fn broadcast_to_feature(
        &self,
        feature: &str,
        message: ServerMessage,
        user_db: &UserDb,
        required_permission: Permission,
    ) {
        let mut disconnected = Vec::new();

        {
            let users = self.users.read().await;
            for user in users.values() {
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

        self.remove_disconnected(disconnected, user_db).await;
    }

    /// Broadcast a message to all sessions of a specific user (by username, case-insensitive)
    ///
    /// This is useful for multi-session scenarios where the same user is logged in
    /// from multiple devices/connections and all sessions need to be notified.
    ///
    /// Automatically removes users whose channels have closed (disconnected connections).
    pub async fn broadcast_to_username(
        &self,
        username: &str,
        message: &ServerMessage,
        user_db: &UserDb,
    ) {
        let mut disconnected = Vec::new();

        let username_lower = username.to_lowercase();

        {
            let users = self.users.read().await;
            for user in users.values() {
                if user.username.to_lowercase() == username_lower
                    && user.tx.send((message.clone(), None)).is_err()
                {
                    disconnected.push(user.session_id);
                }
            }
        }

        self.remove_disconnected(disconnected, user_db).await;
    }

    /// Broadcast a message to all sessions with a specific nickname (case-insensitive)
    ///
    /// This works correctly for both regular and shared accounts:
    /// - Regular accounts: nickname == username, so all sessions of the user receive the message
    /// - Shared accounts: each session has a unique nickname, so only that session receives it
    ///
    /// Automatically removes users whose channels have closed (disconnected connections).
    pub async fn broadcast_to_nickname(
        &self,
        nickname: &str,
        message: &ServerMessage,
        user_db: &UserDb,
    ) {
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

        self.remove_disconnected(disconnected, user_db).await;
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
        user_db: &UserDb,
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

        self.remove_disconnected(disconnected, user_db).await;
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
        user_db: &UserDb,
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

        self.remove_disconnected(disconnected, user_db).await;
    }

    /// Broadcast ServerInfoUpdated to all connected users
    ///
    /// All users receive the full server info including connection/transfer limits.
    /// This is called when server configuration is updated via ServerUpdate.
    pub async fn broadcast_server_info_updated(&self, params: ServerInfoBroadcastParams) {
        let users = self.users.read().await;
        for user in users.values() {
            let server_info = ServerInfo {
                name: Some(params.name.clone()),
                description: Some(params.description.clone()),
                version: Some(params.version.clone()),
                max_connections_per_ip: Some(params.max_connections_per_ip),
                max_transfers_per_ip: Some(params.max_transfers_per_ip),
                image: Some(params.image.clone()),
                transfer_port: Some(params.transfer_port),
            };

            let message = ServerMessage::ServerInfoUpdated { server_info };

            // Ignore send errors - user will be cleaned up by their connection handler
            let _ = user.tx.send((message, None));
        }
    }
}
