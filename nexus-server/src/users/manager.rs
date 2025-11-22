//! User manager for tracking connected users

use super::user::User;
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
        username: String,
        session_id: String,
        address: SocketAddr,
        tx: mpsc::UnboundedSender<ServerMessage>,
        features: Vec<String>,
    ) -> u32 {
        let mut next_id = self.next_id.write().await;
        let id = *next_id;
        *next_id += 1;
        drop(next_id);

        let user = User::new(id, username, session_id, address, tx, features);
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
    // pub async fn get_user_by_session(&self, session_id: &str) -> Option<User> {
    //     let users = self.users.read().await;
    //     users.values().find(|u| u.session_id == session_id).cloned()
    // }

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
            // Ignore send errors (user might have disconnected)
            let _ = user.tx.send(message.clone());
        }
    }

    /// Broadcast a message to all users except one
    pub async fn broadcast_except(&self, exclude_id: u32, message: ServerMessage) {
        let users = self.users.read().await;
        for user in users.values() {
            if user.id != exclude_id {
                // Ignore send errors (user might have disconnected)
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

    /// Broadcast a message to all users with a specific feature
    pub async fn broadcast_to_feature(&self, feature: &str, message: ServerMessage) {
        let users = self.users.read().await;
        for user in users.values() {
            if user.has_feature(feature) {
                // Ignore send errors (user might have disconnected)
                let _ = user.tx.send(message.clone());
            }
        }
    }
}

impl Default for UserManager {
    fn default() -> Self {
        Self::new()
    }
}
