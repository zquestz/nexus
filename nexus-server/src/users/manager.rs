//! User manager for tracking connected users

use super::user::User;
use crate::db::{Permission, UserDb};
use nexus_common::protocol::ServerMessage;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};

/// Manages all connected users
#[derive(Debug, Clone)]
pub struct UserManager {
    users: Arc<RwLock<HashMap<u32, User>>>,
    next_id: Arc<RwLock<u32>>,
}

impl UserManager {
    /// Create a new user manager
    pub fn new() -> Self {
        Self {
            users: Arc::new(RwLock::new(HashMap::new())),
            next_id: Arc::new(RwLock::new(1)),
        }
    }

    /// Add a new user and return their assigned ID
    pub async fn add_user(
        &self,
        db_user_id: i64,
        username: String,
        address: SocketAddr,
        created_at: i64,
        tx: mpsc::UnboundedSender<ServerMessage>,
        features: Vec<String>,
    ) -> u32 {
        let mut next_id = self.next_id.write().await;
        let id = *next_id;
        *next_id += 1;
        drop(next_id);

        let user = User::new(id, db_user_id, username, address, created_at, tx, features);
        let mut users = self.users.write().await;
        users.insert(id, user);

        id
    }

    /// Remove a user by ID
    pub async fn remove_user(&self, id: u32) -> Option<User> {
        let mut users = self.users.write().await;
        users.remove(&id)
    }

    /// Get a user by ID
    pub async fn get_user(&self, id: u32) -> Option<User> {
        let users = self.users.read().await;
        users.get(&id).cloned()
    }

    /// Get a user by session ID

    /// Get all connected users
    pub async fn get_all_users(&self) -> Vec<User> {
        let users = self.users.read().await;
        users.values().cloned().collect()
    }

    /// Get the count of connected users
    // pub async fn user_count(&self) -> usize {
    //     let users = self.users.read().await;
    //     users.len()
    // }

    /// Broadcast a message to all connected users
    pub async fn broadcast(&self, message: ServerMessage) {
        let users = self.users.read().await;
        for user in users.values() {
            // Silently ignore send errors (user might have disconnected)
            let _ = user.tx.send(message.clone());
        }
    }

    /// Broadcast a message to all users except one
    pub async fn broadcast_except(&self, exclude_id: u32, message: ServerMessage) {
        let users = self.users.read().await;
        for user in users.values() {
            if user.session_id != exclude_id {
                // Silently ignore send errors (user might have disconnected)
                let _ = user.tx.send(message.clone());
            }
        }
    }

    /// Send a message to a specific user by ID
    // pub async fn send_to_user(&self, user_id: u32, message: ServerMessage) -> bool {
    //     let users = self.users.read().await;
    //     if let Some(user) = users.get(&user_id) {
    //         user.tx.send(message).is_ok()
    //     } else {
    //         false
    //     }
    // }

    /// Broadcast a message to all users with a specific feature and permission
    pub async fn broadcast_to_feature(
        &self,
        feature: &str,
        message: ServerMessage,
        user_db: &UserDb,
        required_permission: Permission,
    ) {
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
                    continue; // Skip this user on error
                }
            };

            if !has_perm {
                continue;
            }

            // Send message to this user (silently ignore if channel is closed)
            let _ = user.tx.send(message.clone());
        }
    }
}

impl Default for UserManager {
    fn default() -> Self {
        Self::new()
    }
}
