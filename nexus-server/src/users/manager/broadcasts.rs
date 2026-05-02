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

    /// Broadcast a message to all sessions of a specific user (by username, case-insensitive)
    ///
    /// This is useful for multi-session scenarios where the same user is logged in
    /// from multiple devices/connections and all sessions need to be notified.
    ///
    /// Automatically removes users whose channels have closed (disconnected connections).
    pub async fn broadcast_to_username(&self, username: &str, message: &ServerMessage) {
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
