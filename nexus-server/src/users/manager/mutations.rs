//! Mutation methods for UserManager

use std::collections::HashSet;

use super::UserManager;
use crate::db::Permission;
use crate::users::user::{NewSessionParams, UserSession};

/// Error returned when adding a user fails
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddUserError {
    /// The requested nickname is already in use by another session
    NicknameInUse,
    /// The requested nickname matches an existing logged-in username
    NicknameMatchesUsername,
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
    /// another shared account session.
    ///
    /// Returns `AddUserError::NicknameMatchesUsername` if the nickname matches
    /// a currently logged-in regular user's username.
    pub async fn add_user(&self, mut params: NewSessionParams) -> Result<u32, AddUserError> {
        // Acquire write lock first to ensure atomicity of nickname check + insert
        let mut users = self.users.write().await;

        // For shared accounts, check nickname uniqueness while holding the lock
        if let Some(ref nickname) = params.nickname {
            let nickname_lower = nickname.to_lowercase();

            for user in users.values() {
                // Check against existing nicknames (other shared account sessions)
                if let Some(ref existing_nickname) = user.nickname
                    && existing_nickname.to_lowercase() == nickname_lower
                {
                    return Err(AddUserError::NicknameInUse);
                }

                // Check against logged-in usernames (regular accounts)
                // This ensures a shared account can't use a nickname that matches
                // a logged-in regular user's username
                if user.username.to_lowercase() == nickname_lower {
                    return Err(AddUserError::NicknameMatchesUsername);
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
}
