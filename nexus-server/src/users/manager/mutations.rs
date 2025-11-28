//! Mutation methods for UserManager

use super::UserManager;
use crate::users::user::User;
use nexus_common::protocol::ServerMessage;
use std::net::SocketAddr;
use tokio::sync::mpsc;

impl UserManager {
    /// Add a new user and return their assigned session ID
    pub async fn add_user(
        &self,
        db_user_id: i64,
        username: String,
        address: SocketAddr,
        created_at: i64,
        tx: mpsc::UnboundedSender<ServerMessage>,
        features: Vec<String>,
        locale: String,
    ) -> u32 {
        let mut next_id = self.next_id.write().await;
        let session_id = *next_id;
        *next_id += 1;
        drop(next_id);

        let user = User::new(
            session_id, db_user_id, username, address, created_at, tx, features, locale,
        );
        let mut users = self.users.write().await;
        users.insert(session_id, user);

        session_id
    }

    /// Remove a user by session ID
    pub async fn remove_user(&self, session_id: u32) -> Option<User> {
        let mut users = self.users.write().await;
        users.remove(&session_id)
    }

    /// Update username for a user by database user ID
    /// Returns the number of sessions updated
    pub async fn update_username(&self, db_user_id: i64, new_username: String) -> usize {
        let mut users = self.users.write().await;
        let mut count = 0;

        for user in users.values_mut() {
            if user.db_user_id == db_user_id {
                user.username = new_username.clone();
                count += 1;
            }
        }

        count
    }
}
