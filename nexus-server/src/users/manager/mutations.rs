//! Mutation methods for UserManager

use std::collections::HashSet;

use ipnet::IpNet;
use nexus_common::protocol::ServerMessage;

use super::UserManager;
use crate::db::Permission;
use crate::users::user::{NewSessionParams, UserSession};

/// Information about a disconnected session, used for broadcasting UserDisconnected
pub struct DisconnectedSession {
    pub session_id: u32,
    pub nickname: String,
}

/// Error returned when adding a user fails
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddUserError {
    /// The requested nickname is already in use by another session
    NicknameInUse,
}

impl UserManager {
    /// Add a new user and return their assigned session ID
    ///
    /// For shared accounts with nicknames, this performs an atomic check to ensure
    /// the nickname is not already in use by another session or matching a logged-in
    /// username. This prevents race conditions where two users could claim the same
    /// nickname simultaneously.
    ///
    /// # Defense in Depth
    ///
    /// The login handler performs a non-atomic pre-check via `is_nickname_in_use()`
    /// before calling this method. This provides two benefits:
    ///
    /// 1. **Early rejection**: Most conflicts are caught without acquiring the write lock,
    ///    reducing contention for legitimate requests.
    ///
    /// 2. **Atomic guarantee**: This method's check while holding the write lock prevents
    ///    race conditions where two simultaneous logins could both pass the pre-check
    ///    but only one should succeed.
    ///
    /// Both checks are necessary: the pre-check for performance, the atomic check for correctness.
    ///
    /// # Errors
    ///
    /// Returns `AddUserError::NicknameInUse` if the nickname is already taken by
    /// another session (shared or regular).
    pub async fn add_user(&self, mut params: NewSessionParams) -> Result<u32, AddUserError> {
        // Acquire write lock first to ensure atomicity of nickname check + insert
        let mut users = self.users.write().await;

        // For shared accounts, check nickname uniqueness while holding the lock
        // (Regular accounts have nickname == username, so this check is redundant for them,
        // but we do it anyway for consistency)
        if params.is_shared {
            let nickname_lower = params.nickname.to_lowercase();

            for user in users.values() {
                // Check against existing nicknames (all sessions have nicknames now)
                if user.nickname.to_lowercase() == nickname_lower {
                    return Err(AddUserError::NicknameInUse);
                }
            }
        }

        // Nickname is unique (or not a shared account), proceed with adding
        let session_id = self.next_session_id();
        params.session_id = session_id;
        let user = UserSession::new(params);
        users.insert(session_id, user);

        Ok(session_id)
    }

    /// Remove a user by session ID
    pub async fn remove_user(&self, session_id: u32) -> Option<UserSession> {
        let mut users = self.users.write().await;
        users.remove(&session_id)
    }

    /// Remove a user and broadcast UserDisconnected to other clients
    ///
    /// This is a convenience method that combines `remove_user()` with broadcasting
    /// `UserDisconnected` to all users with the `user_list` permission. Use this
    /// for normal disconnects, kicks, account deletion, and account disable.
    ///
    /// For ban disconnects, use `disconnect_sessions_by_ip()` or
    /// `disconnect_sessions_in_range()` instead, as those need to send a custom
    /// message to the disconnected user before removing them.
    pub async fn remove_user_and_broadcast(&self, session_id: u32) -> Option<UserSession> {
        if let Some(user) = self.remove_user(session_id).await {
            self.broadcast_user_event(
                ServerMessage::UserDisconnected {
                    session_id,
                    nickname: user.nickname.clone(),
                },
                Some(session_id),
            )
            .await;
            Some(user)
        } else {
            None
        }
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

    /// Update admin status for a user by database user ID
    /// Returns the number of sessions updated
    pub async fn update_admin_status(&self, db_user_id: i64, is_admin: bool) -> usize {
        let mut users = self.users.write().await;
        let mut count = 0;

        for user in users.values_mut() {
            if user.db_user_id == db_user_id {
                user.is_admin = is_admin;
                count += 1;
            }
        }

        count
    }

    /// Update cached permissions for a user by database user ID
    /// Returns the number of sessions updated
    pub async fn update_permissions(
        &self,
        db_user_id: i64,
        permissions: HashSet<Permission>,
    ) -> usize {
        let mut users = self.users.write().await;
        let mut count = 0;

        for user in users.values_mut() {
            if user.db_user_id == db_user_id {
                user.permissions = permissions.clone();
                count += 1;
            }
        }

        count
    }

    /// Set status and away flag for a session (by session_id)
    /// Returns the updated session if found
    pub async fn set_status(
        &self,
        session_id: u32,
        is_away: bool,
        status: Option<String>,
    ) -> Option<UserSession> {
        let mut users = self.users.write().await;

        if let Some(user) = users.get_mut(&session_id) {
            user.is_away = is_away;
            user.status = status;
            Some(user.clone())
        } else {
            None
        }
    }

    /// Disconnect all sessions from a given IP address
    ///
    /// Builds a disconnect message for each session using the provided function,
    /// which receives the user's locale to generate a properly localized message.
    /// Used by the ban system to disconnect users when their IP is banned.
    ///
    /// Returns information about disconnected sessions so the caller can broadcast
    /// UserDisconnected messages to update other clients' user lists.
    pub async fn disconnect_sessions_by_ip<F>(
        &self,
        ip: &str,
        build_message: F,
    ) -> Vec<DisconnectedSession>
    where
        F: Fn(&str) -> ServerMessage,
    {
        // First, collect session IDs to disconnect
        let session_ids: Vec<u32> = {
            let users = self.users.read().await;
            users
                .values()
                .filter(|u| u.address.ip().to_string() == ip)
                .map(|u| u.session_id)
                .collect()
        };

        if session_ids.is_empty() {
            return Vec::new();
        }

        // Send disconnect message to each session and remove them
        let mut users = self.users.write().await;
        let mut disconnected = Vec::new();

        for session_id in session_ids {
            if let Some(user) = users.remove(&session_id) {
                // Build message with user's locale and send
                // (ignore send errors - channel may already be closed)
                let message = build_message(&user.locale);
                let _ = user.tx.send((message, None));
                disconnected.push(DisconnectedSession {
                    session_id,
                    nickname: user.nickname.clone(),
                });
            }
        }

        disconnected
    }

    /// Disconnect all sessions from IPs within a given CIDR range
    ///
    /// Builds a disconnect message for each session using the provided function,
    /// which receives the user's locale to generate a properly localized message.
    /// Used by the ban system to disconnect users when a CIDR range is banned.
    ///
    /// Returns information about disconnected sessions so the caller can broadcast
    /// UserDisconnected messages to update other clients' user lists.
    pub async fn disconnect_sessions_in_range<F>(
        &self,
        range: &IpNet,
        build_message: F,
    ) -> Vec<DisconnectedSession>
    where
        F: Fn(&str) -> ServerMessage,
    {
        // First, collect session IDs to disconnect
        let session_ids: Vec<u32> = {
            let users = self.users.read().await;
            users
                .values()
                .filter(|u| range.contains(&u.address.ip()))
                .map(|u| u.session_id)
                .collect()
        };

        if session_ids.is_empty() {
            return Vec::new();
        }

        // Send disconnect message to each session and remove them
        let mut users = self.users.write().await;
        let mut disconnected = Vec::new();

        for session_id in session_ids {
            if let Some(user) = users.remove(&session_id) {
                // Build message with user's locale and send
                // (ignore send errors - channel may already be closed)
                let message = build_message(&user.locale);
                let _ = user.tx.send((message, None));
                disconnected.push(DisconnectedSession {
                    session_id,
                    nickname: user.nickname.clone(),
                });
            }
        }

        disconnected
    }
}
