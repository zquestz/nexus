//! UserUpdate message handler

use std::io;

use tokio::io::AsyncWrite;

use nexus_common::is_shared_account_permission;
use nexus_common::protocol::{ChatInfo, ServerInfo, ServerMessage, UserInfo};
use nexus_common::validators::{self, PasswordError, PermissionsError, UsernameError};

use crate::constants::DEFAULT_LOCALE;

#[cfg(test)]
use super::testing::DEFAULT_TEST_LOCALE;
use super::{
    HandlerContext, err_account_disabled_by_admin, err_authentication,
    err_cannot_change_guest_password, err_cannot_demote_last_admin, err_cannot_disable_last_admin,
    err_cannot_edit_admin, err_cannot_edit_self, err_cannot_rename_guest,
    err_current_password_incorrect, err_current_password_required, err_database, err_not_logged_in,
    err_password_empty, err_password_too_long, err_permission_denied,
    err_permissions_contains_newlines, err_permissions_empty_permission,
    err_permissions_invalid_characters, err_permissions_permission_too_long,
    err_permissions_too_many, err_shared_cannot_change_password, err_shared_invalid_permissions,
    err_update_failed, err_user_not_found, err_username_empty, err_username_exists,
    err_username_invalid, err_username_too_long,
};
use crate::db::sql::GUEST_USERNAME;
use crate::db::{Permission, Permissions, hash_password, verify_password};

/// User update request parameters
pub struct UserUpdateRequest {
    pub username: String,
    pub current_password: Option<String>,
    pub requested_username: Option<String>,
    pub requested_password: Option<String>,
    pub requested_is_admin: Option<bool>,
    pub requested_enabled: Option<bool>,
    pub requested_permissions: Option<Vec<String>>,
    pub session_id: Option<u32>,
}

/// Handle a user update request from the client
pub async fn handle_user_update<W>(
    request: UserUpdateRequest,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    // Verify authentication first (before revealing validation errors to unauthenticated users)
    let Some(requesting_session_id) = request.session_id else {
        eprintln!("UserUpdate request from {} without login", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("UserUpdate"))
            .await;
    };

    // Validate target username format
    if let Err(e) = validators::validate_username(&request.username) {
        let error_msg = match e {
            UsernameError::Empty => err_username_empty(ctx.locale),
            UsernameError::TooLong => {
                err_username_too_long(ctx.locale, validators::MAX_USERNAME_LENGTH)
            }
            UsernameError::InvalidCharacters => err_username_invalid(ctx.locale),
        };
        let response = ServerMessage::UserUpdateResponse {
            success: false,
            error: Some(error_msg),
            username: None,
        };
        return ctx.send_message(&response).await;
    }

    // Get requesting user from session
    let requesting_user = match ctx
        .user_manager
        .get_user_by_session_id(requesting_session_id)
        .await
    {
        Some(u) => u,
        None => {
            return ctx
                .send_error_and_disconnect(&err_authentication(ctx.locale), Some("UserUpdate"))
                .await;
        }
    };

    // Check if this is a self-edit (user changing their own password)
    let is_self_edit = request.username.to_lowercase() == requesting_user.username.to_lowercase();

    if is_self_edit {
        // Shared accounts cannot change their own password
        if requesting_user.is_shared {
            let response = ServerMessage::UserUpdateResponse {
                success: false,
                error: Some(err_shared_cannot_change_password(ctx.locale)),
                username: None,
            };
            return ctx.send_message(&response).await;
        }

        // Self-edit: only password change is allowed
        // Reject if trying to change anything other than password
        if request.requested_username.is_some()
            || request.requested_is_admin.is_some()
            || request.requested_enabled.is_some()
            || request.requested_permissions.is_some()
        {
            let response = ServerMessage::UserUpdateResponse {
                success: false,
                error: Some(err_cannot_edit_self(ctx.locale)),
                username: None,
            };
            return ctx.send_message(&response).await;
        }

        // Password change requires current_password
        let Some(ref current_password) = request.current_password else {
            let response = ServerMessage::UserUpdateResponse {
                success: false,
                error: Some(err_current_password_required(ctx.locale)),
                username: None,
            };
            return ctx.send_message(&response).await;
        };

        // Verify current password against database
        let password_hash = match ctx.db.users.get_user_by_username(&request.username).await {
            Ok(Some(user)) => user.hashed_password,
            Ok(None) => {
                let response = ServerMessage::UserUpdateResponse {
                    success: false,
                    error: Some(err_user_not_found(ctx.locale, &request.username)),
                    username: None,
                };
                return ctx.send_message(&response).await;
            }
            Err(e) => {
                eprintln!("Database error getting user: {}", e);
                return ctx
                    .send_error_and_disconnect(&err_database(ctx.locale), Some("UserUpdate"))
                    .await;
            }
        };

        // Verify the current password
        match verify_password(current_password, &password_hash) {
            Ok(true) => {} // Password correct, continue
            Ok(false) => {
                let response = ServerMessage::UserUpdateResponse {
                    success: false,
                    error: Some(err_current_password_incorrect(ctx.locale)),
                    username: None,
                };
                return ctx.send_message(&response).await;
            }
            Err(e) => {
                eprintln!("Error verifying password: {}", e);
                return ctx
                    .send_error_and_disconnect(&err_database(ctx.locale), Some("UserUpdate"))
                    .await;
            }
        }
    } else {
        // Editing another user: check UserEdit permission
        if !requesting_user.has_permission(Permission::UserEdit) {
            eprintln!(
                "UserUpdate from {} (user: {}) without permission",
                ctx.peer_addr, requesting_user.username
            );
            return ctx
                .send_error(&err_permission_denied(ctx.locale), Some("UserUpdate"))
                .await;
        }

        // Prevent non-admins from editing admin users
        // Look up target user to check their admin status
        if !requesting_user.is_admin {
            match ctx.db.users.get_user_by_username(&request.username).await {
                Ok(Some(target_user)) if target_user.is_admin => {
                    eprintln!(
                        "UserUpdate from {} (user: {}) trying to edit admin user",
                        ctx.peer_addr, requesting_user.username
                    );
                    let response = ServerMessage::UserUpdateResponse {
                        success: false,
                        error: Some(err_cannot_edit_admin(ctx.locale)),
                        username: None,
                    };
                    return ctx.send_message(&response).await;
                }
                Ok(Some(_)) => {} // Target is not admin, proceed
                Ok(None) => {
                    let response = ServerMessage::UserUpdateResponse {
                        success: false,
                        error: Some(err_user_not_found(ctx.locale, &request.username)),
                        username: None,
                    };
                    return ctx.send_message(&response).await;
                }
                Err(e) => {
                    eprintln!("Database error getting target user: {}", e);
                    return ctx
                        .send_error_and_disconnect(&err_database(ctx.locale), Some("UserUpdate"))
                        .await;
                }
            }
        }
    }

    // Validate new username format if it's being changed
    if let Some(ref new_username) = request.requested_username
        && let Err(e) = validators::validate_username(new_username)
    {
        let error_msg = match e {
            UsernameError::Empty => err_username_empty(ctx.locale),
            UsernameError::TooLong => {
                err_username_too_long(ctx.locale, validators::MAX_USERNAME_LENGTH)
            }
            UsernameError::InvalidCharacters => err_username_invalid(ctx.locale),
        };
        let response = ServerMessage::UserUpdateResponse {
            success: false,
            error: Some(error_msg),
            username: None,
        };
        return ctx.send_message(&response).await;
    }

    // Prevent renaming the guest account
    if let Some(ref new_username) = request.requested_username
        && request.username.to_lowercase() == GUEST_USERNAME
        && new_username.to_lowercase() != GUEST_USERNAME
    {
        let response = ServerMessage::UserUpdateResponse {
            success: false,
            error: Some(err_cannot_rename_guest(ctx.locale)),
            username: None,
        };
        return ctx.send_message(&response).await;
    }

    // Prevent changing the guest account password
    if let Some(ref new_password) = request.requested_password
        && !new_password.trim().is_empty()
        && request.username.to_lowercase() == GUEST_USERNAME
    {
        let response = ServerMessage::UserUpdateResponse {
            success: false,
            error: Some(err_cannot_change_guest_password(ctx.locale)),
            username: None,
        };
        return ctx.send_message(&response).await;
    }

    // Note: Last admin protection is now handled atomically at the database level
    // in update_user() SQL query to prevent race conditions

    // Verify admin flag modification privilege (use is_admin from UserManager)
    // Skip for self-edit since we already rejected admin changes above
    if !is_self_edit && request.requested_is_admin.is_some() && !requesting_user.is_admin {
        return ctx
            .send_error(&err_permission_denied(ctx.locale), Some("UserUpdate"))
            .await;
    }

    // Fetch target user to check if they're a shared account (needed for permission validation)
    let target_user_account = match ctx.db.users.get_user_by_username(&request.username).await {
        Ok(Some(account)) => Some(account),
        Ok(None) => {
            let response = ServerMessage::UserUpdateResponse {
                success: false,
                error: Some(err_user_not_found(ctx.locale, &request.username)),
                username: None,
            };
            return ctx.send_message(&response).await;
        }
        Err(e) => {
            eprintln!("Database error getting target user: {}", e);
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some("UserUpdate"))
                .await;
        }
    };

    // Validate and parse requested permissions
    let parsed_permissions = if let Some(ref perm_strings) = request.requested_permissions {
        // For shared accounts, validate that only allowed permissions are requested
        if let Some(ref account) = target_user_account
            && account.is_shared
        {
            let forbidden: Vec<&str> = perm_strings
                .iter()
                .map(|s| s.as_str())
                .filter(|p| !is_shared_account_permission(p))
                .collect();

            if !forbidden.is_empty() {
                let response = ServerMessage::UserUpdateResponse {
                    success: false,
                    error: Some(err_shared_invalid_permissions(
                        ctx.locale,
                        &forbidden.join(", "),
                    )),
                    username: None,
                };
                return ctx.send_message(&response).await;
            }
        }

        // Validate permissions format first
        if let Err(e) = validators::validate_permissions(perm_strings) {
            let error_msg = match e {
                PermissionsError::TooMany => {
                    err_permissions_too_many(ctx.locale, nexus_common::PERMISSIONS_COUNT)
                }
                PermissionsError::EmptyPermission => err_permissions_empty_permission(ctx.locale),
                PermissionsError::PermissionTooLong => err_permissions_permission_too_long(
                    ctx.locale,
                    validators::MAX_PERMISSION_LENGTH,
                ),
                PermissionsError::ContainsNewlines => err_permissions_contains_newlines(ctx.locale),
                PermissionsError::InvalidCharacters => {
                    err_permissions_invalid_characters(ctx.locale)
                }
            };
            let response = ServerMessage::UserUpdateResponse {
                success: false,
                error: Some(error_msg),
                username: None,
            };
            return ctx.send_message(&response).await;
        }

        let mut perms = Permissions::new();
        for perm_str in perm_strings {
            if let Some(perm) = Permission::parse(perm_str) {
                // Check permission delegation authority (uses cached permissions, admin bypass built-in)
                if !requesting_user.has_permission(perm) {
                    eprintln!(
                        "UserUpdate from {} (user: {}) trying to set permission they don't have: {}",
                        ctx.peer_addr, requesting_user.username, perm_str
                    );
                    return ctx
                        .send_error(&err_permission_denied(ctx.locale), Some("UserUpdate"))
                        .await;
                }

                perms.permissions.insert(perm);
            } else {
                eprintln!("Warning: unknown permission '{}'", perm_str);
            }
        }

        // Apply permission merge logic for non-admins
        if !requesting_user.is_admin {
            // Get target user's account
            if let Ok(Some(target_account)) =
                ctx.db.users.get_user_by_username(&request.username).await
            {
                // Get target user's current permissions
                if let Ok(target_perms) = ctx.db.users.get_user_permissions(target_account.id).await
                {
                    // Start with an empty set for the final permissions
                    let mut final_perms = Permissions::new();

                    // Add all permissions from target that requesting user DOESN'T have
                    // (these are preserved and cannot be modified)
                    for target_perm in &target_perms.permissions {
                        if !requesting_user.has_permission(*target_perm) {
                            // Preserve this permission - requester can't modify it
                            final_perms.permissions.insert(*target_perm);
                        }
                    }

                    // Add all requested permissions that the requester DOES have
                    // (these are the ones the requester can control)
                    for requested_perm in &perms.permissions {
                        final_perms.permissions.insert(*requested_perm);
                    }

                    // Replace the requested permissions with the merged set
                    perms = final_perms;
                }
            }
        }

        Some(perms)
    } else {
        None
    };

    // Process password change request
    let requested_password_hash = if let Some(ref password) = request.requested_password {
        // Empty/whitespace password = no change
        if password.trim().is_empty() {
            None
        } else {
            // Validate password format
            if let Err(e) = validators::validate_password(password) {
                let error_msg = match e {
                    PasswordError::Empty => err_password_empty(ctx.locale),
                    PasswordError::TooLong => {
                        err_password_too_long(ctx.locale, validators::MAX_PASSWORD_LENGTH)
                    }
                };
                let response = ServerMessage::UserUpdateResponse {
                    success: false,
                    error: Some(error_msg),
                    username: None,
                };
                return ctx.send_message(&response).await;
            }
            match hash_password(password) {
                Ok(hash) => Some(hash),
                Err(e) => {
                    eprintln!("Database error updating user {}: {}", request.username, e);
                    return ctx
                        .send_error_and_disconnect(&err_database(ctx.locale), Some("UserUpdate"))
                        .await;
                }
            }
        }
    } else {
        None
    };

    // Note: Username validation is already done earlier, so no need to check for empty here

    // Get old username and admin status before update (to detect changes)
    // Check if target user is online to use cached data, otherwise fall back to DB
    let old_account = if let Some(online_user) = ctx
        .user_manager
        .get_session_by_username(&request.username)
        .await
    {
        Some((online_user.username.clone(), online_user.is_admin))
    } else {
        // User is offline - must check DB
        match ctx.db.users.get_user_by_username(&request.username).await {
            Ok(Some(acc)) => Some((acc.username.clone(), acc.is_admin)),
            _ => None,
        }
    };

    // Attempt to update the user (with atomic last-admin protection in SQL)
    match ctx
        .db
        .users
        .update_user(
            &request.username,
            request.requested_username.as_deref(),
            requested_password_hash.as_deref(),
            request.requested_is_admin,
            request.requested_enabled,
            parsed_permissions.as_ref(),
        )
        .await
    {
        Ok(true) => {
            // Success - send response to requester
            // Use the final username (in case it changed)
            let final_username = request
                .requested_username
                .as_ref()
                .unwrap_or(&request.username)
                .clone();
            let response = ServerMessage::UserUpdateResponse {
                success: true,
                error: None,
                username: Some(final_username.clone()),
            };
            ctx.send_message(&response).await?;

            // Only send PermissionsUpdated if admin, enabled, or permissions changed
            // (not for password-only or username-only changes)
            let permissions_changed = request.requested_is_admin.is_some()
                || request.requested_enabled.is_some()
                || request.requested_permissions.is_some();

            // Get the updated user's account
            if let Ok(Some(updated_account)) =
                ctx.db.users.get_user_by_username(&final_username).await
            {
                // Get the final permissions
                if let Ok(final_permissions) =
                    ctx.db.users.get_user_permissions(updated_account.id).await
                {
                    // Always update cached permissions in UserManager for all sessions of this user
                    // (even if we don't broadcast, keeps cache in sync)
                    ctx.user_manager
                        .update_permissions(
                            updated_account.id,
                            final_permissions.permissions.clone(),
                        )
                        .await;

                    // Only notify the user if their permissions/admin/enabled status changed
                    if permissions_changed {
                        let permission_strings: Vec<String> = final_permissions
                            .permissions
                            .iter()
                            .map(|p| p.as_str().to_string())
                            .collect();

                        // Check if user now has chat topic permission
                        let now_has_chat_topic = updated_account.is_admin
                            || final_permissions
                                .permissions
                                .contains(&Permission::ChatTopic);

                        // Only send fields that change with permissions (max_connections_per_ip for admins)
                        // Other fields (name, description, image, transfer_port) are unchanged and
                        // the client already knows them from login
                        let server_info = if updated_account.is_admin {
                            Some(ServerInfo {
                                max_connections_per_ip: Some(
                                    ctx.db.config.get_max_connections_per_ip().await as u32,
                                ),
                                ..Default::default()
                            })
                        } else {
                            None
                        };

                        // Include chat info only if user has permission
                        let chat_info = if now_has_chat_topic {
                            match ctx.db.chat.get_topic().await {
                                Ok(topic) => Some(ChatInfo {
                                    topic: topic.topic,
                                    topic_set_by: topic.set_by,
                                }),
                                Err(_) => None,
                            }
                        } else {
                            None
                        };

                        let permissions_update = ServerMessage::PermissionsUpdated {
                            is_admin: updated_account.is_admin,
                            permissions: permission_strings,
                            server_info,
                            chat_info,
                        };

                        // Send to all sessions belonging to the updated user
                        ctx.user_manager
                            .broadcast_to_username(&updated_account.username, &permissions_update)
                            .await;
                    }
                }

                // If user was disabled, disconnect all their active sessions
                //
                // Clean Disconnect Flow:
                // 1. Send Error message to user ("Account disabled by admin")
                // 2. Remove user from UserManager (drops the tx sender)
                // 3. Connection handler's rx.recv() returns None (channel closed)
                // 4. Connection loop breaks cleanly
                // 5. TCP connection closes
                //
                // This approach avoids manual shutdown signals and relies on channel semantics:
                // - User struct contains a tx (clone of the channel sender)
                // - UserManager.remove_user() drops the User, which drops tx
                // - When all senders are dropped, rx.recv() returns None
                // - Connection handler detects None and breaks the loop
                //
                // Note: UserDisconnected is only broadcast once here (connection.rs cleanup
                // doesn't re-broadcast because the user is already removed from manager)
                if let Some(false) = request.requested_enabled {
                    // Get all session IDs for this user
                    let session_ids = ctx
                        .user_manager
                        .get_session_ids_for_user(&updated_account.username)
                        .await;

                    // Disconnect each session
                    for session_id in session_ids {
                        // Send disconnect message to inform the user in their locale
                        if let Some(user) =
                            ctx.user_manager.get_user_by_session_id(session_id).await
                        {
                            let disconnect_msg = ServerMessage::Error {
                                message: err_account_disabled_by_admin(&user.locale),
                                command: None,
                            };
                            let _ = user.tx.send((disconnect_msg, None));
                        }

                        // Remove user from manager and broadcast disconnection
                        ctx.user_manager.remove_user_and_broadcast(session_id).await;
                    }
                }

                // Check if username or admin status changed
                let username_changed = old_account
                    .as_ref()
                    .map(|(old_name, _)| old_name != &updated_account.username)
                    .unwrap_or(false);
                let admin_status_changed = old_account
                    .as_ref()
                    .map(|(_, old_admin)| *old_admin != updated_account.is_admin)
                    .unwrap_or(false);

                // If username changed, update UserManager
                if username_changed {
                    ctx.user_manager
                        .update_username(updated_account.id, updated_account.username.clone())
                        .await;
                }

                // If admin status changed, update UserManager
                if admin_status_changed {
                    ctx.user_manager
                        .update_admin_status(updated_account.id, updated_account.is_admin)
                        .await;
                }

                // Only broadcast UserUpdated if username or admin status changed
                if username_changed || admin_status_changed {
                    let session_ids = ctx
                        .user_manager
                        .get_session_ids_for_user(&updated_account.username)
                        .await;

                    // Get earliest login time, locale, and avatar from all sessions
                    // Avatar uses "latest login wins"
                    let (login_time, locale, avatar) = if !session_ids.is_empty() {
                        let user_sessions = ctx
                            .user_manager
                            .get_sessions_by_username(&updated_account.username)
                            .await;

                        let login_time = user_sessions
                            .iter()
                            .map(|u| u.login_time)
                            .min()
                            .unwrap_or(0);

                        let locale = user_sessions
                            .first()
                            .map(|u| u.locale.clone())
                            .unwrap_or_else(|| DEFAULT_LOCALE.to_string());

                        // Avatar from most recent login
                        let avatar = user_sessions
                            .iter()
                            .max_by_key(|u| u.login_time)
                            .and_then(|u| u.avatar.clone());

                        (login_time, locale, avatar)
                    } else {
                        (0, DEFAULT_LOCALE.to_string(), None) // User not currently online
                    };

                    let user_info = UserInfo {
                        username: updated_account.username.clone(),
                        // For account-level updates, nickname == username
                        // (we're broadcasting about the account, not a specific session)
                        nickname: updated_account.username.clone(),
                        login_time,
                        is_admin: updated_account.is_admin,
                        is_shared: updated_account.is_shared,
                        session_ids,
                        locale,
                        avatar,
                        is_away: false,
                        status: None,
                    };

                    let user_updated = ServerMessage::UserUpdated {
                        previous_username: old_account
                            .as_ref()
                            .map(|(name, _)| name.clone())
                            .unwrap_or(updated_account.username.clone()),
                        user: user_info,
                    };
                    ctx.user_manager
                        .broadcast_to_permission(user_updated, Permission::UserList)
                        .await;
                }
            }

            Ok(())
        }
        Ok(false) => {
            // Update was blocked (user not found, last admin, or duplicate username)
            // We need to determine which error to return
            let error_message = if ctx
                .db
                .users
                .get_user_by_username(&request.username)
                .await
                .ok()
                .flatten()
                .is_none()
            {
                err_user_not_found(ctx.locale, &request.username)
            } else if let Some(ref new_username) = request.requested_username {
                // Check if the new username already exists (and it's not the same user)
                if new_username != &request.username
                    && ctx
                        .db
                        .users
                        .get_user_by_username(new_username)
                        .await
                        .ok()
                        .flatten()
                        .is_some()
                {
                    err_username_exists(ctx.locale, new_username)
                } else {
                    // Username change was blocked but not due to duplicate - must be admin protection
                    err_cannot_demote_last_admin(ctx.locale)
                }
            } else if request.requested_is_admin == Some(false) {
                err_cannot_demote_last_admin(ctx.locale)
            } else if request.requested_enabled == Some(false) {
                err_cannot_disable_last_admin(ctx.locale)
            } else {
                err_update_failed(ctx.locale, &request.username)
            };

            let response = ServerMessage::UserUpdateResponse {
                success: false,
                error: Some(error_message.to_string()),
                username: None,
            };
            ctx.send_message(&response).await
        }
        Err(e) => {
            eprintln!("Database error updating user: {}", e);
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some("UserUpdate"))
                .await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::handlers::testing::*;
    use crate::users::user::NewSessionParams;

    #[tokio::test]
    async fn test_userupdate_requires_login() {
        let mut test_ctx = create_test_context().await;

        let request = UserUpdateRequest {
            current_password: None,
            username: "alice".to_string(),
            requested_username: Some("alice2".to_string()),
            requested_password: None,
            requested_is_admin: None,
            requested_enabled: None,
            requested_permissions: None,
            session_id: None, // Not logged in
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_userupdate_requires_permission() {
        let mut test_ctx = create_test_context().await;

        // Login as user without UserEdit permission
        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        // Create another user to edit
        test_ctx
            .db
            .users
            .create_user("bob", "hash", false, false, true, &Permissions::new())
            .await
            .unwrap();

        let request = UserUpdateRequest {
            current_password: None,
            username: "bob".to_string(),
            requested_username: Some("bob2".to_string()),
            requested_password: None,
            requested_is_admin: None,
            requested_enabled: None,
            requested_permissions: None,
            session_id: Some(session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::Error { message, .. } => {
                assert_eq!(message, err_permission_denied(DEFAULT_TEST_LOCALE));
            }
            _ => panic!("Expected Error message"),
        }
    }

    #[tokio::test]
    async fn test_userupdate_cannot_edit_self_username() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let request = UserUpdateRequest {
            current_password: None,
            username: "admin".to_string(),
            requested_username: Some("admin2".to_string()),
            requested_password: None,
            requested_is_admin: None,
            requested_enabled: None,
            requested_permissions: None,
            session_id: Some(session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error.unwrap(), err_cannot_edit_self(DEFAULT_TEST_LOCALE));
            }
            _ => panic!("Expected UserUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_userupdate_cannot_edit_self_admin_status() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Try to change own admin status (even with current_password, this should be rejected)
        let request = UserUpdateRequest {
            current_password: Some("password".to_string()),
            username: "admin".to_string(),
            requested_username: None,
            requested_password: None,
            requested_is_admin: Some(false), // Trying to demote self
            requested_enabled: None,
            requested_permissions: None,
            session_id: Some(session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error.unwrap(), err_cannot_edit_self(DEFAULT_TEST_LOCALE));
            }
            _ => panic!("Expected UserUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_userupdate_cannot_edit_self_enabled_status() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Try to change own enabled status
        let request = UserUpdateRequest {
            current_password: Some("password".to_string()),
            username: "admin".to_string(),
            requested_username: None,
            requested_password: None,
            requested_is_admin: None,
            requested_enabled: Some(false), // Trying to disable self
            requested_permissions: None,
            session_id: Some(session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error.unwrap(), err_cannot_edit_self(DEFAULT_TEST_LOCALE));
            }
            _ => panic!("Expected UserUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_userupdate_cannot_edit_self_permissions() {
        let mut test_ctx = create_test_context().await;

        // Login as regular user (non-admin)
        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        // Try to change own permissions
        let request = UserUpdateRequest {
            current_password: Some("password".to_string()),
            username: "alice".to_string(),
            requested_username: None,
            requested_password: None,
            requested_is_admin: None,
            requested_enabled: None,
            requested_permissions: Some(vec!["user_edit".to_string()]), // Trying to give self more permissions
            session_id: Some(session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error.unwrap(), err_cannot_edit_self(DEFAULT_TEST_LOCALE));
            }
            _ => panic!("Expected UserUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_userupdate_self_password_change_success() {
        let mut test_ctx = create_test_context().await;

        // Login as alice (login_user creates the user with the given password)
        let session_id = login_user(&mut test_ctx, "alice", "oldpassword", &[], false).await;

        // Change own password with correct current password
        let request = UserUpdateRequest {
            current_password: Some("oldpassword".to_string()),
            username: "alice".to_string(),
            requested_username: None,
            requested_password: Some("newpassword".to_string()),
            requested_is_admin: None,
            requested_enabled: None,
            requested_permissions: None,
            session_id: Some(session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse {
                success,
                error,
                username,
            } => {
                assert!(success, "Expected success, got error: {:?}", error);
                assert!(error.is_none());
                assert_eq!(username, Some("alice".to_string()));
            }
            _ => panic!("Expected UserUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_userupdate_self_password_change_wrong_current_password() {
        let mut test_ctx = create_test_context().await;

        // Login as alice (login_user creates the user with the given password)
        let session_id = login_user(&mut test_ctx, "alice", "correctpassword", &[], false).await;

        // Try to change password with wrong current password
        let request = UserUpdateRequest {
            current_password: Some("wrongpassword".to_string()),
            username: "alice".to_string(),
            requested_username: None,
            requested_password: Some("newpassword".to_string()),
            requested_is_admin: None,
            requested_enabled: None,
            requested_permissions: None,
            session_id: Some(session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(
                    error.unwrap(),
                    err_current_password_incorrect(DEFAULT_TEST_LOCALE)
                );
            }
            _ => panic!("Expected UserUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_userupdate_self_password_change_missing_current_password() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Try to change own password without providing current password
        let request = UserUpdateRequest {
            current_password: None,
            username: "admin".to_string(),
            requested_username: None,
            requested_password: Some("newpassword".to_string()),
            requested_is_admin: None,
            requested_enabled: None,
            requested_permissions: None,
            session_id: Some(session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(
                    error.unwrap(),
                    err_current_password_required(DEFAULT_TEST_LOCALE)
                );
            }
            _ => panic!("Expected UserUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_userupdate_admin_can_edit() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create another user to edit
        test_ctx
            .db
            .users
            .create_user("bob", "hash", false, false, true, &Permissions::new())
            .await
            .unwrap();

        let request = UserUpdateRequest {
            current_password: None,
            username: "bob".to_string(),
            requested_username: Some("bobby".to_string()),
            requested_password: None,
            requested_is_admin: None,
            requested_enabled: None,
            requested_permissions: None,
            session_id: Some(session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse {
                success,
                error,
                username,
            } => {
                assert!(success);
                assert!(error.is_none());
                assert_eq!(username, Some("bobby".to_string()));
            }
            _ => panic!("Expected UserUpdateResponse"),
        }

        // Verify username was changed
        let user = test_ctx
            .db
            .users
            .get_user_by_username("bobby")
            .await
            .unwrap();
        assert!(user.is_some());
        let user = test_ctx.db.users.get_user_by_username("bob").await.unwrap();
        assert!(user.is_none());
    }

    #[tokio::test]
    async fn test_userupdate_user_not_found() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let request = UserUpdateRequest {
            current_password: None,
            username: "nonexistent".to_string(),
            requested_username: Some("newname".to_string()),
            requested_password: None,
            requested_is_admin: None,
            requested_enabled: None,
            requested_permissions: None,
            session_id: Some(session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(
                    error.unwrap(),
                    err_user_not_found(DEFAULT_TEST_LOCALE, "nonexistent")
                );
            }
            _ => panic!("Expected UserUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_userupdate_cannot_demote_last_admin() {
        let mut test_ctx = create_test_context().await;

        // Create two admins
        let admin1_session = login_user(&mut test_ctx, "admin1", "password", &[], true).await;
        let admin2_session = login_user(&mut test_ctx, "admin2", "password", &[], true).await;

        // Admin1 demotes Admin2 (should succeed, admin1 still exists)
        let request = UserUpdateRequest {
            current_password: None,
            username: "admin2".to_string(),
            requested_username: None,
            requested_password: None,
            requested_is_admin: Some(false), // Demote to non-admin
            requested_enabled: None,
            requested_permissions: None,
            session_id: Some(admin1_session),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse {
                success,
                error,
                username,
            } => {
                assert!(success);
                assert!(error.is_none());
                assert_eq!(username, Some("admin2".to_string()));
            }
            _ => panic!("Expected UserUpdateResponse"),
        }

        // Now admin2 tries to demote admin1 (should fail - no permission)
        let request = UserUpdateRequest {
            current_password: None,
            username: "admin1".to_string(),
            requested_username: None,
            requested_password: None,
            requested_is_admin: Some(false), // Try to demote last admin
            requested_enabled: None,
            requested_permissions: None,
            session_id: Some(admin2_session),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::Error { message, .. } => {
                assert_eq!(message, err_permission_denied(DEFAULT_TEST_LOCALE));
            }
            _ => panic!("Expected Error message"),
        }
    }

    #[tokio::test]
    async fn test_userupdate_with_permission() {
        let mut test_ctx = create_test_context().await;

        // Login as user with UserEdit permission
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[crate::db::Permission::UserEdit],
            false,
        )
        .await;

        // Create another user to edit
        test_ctx
            .db
            .users
            .create_user("bob", "hash", false, false, true, &Permissions::new())
            .await
            .unwrap();

        let request = UserUpdateRequest {
            current_password: None,
            username: "bob".to_string(),
            requested_username: Some("robert".to_string()),
            requested_password: None,
            requested_is_admin: None,
            requested_enabled: None,
            requested_permissions: None,
            session_id: Some(session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse {
                success,
                error,
                username,
            } => {
                assert!(success);
                assert!(error.is_none());
                assert_eq!(username, Some("robert".to_string()));
            }
            _ => panic!("Expected UserUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_userupdate_non_admin_cannot_change_admin_status() {
        let mut test_ctx = create_test_context().await;

        // Login as user with UserEdit permission
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[crate::db::Permission::UserEdit],
            false,
        )
        .await;

        // Create another user to edit
        test_ctx
            .db
            .users
            .create_user("bob", "hash", false, false, true, &Permissions::new())
            .await
            .unwrap();

        // Try to make bob an admin
        let request = UserUpdateRequest {
            current_password: None,
            username: "bob".to_string(),
            requested_username: None,
            requested_password: None,
            requested_is_admin: Some(true), // Try to make admin
            requested_enabled: None,
            requested_permissions: None,
            session_id: Some(session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::Error { message, .. } => {
                assert_eq!(message, err_permission_denied(DEFAULT_TEST_LOCALE));
            }
            _ => panic!("Expected Error message"),
        }
    }

    #[tokio::test]
    async fn test_userupdate_duplicate_username() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create two users
        test_ctx
            .db
            .users
            .create_user("alice", "hash", false, false, true, &Permissions::new())
            .await
            .unwrap();
        test_ctx
            .db
            .users
            .create_user("bob", "hash", false, false, true, &Permissions::new())
            .await
            .unwrap();

        // Try to rename bob to alice (should fail)
        let request = UserUpdateRequest {
            current_password: None,
            username: "bob".to_string(),
            requested_username: Some("alice".to_string()),
            requested_password: None,
            requested_is_admin: None,
            requested_enabled: None,
            requested_permissions: None,
            session_id: Some(session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            _ => panic!("Expected UserUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_userupdate_change_password() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a user
        test_ctx
            .db
            .users
            .create_user("alice", "oldhash", false, false, true, &Permissions::new())
            .await
            .unwrap();

        // Change alice's password
        let request = UserUpdateRequest {
            current_password: None,
            username: "alice".to_string(),
            requested_username: None,
            requested_password: Some("newpassword".to_string()),
            requested_is_admin: None,
            requested_enabled: None,
            requested_permissions: None,
            session_id: Some(session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse {
                success,
                error,
                username,
            } => {
                assert!(success);
                assert!(error.is_none());
                assert_eq!(username, Some("alice".to_string()));
            }
            _ => panic!("Expected UserUpdateResponse"),
        }

        // Verify password was changed (hash should be different)
        let user = test_ctx
            .db
            .users
            .get_user_by_username("alice")
            .await
            .unwrap()
            .unwrap();
        assert_ne!(user.hashed_password, "oldhash");
    }

    #[tokio::test]
    async fn test_userupdate_change_permissions() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a user with no permissions
        let bob = test_ctx
            .db
            .users
            .create_user("bob", "hash", false, false, true, &Permissions::new())
            .await
            .unwrap();

        // Give bob some permissions
        let request = UserUpdateRequest {
            current_password: None,
            username: "bob".to_string(),
            requested_username: None,
            requested_password: None,
            requested_is_admin: None,
            requested_enabled: None,
            requested_permissions: Some(vec!["user_list".to_string(), "chat_send".to_string()]),
            session_id: Some(session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse {
                success,
                error,
                username,
            } => {
                assert!(success);
                assert!(error.is_none());
                assert_eq!(username, Some("bob".to_string()));
            }
            _ => panic!("Expected UserUpdateResponse"),
        }

        // Verify permissions were changed
        assert!(
            test_ctx
                .db
                .users
                .has_permission(bob.id, Permission::UserList)
                .await
                .unwrap()
        );
        assert!(
            test_ctx
                .db
                .users
                .has_permission(bob.id, Permission::ChatSend)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn test_userupdate_empty_password_means_no_change() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a user with a specific password hash
        let original_hash = "original_hash_12345";
        test_ctx
            .db
            .users
            .create_user(
                "alice",
                original_hash,
                false,
                false,
                true,
                &Permissions::new(),
            )
            .await
            .unwrap();

        // Try to edit alice with empty password (should not change password)
        let request = UserUpdateRequest {
            current_password: None,
            username: "alice".to_string(),
            requested_username: None,
            requested_password: Some("".to_string()), // Empty password
            requested_is_admin: None,
            requested_enabled: None,
            requested_permissions: None,
            session_id: Some(session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse {
                success,
                error,
                username,
            } => {
                assert!(success);
                assert!(error.is_none());
                assert_eq!(username, Some("alice".to_string()));
            }
            _ => panic!("Expected UserUpdateResponse"),
        }

        // Verify password was NOT changed (hash should be same)
        // Verify password was NOT changed
        let user = test_ctx
            .db
            .users
            .get_user_by_username("alice")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            user.hashed_password, original_hash,
            "Password should not have been changed"
        );
    }

    #[tokio::test]
    async fn test_userupdate_cannot_revoke_permissions_user_doesnt_have() {
        let mut test_ctx = create_test_context().await;

        // Create Alice with user_list, user_info, and chat_send permissions
        let _alice_session = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[
                Permission::UserList,
                Permission::UserInfo,
                Permission::ChatSend,
            ],
            false,
        )
        .await;

        // Create Bob with only user_edit and user_list permissions
        let bob_session_id = login_user(
            &mut test_ctx,
            "bob",
            "password",
            &[Permission::UserEdit, Permission::UserList],
            false,
        )
        .await;

        // Get Alice's user ID for verification later
        let alice = test_ctx
            .db
            .users
            .get_user_by_username("alice")
            .await
            .unwrap()
            .unwrap();

        // Bob tries to update Alice, removing user_info and chat_send (permissions Bob doesn't have)
        // Bob tries to set Alice's permissions to just user_list (which Bob has)
        let request = UserUpdateRequest {
            current_password: None,
            username: "alice".to_string(),
            requested_username: None,
            requested_password: None,
            requested_is_admin: None,
            requested_enabled: None,
            requested_permissions: Some(vec!["user_list".to_string()]), // Bob only grants user_list
            session_id: Some(bob_session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse {
                success,
                error,
                username,
            } => {
                assert!(success, "Update should succeed with merged permissions");
                assert!(error.is_none());
                assert_eq!(username, Some("alice".to_string()));
            }
            _ => panic!("Expected UserUpdateResponse"),
        }

        // Verify target has both their original permission AND the editor's granted permission
        // Verify Alice's permissions were merged correctly:
        // - user_list: Bob set this (and has it), Alice should have it
        // - user_info: Bob can't modify this (he doesn't have it), Alice should keep it
        // - chat_send: Bob can't modify this (he doesn't have it), Alice should keep it
        assert!(
            test_ctx
                .db
                .users
                .has_permission(alice.id, Permission::UserList)
                .await
                .unwrap(),
            "Alice should have user_list (Bob set it)"
        );
        assert!(
            test_ctx
                .db
                .users
                .has_permission(alice.id, Permission::UserInfo)
                .await
                .unwrap(),
            "Alice should keep user_info (Bob can't modify it)"
        );
        assert!(
            test_ctx
                .db
                .users
                .has_permission(alice.id, Permission::ChatSend)
                .await
                .unwrap(),
            "Alice should keep chat_send (Bob can't modify it)"
        );
    }

    #[tokio::test]
    async fn test_userupdate_cannot_disable_self() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Try to disable self (will be caught by self-edit check)
        let request = UserUpdateRequest {
            current_password: None,
            username: "admin".to_string(),
            requested_username: None,
            requested_password: None,
            requested_is_admin: None,
            requested_enabled: Some(false), // Try to disable
            requested_permissions: None,
            session_id: Some(session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Should send error response, not disconnect");
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(!success, "Should not allow self-edit");
                assert_eq!(error, Some(err_cannot_edit_self(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected UserUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_userupdate_cannot_disable_last_admin() {
        let mut test_ctx = create_test_context().await;

        // Create two admins
        let admin1_session = login_user(&mut test_ctx, "admin1", "password", &[], true).await;
        let _admin2_session = login_user(&mut test_ctx, "admin2", "password", &[], true).await;

        // Admin1 disables admin2 (should succeed, admin1 still exists)
        let request = UserUpdateRequest {
            current_password: None,
            username: "admin2".to_string(),
            requested_username: None,
            requested_password: None,
            requested_is_admin: None,
            requested_enabled: Some(false),
            requested_permissions: None,
            session_id: Some(admin1_session),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse { success, .. } => {
                assert!(success, "Should successfully disable admin2");
            }
            _ => panic!("Expected UserUpdateResponse"),
        }

        // Now admin1 is the only admin. Create another admin to try to disable admin1.
        let _admin3_session = login_user(&mut test_ctx, "admin3", "password", &[], true).await;

        // Admin3 tries to disable admin1 (should fail - last admin protection)
        // But wait, admin3 is also an admin now, so there are two admins again.
        // The test needs to be that admin3 tries to disable themselves when they're the last.
        // Actually, let's test the database layer directly for last admin protection.

        // Re-enable admin2 first
        let request = UserUpdateRequest {
            current_password: None,
            username: "admin2".to_string(),
            requested_username: None,
            requested_password: None,
            requested_is_admin: None,
            requested_enabled: Some(true),
            requested_permissions: None,
            session_id: Some(admin1_session),
        };
        let _ = handle_user_update(request, &mut test_ctx.handler_context()).await;
        let _ = read_server_message(&mut test_ctx.client).await;

        // Demote admin2 and admin3 so admin1 is the only admin
        let request = UserUpdateRequest {
            current_password: None,
            username: "admin2".to_string(),
            requested_username: None,
            requested_password: None,
            requested_is_admin: Some(false),
            requested_enabled: None,
            requested_permissions: None,
            session_id: Some(admin1_session),
        };
        let _ = handle_user_update(request, &mut test_ctx.handler_context()).await;
        let _ = read_server_message(&mut test_ctx.client).await;

        let request = UserUpdateRequest {
            current_password: None,
            username: "admin3".to_string(),
            requested_username: None,
            requested_password: None,
            requested_is_admin: Some(false),
            requested_enabled: None,
            requested_permissions: None,
            session_id: Some(admin1_session),
        };
        let _ = handle_user_update(request, &mut test_ctx.handler_context()).await;
        let _ = read_server_message(&mut test_ctx.client).await;

        // Now admin1 is the only admin. Admin1 tries to disable themselves (should fail - self-edit)
        // But self-edit is blocked. So we test the database protection directly.
        let admin1 = test_ctx
            .db
            .users
            .get_user_by_username("admin1")
            .await
            .unwrap()
            .unwrap();

        // Try to disable the last admin via database
        let result = test_ctx
            .db
            .users
            .update_user(&admin1.username, None, None, None, Some(false), None)
            .await
            .unwrap();
        assert!(!result, "Should not be able to disable the last admin");

        // Verify admin1 is still enabled
        let admin1_after = test_ctx
            .db
            .users
            .get_user_by_username("admin1")
            .await
            .unwrap()
            .unwrap();
        assert!(admin1_after.enabled, "Last admin should still be enabled");
    }

    #[tokio::test]
    async fn test_userupdate_non_admin_cannot_edit_admin() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let _admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a non-admin user with user_edit permission
        let mut perms = Permissions::new();
        perms.permissions.insert(Permission::UserEdit);
        let editor = test_ctx
            .db
            .users
            .create_user("editor", "hash", false, false, true, &perms)
            .await
            .unwrap();

        // Add editor to UserManager
        let editor_session = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                db_user_id: editor.id,
                username: "editor".to_string(),
                is_admin: false,
                is_shared: false,
                permissions: perms.permissions.clone(),
                address: test_ctx.peer_addr,
                created_at: editor.created_at,
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: DEFAULT_TEST_LOCALE.to_string(),
                avatar: None,
                nickname: "editor".to_string(),
                is_away: false,
                status: None,
            })
            .await
            .expect("Failed to add user");

        // Non-admin editor tries to edit admin - should fail
        let request = UserUpdateRequest {
            current_password: None,
            username: "admin".to_string(),
            requested_username: None,
            requested_password: Some("newpassword".to_string()),
            requested_is_admin: None,
            requested_enabled: None,
            requested_permissions: None,
            session_id: Some(editor_session),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Should send error response, not disconnect");
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(!success, "Non-admin should not be able to edit admin");
                assert!(error.is_some(), "Should have error message");
                let error_msg = error.unwrap();
                assert!(
                    error_msg.contains("admin"),
                    "Error should mention admin restriction"
                );
            }
            _ => panic!("Expected UserUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_userupdate_change_enabled_status() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a regular user
        let bob = test_ctx
            .db
            .users
            .create_user("bob", "hash", false, false, true, &Permissions::new())
            .await
            .unwrap();

        // Verify bob is enabled
        assert!(bob.enabled, "Bob should be enabled initially");

        // Disable bob
        let request = UserUpdateRequest {
            current_password: None,
            username: "bob".to_string(),
            requested_username: None,
            requested_password: None,
            requested_is_admin: None,
            requested_enabled: Some(false), // Disable
            requested_permissions: None,
            session_id: Some(session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse {
                success,
                error,
                username,
            } => {
                assert!(success, "Should successfully disable user");
                assert!(error.is_none());
                assert_eq!(username, Some("bob".to_string()));
            }
            _ => panic!("Expected UserUpdateResponse"),
        }

        // Verify user is now disabled
        // Verify bob is now disabled in database
        let bob_after = test_ctx
            .db
            .users
            .get_user_by_username("bob")
            .await
            .unwrap()
            .unwrap();
        assert!(!bob_after.enabled, "Bob should be disabled");

        // Re-enable bob
        let request = UserUpdateRequest {
            current_password: None,
            username: "bob".to_string(),
            requested_username: None,
            requested_password: None,
            requested_is_admin: None,
            requested_enabled: Some(true), // Enable
            requested_permissions: None,
            session_id: Some(session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse {
                success,
                error,
                username,
            } => {
                assert!(success, "Should successfully re-enable user");
                assert!(error.is_none());
                assert_eq!(username, Some("bob".to_string()));
            }
            _ => panic!("Expected UserUpdateResponse"),
        }

        // Verify user is now enabled
        // Verify bob is enabled again
        let bob_final = test_ctx
            .db
            .users
            .get_user_by_username("bob")
            .await
            .unwrap()
            .unwrap();
        assert!(bob_final.enabled, "Bob should be enabled again");
    }

    #[tokio::test]
    async fn test_userupdate_disconnects_when_disabling() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Login as bob (the user we'll disable)
        let bob_session = login_user(&mut test_ctx, "bob", "password", &[], false).await;

        // Verify bob is in the user manager
        assert!(
            test_ctx
                .user_manager
                .get_user_by_session_id(bob_session)
                .await
                .is_some(),
            "Bob should be in user manager"
        );

        // Admin disables bob
        let request = UserUpdateRequest {
            current_password: None,
            username: "bob".to_string(),
            requested_username: None,
            requested_password: None,
            requested_is_admin: None,
            requested_enabled: Some(false), // Disable
            requested_permissions: None,
            session_id: Some(admin_session),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        // Bob should be removed from user manager
        assert!(
            test_ctx
                .user_manager
                .get_user_by_session_id(bob_session)
                .await
                .is_none(),
            "Bob should be removed from user manager after being disabled"
        );
    }

    #[tokio::test]
    async fn test_userupdate_atomic_admin_demotion_protection() {
        let mut test_ctx = create_test_context().await;

        // Create two admin users
        let admin1 = test_ctx
            .db
            .users
            .create_user("admin1", "hash1", true, false, true, &Permissions::new())
            .await
            .unwrap();
        let admin2 = test_ctx
            .db
            .users
            .create_user("admin2", "hash2", true, false, true, &Permissions::new())
            .await
            .unwrap();

        // Login both admins
        let admin1_session = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                db_user_id: admin1.id,
                username: "admin1".to_string(),
                is_admin: true,
                is_shared: false,
                permissions: std::collections::HashSet::new(),
                address: test_ctx.peer_addr,
                created_at: admin1.created_at,
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: DEFAULT_TEST_LOCALE.to_string(),
                avatar: None,
                nickname: "editor".to_string(),
                is_away: false,
                status: None,
            })
            .await
            .expect("Failed to add user");

        let _admin2_session = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                db_user_id: admin2.id,
                username: "admin2".to_string(),
                is_admin: true,
                is_shared: false,
                permissions: std::collections::HashSet::new(),
                address: test_ctx.peer_addr,
                created_at: admin2.created_at,
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: DEFAULT_TEST_LOCALE.to_string(),
                avatar: None,
                nickname: "admin".to_string(),
                is_away: false,
                status: None,
            })
            .await
            .expect("Failed to add user");

        // Admin1 demotes admin2 to non-admin (should succeed - 2 admins exist)
        let request = UserUpdateRequest {
            current_password: None,
            username: "admin2".to_string(),
            requested_username: None,
            requested_password: None,
            requested_is_admin: Some(false), // Demote to non-admin
            requested_enabled: None,
            requested_permissions: None,
            session_id: Some(admin1_session),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse {
                success,
                error,
                username,
            } => {
                assert!(
                    success,
                    "Should successfully demote admin2 (2 admins exist)"
                );
                assert!(error.is_none());
                assert_eq!(username, Some("admin2".to_string()));
            }
            _ => panic!("Expected UserUpdateResponse"),
        }

        // Verify admin2 is now non-admin in database
        let admin2_account = test_ctx
            .db
            .users
            .get_user_by_username("admin2")
            .await
            .unwrap()
            .unwrap();
        assert!(
            !admin2_account.is_admin,
            "Admin2 should be demoted to non-admin"
        );

        // Now admin2 (now a non-admin with user_edit permission) tries to demote admin1
        // First, give admin2 the user_edit permission
        let mut perms = Permissions::new();
        perms.permissions.insert(Permission::UserEdit);
        test_ctx
            .db
            .users
            .update_user("admin2", None, None, None, None, Some(&perms))
            .await
            .unwrap();

        // Admin2 tries to demote admin1 (last admin) - should fail at DB level atomically
        // Note: This bypasses the "non-admin cannot change admin status" check by using
        // the database directly to test the atomic SQL protection
        let result = test_ctx
            .db
            .users
            .update_user(
                "admin1",
                None,
                None,
                Some(false), // Try to demote last admin
                None,
                None,
            )
            .await;

        // Should return Ok(false) - update blocked by atomic SQL protection
        assert!(result.is_ok());
        assert!(
            !result.unwrap(),
            "Database should block demoting last admin atomically"
        );

        // Verify admin1 is still admin
        let admin1_account = test_ctx
            .db
            .users
            .get_user_by_username("admin1")
            .await
            .unwrap()
            .unwrap();
        assert!(
            admin1_account.is_admin,
            "Admin1 should still be admin (protected by atomic SQL)"
        );
    }

    // ========================================================================
    // Shared Account Tests
    // ========================================================================

    #[tokio::test]
    async fn test_userupdate_shared_user_cannot_change_own_password() {
        let mut test_ctx = create_test_context().await;

        // Create admin first
        let _admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create shared account
        test_ctx
            .db
            .users
            .create_user(
                "shared_acct",
                &db::hash_password("sharedpass").unwrap(),
                false,
                true,
                true,
                &db::Permissions::new(),
            )
            .await
            .expect("shared account creation should succeed");

        // Login as shared account with nickname
        let mut shared_session_id = None;
        let login_request = crate::handlers::login::LoginRequest {
            username: "shared_acct".to_string(),
            password: "sharedpass".to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: Some("Alice".to_string()),
            handshake_complete: true,
        };
        let login_result = crate::handlers::handle_login(
            login_request,
            &mut shared_session_id,
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(login_result.is_ok(), "Shared account login should succeed");

        // Read login response
        let _login_response = read_server_message(&mut test_ctx.client).await;

        // Try to change own password
        let request = UserUpdateRequest {
            username: "shared_acct".to_string(),
            current_password: Some("sharedpass".to_string()),
            requested_username: None,
            requested_password: Some("newpassword".to_string()),
            requested_is_admin: None,
            requested_enabled: None,
            requested_permissions: None,
            session_id: shared_session_id,
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Should send error response");

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(
                    !success,
                    "Shared user should not be able to change password"
                );
                assert!(error.is_some(), "Should have error message");
                assert!(
                    error.unwrap().contains("shared account"),
                    "Error should mention shared account"
                );
            }
            _ => panic!("Expected UserUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_userupdate_shared_account_forbidden_permissions() {
        let mut test_ctx = create_test_context().await;

        // Create admin
        let admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create shared account
        test_ctx
            .db
            .users
            .create_user(
                "shared_acct",
                &db::hash_password("sharedpass").unwrap(),
                false,
                true,
                true,
                &db::Permissions::new(),
            )
            .await
            .expect("shared account creation should succeed");

        // Try to update shared account with forbidden permissions
        let request = UserUpdateRequest {
            username: "shared_acct".to_string(),
            current_password: None,
            requested_username: None,
            requested_password: None,
            requested_is_admin: None,
            requested_enabled: None,
            requested_permissions: Some(vec![
                "chat_send".to_string(),   // allowed
                "user_kick".to_string(),   // forbidden
                "news_create".to_string(), // forbidden
            ]),
            session_id: Some(admin_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Should send error response");

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(!success, "Should fail with forbidden permissions");
                assert!(error.is_some(), "Should have error message");
                let err_msg = error.unwrap();
                assert!(
                    err_msg.contains("user_kick") || err_msg.contains("news_create"),
                    "Error should mention forbidden permissions: {}",
                    err_msg
                );
            }
            _ => panic!("Expected UserUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_userupdate_shared_account_allowed_permissions() {
        let mut test_ctx = create_test_context().await;

        // Create admin
        let admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create shared account
        test_ctx
            .db
            .users
            .create_user(
                "shared_acct",
                &db::hash_password("sharedpass").unwrap(),
                false,
                true,
                true,
                &db::Permissions::new(),
            )
            .await
            .expect("shared account creation should succeed");

        // Update shared account with only allowed permissions
        let request = UserUpdateRequest {
            username: "shared_acct".to_string(),
            current_password: None,
            requested_username: None,
            requested_password: None,
            requested_is_admin: None,
            requested_enabled: None,
            requested_permissions: Some(vec![
                "chat_send".to_string(),
                "chat_receive".to_string(),
                "user_list".to_string(),
                "user_message".to_string(),
            ]),
            session_id: Some(admin_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Should succeed");

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(
                    success,
                    "Should successfully update shared account permissions"
                );
                assert!(error.is_none(), "Should have no error");
            }
            _ => panic!("Expected UserUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_userupdate_cannot_rename_guest_account() {
        use crate::handlers::testing::read_server_message;

        let mut test_ctx = create_test_context().await;

        // Create admin user
        let password = "password";
        let hashed = hash_password(password).expect("hash should work");
        test_ctx
            .db
            .users
            .create_user("admin", &hashed, true, false, true, &Permissions::new())
            .await
            .unwrap();

        // Login as admin
        let admin_id = test_ctx
            .user_manager
            .add_user(crate::users::user::NewSessionParams {
                session_id: 0,
                db_user_id: 1,
                username: "admin".to_string(),
                is_admin: true,
                is_shared: false,
                permissions: std::collections::HashSet::new(),
                address: test_ctx.peer_addr,
                created_at: 0,
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: DEFAULT_TEST_LOCALE.to_string(),
                avatar: None,
                nickname: "admin".to_string(),
                is_away: false,
                status: None,
            })
            .await
            .unwrap();

        // Try to rename guest account
        let request = UserUpdateRequest {
            username: "guest".to_string(),
            current_password: None,
            requested_username: Some("notguest".to_string()),
            requested_password: None,
            requested_is_admin: None,
            requested_enabled: None,
            requested_permissions: None,
            session_id: Some(admin_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Handler should return Ok");

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(!success, "Should fail to rename guest account");
                assert!(error.is_some(), "Should have error message");
                let error_msg = error.unwrap();
                assert!(
                    error_msg.contains("guest") || error_msg.contains("renamed"),
                    "Error should mention guest account cannot be renamed"
                );
            }
            _ => panic!("Expected UserUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_userupdate_guest_account_other_fields_allowed() {
        use crate::handlers::testing::read_server_message;

        let mut test_ctx = create_test_context().await;

        // Create admin user
        let password = "password";
        let hashed = hash_password(password).expect("hash should work");
        test_ctx
            .db
            .users
            .create_user("admin", &hashed, true, false, true, &Permissions::new())
            .await
            .unwrap();

        // Login as admin
        let admin_id = test_ctx
            .user_manager
            .add_user(crate::users::user::NewSessionParams {
                session_id: 0,
                db_user_id: 1,
                username: "admin".to_string(),
                is_admin: true,
                is_shared: false,
                permissions: std::collections::HashSet::new(),
                address: test_ctx.peer_addr,
                created_at: 0,
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: DEFAULT_TEST_LOCALE.to_string(),
                avatar: None,
                nickname: "admin".to_string(),
                is_away: false,
                status: None,
            })
            .await
            .unwrap();

        // Update guest account permissions
        // Enable the guest account (should be allowed)
        let request = UserUpdateRequest {
            username: "guest".to_string(),
            current_password: None,
            requested_username: None, // Not renaming
            requested_password: None,
            requested_is_admin: None,
            requested_enabled: Some(true), // Enable guest
            requested_permissions: None,
            session_id: Some(admin_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Handler should return Ok");

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(success, "Should succeed enabling guest account");
                assert!(error.is_none(), "Should have no error");
            }
            _ => panic!("Expected UserUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_userupdate_guest_account_permissions() {
        use crate::handlers::testing::read_server_message;

        let mut test_ctx = create_test_context().await;

        // Create admin user
        let password = "password";
        let hashed = hash_password(password).expect("hash should work");
        test_ctx
            .db
            .users
            .create_user("admin", &hashed, true, false, true, &Permissions::new())
            .await
            .unwrap();

        // Login as admin
        let admin_id = test_ctx
            .user_manager
            .add_user(crate::users::user::NewSessionParams {
                session_id: 0,
                db_user_id: 1,
                username: "admin".to_string(),
                is_admin: true,
                is_shared: false,
                permissions: std::collections::HashSet::new(),
                address: test_ctx.peer_addr,
                created_at: 0,
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: DEFAULT_TEST_LOCALE.to_string(),
                avatar: None,
                nickname: "admin".to_string(),
                is_away: false,
                status: None,
            })
            .await
            .unwrap();

        // Update guest account permissions (should succeed with allowed permissions)
        let request = UserUpdateRequest {
            username: "guest".to_string(),
            current_password: None,
            requested_username: None,
            requested_password: None,
            requested_is_admin: None,
            requested_enabled: None,
            requested_permissions: Some(vec![
                "chat_send".to_string(),
                "chat_receive".to_string(),
                "user_list".to_string(),
            ]),
            session_id: Some(admin_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Handler should return Ok");

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(success, "Should succeed updating guest permissions");
                assert!(error.is_none(), "Should have no error");
            }
            _ => panic!("Expected UserUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_userupdate_cannot_change_guest_password() {
        use crate::handlers::testing::read_server_message;

        let mut test_ctx = create_test_context().await;

        // Create admin user
        let password = "password";
        let hashed = hash_password(password).expect("hash should work");
        test_ctx
            .db
            .users
            .create_user("admin", &hashed, true, false, true, &Permissions::new())
            .await
            .unwrap();

        // Login as admin
        let admin_id = test_ctx
            .user_manager
            .add_user(crate::users::user::NewSessionParams {
                session_id: 0,
                db_user_id: 1,
                username: "admin".to_string(),
                is_admin: true,
                is_shared: false,
                permissions: std::collections::HashSet::new(),
                address: test_ctx.peer_addr,
                created_at: 0,
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: DEFAULT_TEST_LOCALE.to_string(),
                avatar: None,
                nickname: "admin".to_string(),
                is_away: false,
                status: None,
            })
            .await
            .unwrap();

        // Try to change guest account password
        let request = UserUpdateRequest {
            username: "guest".to_string(),
            current_password: None,
            requested_username: None,
            requested_password: Some("newpassword".to_string()),
            requested_is_admin: None,
            requested_enabled: None,
            requested_permissions: None,
            session_id: Some(admin_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Handler should return Ok");

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(!success, "Should fail to change guest password");
                assert!(error.is_some(), "Should have error message");
            }
            _ => panic!("Expected UserUpdateResponse"),
        }
    }
}
