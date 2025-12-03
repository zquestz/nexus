//! Broadcast methods for UserManager

use super::UserManager;
use crate::constants::*;
use crate::db::{Permission, UserDb};
use nexus_common::protocol::ServerMessage;

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
                if user.tx.send(message.clone()).is_err() {
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

                // Check if user has the required permission (admin bypass)
                let has_perm = if user.is_admin {
                    true
                } else {
                    match user_db
                        .has_permission(user.db_user_id, required_permission)
                        .await
                    {
                        Ok(has) => has,
                        Err(e) => {
                            eprintln!("{}{}: {}", ERR_CHECK_PERMISSION, user.username, e);
                            continue;
                        }
                    }
                };

                if !has_perm {
                    continue;
                }

                // Send message to this user
                if user.tx.send(message.clone()).is_err() {
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
                    && user.tx.send(message.clone()).is_err()
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
                // Check if user has the required permission (admin bypass)
                let has_perm = if user.is_admin {
                    true
                } else {
                    match user_db
                        .has_permission(user.db_user_id, required_permission)
                        .await
                    {
                        Ok(has) => has,
                        Err(e) => {
                            eprintln!("{}{}: {}", ERR_CHECK_PERMISSION, user.username, e);
                            continue;
                        }
                    }
                };

                if !has_perm {
                    continue;
                }

                // Send message to this user
                if user.tx.send(message.clone()).is_err() {
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

                // Check if user has user_list permission (admin bypass)
                let has_permission = if user.is_admin {
                    true
                } else {
                    match user_db
                        .has_permission(user.db_user_id, Permission::UserList)
                        .await
                    {
                        Ok(has) => has,
                        Err(e) => {
                            eprintln!("{}{}: {}", ERR_CHECK_USER_LIST_PERMISSION, user.username, e);
                            continue;
                        }
                    }
                };

                if !has_permission {
                    continue;
                }

                // Send message to this user
                if user.tx.send(message.clone()).is_err() {
                    disconnected.push(user.session_id);
                }
            }
        }

        self.remove_disconnected(disconnected, user_db).await;
    }
}
