//! Broadcast methods for UserManager

use super::UserManager;
use crate::db::{Permission, UserDb};
use nexus_common::protocol::ServerMessage;

impl UserManager {
    /// Broadcast a message to all connected users
    ///
    /// Automatically removes users whose channels have closed (disconnected connections).
    pub async fn broadcast(&self, message: ServerMessage) {
        let mut disconnected = Vec::new();
        
        {
            let users = self.users.read().await;
            for user in users.values() {
                if user.tx.send(message.clone()).is_err() {
                    disconnected.push(user.session_id);
                }
            }
        }
        
        self.remove_disconnected(disconnected).await;
    }

    /// Broadcast a message to all users except one
    ///
    /// Automatically removes users whose channels have closed (disconnected connections).
    pub async fn broadcast_except(&self, exclude_session_id: u32, message: ServerMessage) {
        let mut disconnected = Vec::new();
        
        {
            let users = self.users.read().await;
            for user in users.values() {
                if user.session_id != exclude_session_id {
                    if user.tx.send(message.clone()).is_err() {
                        disconnected.push(user.session_id);
                    }
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

                // Check if user has the required permission
                let has_perm = match user_db
                    .has_permission(user.db_user_id, required_permission)
                    .await
                {
                    Ok(has) => has,
                    Err(e) => {
                        eprintln!("Error checking permission for {}: {}", user.username, e);
                        continue;
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
        
        self.remove_disconnected(disconnected).await;
    }

    /// Broadcast a message to all sessions of a specific user (by username)
    ///
    /// This is useful for multi-session scenarios where the same user is logged in
    /// from multiple devices/connections and all sessions need to be notified.
    ///
    /// Automatically removes users whose channels have closed (disconnected connections).
    pub async fn broadcast_to_username(&self, username: &str, message: &ServerMessage) {
        let mut disconnected = Vec::new();
        
        {
            let users = self.users.read().await;
            for user in users.values() {
                if user.username == username {
                    if user.tx.send(message.clone()).is_err() {
                        disconnected.push(user.session_id);
                    }
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
        user_db: &UserDb,
        required_permission: Permission,
    ) {
        let mut disconnected = Vec::new();
        
        {
            let users = self.users.read().await;
            for user in users.values() {
                // Check if user has the required permission
                let has_perm = match user_db
                    .has_permission(user.db_user_id, required_permission)
                    .await
                {
                    Ok(has) => has,
                    Err(e) => {
                        eprintln!("Error checking permission for {}: {}", user.username, e);
                        continue;
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
        
        self.remove_disconnected(disconnected).await;
    }
}