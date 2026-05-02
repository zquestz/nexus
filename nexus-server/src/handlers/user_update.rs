//! UserUpdate message handler

use std::io;

use tokio::io::AsyncWrite;
use tracing::{error, info, warn};

use nexus_common::is_shared_account_permission;
use nexus_common::protocol::{ServerMessage, UserInfo};
use nexus_common::validators::{self, PasswordError, PermissionsError, UsernameError};

use crate::constants::{
    DEFAULT_LOCALE, LOG_USER_UPDATE_ADMIN, LOG_USER_UPDATE_DB_ERROR,
    LOG_USER_UPDATE_DB_ERROR_GROUP, LOG_USER_UPDATE_DB_ERROR_GROUP_PERMS,
    LOG_USER_UPDATE_DB_ERROR_LOOKUP, LOG_USER_UPDATE_DB_ERROR_PERMISSIONS,
    LOG_USER_UPDATE_DB_ERROR_TARGET, LOG_USER_UPDATE_DB_ERROR_USER, LOG_USER_UPDATE_HASH_ERROR,
    LOG_USER_UPDATE_NOT_LOGGED_IN, LOG_USER_UPDATE_PASSWORD_VERIFY,
    LOG_USER_UPDATE_PERMISSION_DENIED, LOG_USER_UPDATE_SUCCESS, LOG_USER_UPDATE_UNOWNED_PERMISSION,
    LOG_USER_UPDATE_UNOWNED_REVOKE,
};

#[cfg(test)]
use super::testing::DEFAULT_TEST_LOCALE;
use super::{
    HandlerContext, err_account_disabled_by_admin, err_authentication,
    err_cannot_change_guest_password, err_cannot_demote_last_admin, err_cannot_disable_last_admin,
    err_cannot_edit_admin, err_cannot_edit_self, err_cannot_rename_guest,
    err_current_password_incorrect, err_current_password_required, err_database,
    err_group_not_found, err_group_shared_mismatch, err_not_logged_in, err_password_empty,
    err_password_too_long, err_password_too_weak, err_permission_denied,
    err_permissions_contains_newlines, err_permissions_empty_permission,
    err_permissions_invalid_characters, err_permissions_permission_too_long,
    err_permissions_too_many, err_shared_cannot_change_password, err_shared_invalid_permissions,
    err_unknown_permission, err_update_failed, err_user_not_found, err_username_empty,
    err_username_exists, err_username_invalid, err_username_too_long,
    remove_user_with_voice_cleanup,
};
use super::{ServerInfoOptions, ServerInfoValues, build_server_info};
#[cfg(test)]
use crate::db::hash_password;
use crate::db::sql::GUEST_USERNAME;
use crate::db::{
    Permission, Permissions, UpdateUserParams, hash_password_async, verify_password_async,
};
use crate::voice::send_voice_leave_notifications;

/// User update request parameters
pub struct UserUpdateRequest {
    pub id: i64,
    pub current_password: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub is_admin: Option<bool>,
    pub enabled: Option<bool>,
    pub permissions: Option<Vec<String>>,
    pub group_id: Option<i64>,
    pub remove_group: Option<bool>,
    pub revokes: Option<Vec<String>>,
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
    // Verify authentication
    let Some(requesting_session_id) = request.session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_USER_UPDATE_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("UserUpdate"))
            .await;
    };

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

    // Look up target user by ID
    let target_account = match ctx.db.users.get_user_by_id(request.id).await {
        Ok(Some(account)) => account,
        Ok(None) => {
            let response = ServerMessage::UserUpdateResponse {
                success: false,
                error: Some(err_user_not_found(ctx.locale, &request.id.to_string())),
                id: None,
                username: None,
            };
            return ctx.send_message(&response).await;
        }
        Err(e) => {
            error!(user = %requesting_user.username, ip = %ctx.peer_addr, target = %request.id, err = %e, "{}", LOG_USER_UPDATE_DB_ERROR_LOOKUP);
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some("UserUpdate"))
                .await;
        }
    };
    let target_username = target_account.username.clone();

    // Validate target username format
    if let Err(e) = validators::validate_username(&target_username) {
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
            id: None,
            username: None,
        };
        return ctx.send_message(&response).await;
    }

    // Check if this is a self-edit (user changing their own password)
    let is_self_edit = target_username.to_lowercase() == requesting_user.username.to_lowercase();

    if is_self_edit {
        // Shared accounts cannot change their own password
        if requesting_user.is_shared {
            let response = ServerMessage::UserUpdateResponse {
                success: false,
                error: Some(err_shared_cannot_change_password(ctx.locale)),
                id: None,
                username: None,
            };
            return ctx.send_message(&response).await;
        }

        // Self-edit: only password change is allowed
        // Reject if trying to change anything other than password
        if request.username.is_some()
            || request.is_admin.is_some()
            || request.enabled.is_some()
            || request.permissions.is_some()
            || request.group_id.is_some()
            || request.remove_group == Some(true)
            || request.revokes.is_some()
        {
            let response = ServerMessage::UserUpdateResponse {
                success: false,
                error: Some(err_cannot_edit_self(ctx.locale)),
                id: None,
                username: None,
            };
            return ctx.send_message(&response).await;
        }

        // Password change requires current_password
        let Some(ref current_password) = request.current_password else {
            let response = ServerMessage::UserUpdateResponse {
                success: false,
                error: Some(err_current_password_required(ctx.locale)),
                id: None,
                username: None,
            };
            return ctx.send_message(&response).await;
        };

        // Verify current password against database
        let password_hash = match ctx.db.users.get_user_by_username(&target_username).await {
            Ok(Some(user)) => user.hashed_password,
            Ok(None) => {
                let response = ServerMessage::UserUpdateResponse {
                    success: false,
                    error: Some(err_user_not_found(ctx.locale, &target_username)),
                    id: None,
                    username: None,
                };
                return ctx.send_message(&response).await;
            }
            Err(e) => {
                error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_USER_UPDATE_DB_ERROR_USER);
                return ctx
                    .send_error_and_disconnect(&err_database(ctx.locale), Some("UserUpdate"))
                    .await;
            }
        };

        // Verify the current password
        match verify_password_async(current_password.to_string(), password_hash.clone()).await {
            Ok(true) => {} // Password correct, continue
            Ok(false) => {
                let response = ServerMessage::UserUpdateResponse {
                    success: false,
                    error: Some(err_current_password_incorrect(ctx.locale)),
                    id: None,
                    username: None,
                };
                return ctx.send_message(&response).await;
            }
            Err(e) => {
                error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_USER_UPDATE_PASSWORD_VERIFY);
                return ctx
                    .send_error_and_disconnect(&err_database(ctx.locale), Some("UserUpdate"))
                    .await;
            }
        }
    } else {
        // Editing another user: check UserEdit permission
        if !requesting_user.has_permission(Permission::UserEdit) {
            warn!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_USER_UPDATE_PERMISSION_DENIED);
            let response = ServerMessage::UserUpdateResponse {
                success: false,
                error: Some(err_permission_denied(ctx.locale)),
                id: None,
                username: None,
            };
            return ctx.send_message(&response).await;
        }

        // Prevent non-admins from editing admin users
        // Look up target user to check their admin status
        if !requesting_user.is_admin {
            match ctx.db.users.get_user_by_username(&target_username).await {
                Ok(Some(target_user)) if target_user.is_admin => {
                    warn!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_USER_UPDATE_ADMIN);
                    let response = ServerMessage::UserUpdateResponse {
                        success: false,
                        error: Some(err_cannot_edit_admin(ctx.locale)),
                        id: None,
                        username: None,
                    };
                    return ctx.send_message(&response).await;
                }
                Ok(Some(_)) => {} // Target is not admin, proceed
                Ok(None) => {
                    let response = ServerMessage::UserUpdateResponse {
                        success: false,
                        error: Some(err_user_not_found(ctx.locale, &target_username)),
                        id: None,
                        username: None,
                    };
                    return ctx.send_message(&response).await;
                }
                Err(e) => {
                    error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_USER_UPDATE_DB_ERROR_TARGET);
                    return ctx
                        .send_error_and_disconnect(&err_database(ctx.locale), Some("UserUpdate"))
                        .await;
                }
            }
        }
    }

    // Validate new username format if it's being changed
    if let Some(ref new_username) = request.username
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
            id: None,
            username: None,
        };
        return ctx.send_message(&response).await;
    }

    // Prevent renaming the guest account
    if let Some(ref new_username) = request.username
        && target_username.to_lowercase() == GUEST_USERNAME
        && new_username.to_lowercase() != GUEST_USERNAME
    {
        let response = ServerMessage::UserUpdateResponse {
            success: false,
            error: Some(err_cannot_rename_guest(ctx.locale)),
            id: None,
            username: None,
        };
        return ctx.send_message(&response).await;
    }

    // Prevent changing the guest account password
    if let Some(ref new_password) = request.password
        && !new_password.trim().is_empty()
        && target_username.to_lowercase() == GUEST_USERNAME
    {
        let response = ServerMessage::UserUpdateResponse {
            success: false,
            error: Some(err_cannot_change_guest_password(ctx.locale)),
            id: None,
            username: None,
        };
        return ctx.send_message(&response).await;
    }

    // Note: Last admin protection is now handled atomically at the database level
    // in update_user() SQL query to prevent race conditions

    // Verify admin flag modification privilege (use is_admin from UserManager)
    // Skip for self-edit since we already rejected admin changes above
    if !is_self_edit && request.is_admin.is_some() && !requesting_user.is_admin {
        let response = ServerMessage::UserUpdateResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            id: None,
            username: None,
        };
        return ctx.send_message(&response).await;
    }

    // Fetch target user to check if they're a shared account (needed for permission validation)
    let target_user_account = match ctx.db.users.get_user_by_username(&target_username).await {
        Ok(Some(account)) => Some(account),
        Ok(None) => {
            let response = ServerMessage::UserUpdateResponse {
                success: false,
                error: Some(err_user_not_found(ctx.locale, &target_username)),
                id: None,
                username: None,
            };
            return ctx.send_message(&response).await;
        }
        Err(e) => {
            error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_USER_UPDATE_DB_ERROR_TARGET);
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some("UserUpdate"))
                .await;
        }
    };

    // Validate and parse requested permissions
    let parsed_permissions = if let Some(ref perm_strings) = request.permissions {
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
                    id: None,
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
                id: None,
                username: None,
            };
            return ctx.send_message(&response).await;
        }

        let mut perms = Permissions::new();
        for perm_str in perm_strings {
            let perm = match Permission::parse(perm_str) {
                Some(p) => p,
                None => {
                    let response = ServerMessage::UserUpdateResponse {
                        success: false,
                        error: Some(err_unknown_permission(ctx.locale, perm_str)),
                        id: None,
                        username: None,
                    };
                    return ctx.send_message(&response).await;
                }
            };

            // Check permission delegation authority (uses cached permissions, admin bypass built-in)
            if !requesting_user.has_permission(perm) {
                warn!(user = %requesting_user.username, ip = %ctx.peer_addr, perm = %perm_str, "{}", LOG_USER_UPDATE_UNOWNED_PERMISSION);
                let response = ServerMessage::UserUpdateResponse {
                    success: false,
                    error: Some(err_permission_denied(ctx.locale)),
                    id: None,
                    username: None,
                };
                return ctx.send_message(&response).await;
            }

            perms.permissions.insert(perm);
        }

        // Apply permission merge logic for non-admins: preserve permissions
        // the requester can't control, layer in their requested changes
        if !requesting_user.is_admin
            && let Some(ref account) = target_user_account
        {
            let target_perms = match ctx.db.users.get_user_permissions(account.id).await {
                Ok(p) => p,
                Err(e) => {
                    error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_USER_UPDATE_DB_ERROR_PERMISSIONS);
                    return ctx
                        .send_error_and_disconnect(&err_database(ctx.locale), Some("UserUpdate"))
                        .await;
                }
            };

            let mut final_perms = Permissions::new();

            // Preserve target's permissions the requester can't control
            for target_perm in &target_perms.permissions {
                if !requesting_user.has_permission(*target_perm) {
                    final_perms.permissions.insert(*target_perm);
                }
            }

            // Add all requested permissions (already validated as requester-held)
            for requested_perm in &perms.permissions {
                final_perms.permissions.insert(*requested_perm);
            }

            perms = final_perms;
        }

        Some(perms)
    } else {
        None
    };

    // Validate group assignment/removal. DB writes happen atomically inside
    // update_user's transaction via remove_group + group_id.
    let (validated_remove_group, validated_group_id): (bool, Option<i64>) = if !is_self_edit {
        if request.remove_group == Some(true) {
            // Remove from group — takes precedence over group_id
            if let Some(ref account) = target_user_account {
                if let Some(current_group_id) = account.group_id {
                    // Non-admin delegation: requester must have all current group
                    // permissions (removal changes effective perms the editor can't grant back)
                    if !requesting_user.is_admin {
                        let group_perms = match ctx
                            .db
                            .groups
                            .get_group_permissions(current_group_id)
                            .await
                        {
                            Ok(p) => p,
                            Err(e) => {
                                error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_USER_UPDATE_DB_ERROR_GROUP_PERMS);
                                let response = ServerMessage::UserUpdateResponse {
                                    success: false,
                                    error: Some(err_database(ctx.locale)),
                                    id: None,
                                    username: None,
                                };
                                return ctx.send_message(&response).await;
                            }
                        };
                        for perm in &group_perms {
                            if !requesting_user.has_permission(*perm) {
                                let response = ServerMessage::UserUpdateResponse {
                                    success: false,
                                    error: Some(err_permission_denied(ctx.locale)),
                                    id: None,
                                    username: None,
                                };
                                return ctx.send_message(&response).await;
                            }
                        }
                    }
                    (true, None)
                } else {
                    (false, None) // Already no group
                }
            } else {
                (false, None)
            }
        } else if let Some(new_group_id) = request.group_id {
            if let Some(ref account) = target_user_account {
                // Skip if already in this group
                if account.group_id == Some(new_group_id) {
                    (false, None)
                } else {
                    // Non-admin delegation: requester must have all current group
                    // permissions (moving away removes them, same check as remove_group)
                    if !requesting_user.is_admin
                        && let Some(current_group_id) = account.group_id
                    {
                        let old_group_perms = match ctx
                            .db
                            .groups
                            .get_group_permissions(current_group_id)
                            .await
                        {
                            Ok(p) => p,
                            Err(e) => {
                                error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_USER_UPDATE_DB_ERROR_GROUP_PERMS);
                                let response = ServerMessage::UserUpdateResponse {
                                    success: false,
                                    error: Some(err_database(ctx.locale)),
                                    id: None,
                                    username: None,
                                };
                                return ctx.send_message(&response).await;
                            }
                        };
                        for perm in &old_group_perms {
                            if !requesting_user.has_permission(*perm) {
                                let response = ServerMessage::UserUpdateResponse {
                                    success: false,
                                    error: Some(err_permission_denied(ctx.locale)),
                                    id: None,
                                    username: None,
                                };
                                return ctx.send_message(&response).await;
                            }
                        }
                    }

                    // Fetch the group
                    let group = match ctx.db.groups.get_group_by_id(new_group_id).await {
                        Ok(Some(g)) => g,
                        Ok(None) => {
                            let response = ServerMessage::UserUpdateResponse {
                                success: false,
                                error: Some(err_group_not_found(ctx.locale)),
                                id: None,
                                username: None,
                            };
                            return ctx.send_message(&response).await;
                        }
                        Err(e) => {
                            error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_USER_UPDATE_DB_ERROR_GROUP);
                            return ctx
                                .send_error_and_disconnect(
                                    &err_database(ctx.locale),
                                    Some("UserUpdate"),
                                )
                                .await;
                        }
                    };

                    // Shared compatibility check
                    if account.is_shared && !group.is_shared {
                        let response = ServerMessage::UserUpdateResponse {
                            success: false,
                            error: Some(err_group_shared_mismatch(ctx.locale)),
                            id: None,
                            username: None,
                        };
                        return ctx.send_message(&response).await;
                    }
                    if !account.is_shared && group.is_shared {
                        let response = ServerMessage::UserUpdateResponse {
                            success: false,
                            error: Some(err_group_shared_mismatch(ctx.locale)),
                            id: None,
                            username: None,
                        };
                        return ctx.send_message(&response).await;
                    }

                    // Non-admin delegation: requester must have all group permissions
                    if !requesting_user.is_admin {
                        let group_perms = match ctx
                            .db
                            .groups
                            .get_group_permissions(new_group_id)
                            .await
                        {
                            Ok(p) => p,
                            Err(e) => {
                                error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_USER_UPDATE_DB_ERROR_GROUP_PERMS);
                                let response = ServerMessage::UserUpdateResponse {
                                    success: false,
                                    error: Some(err_database(ctx.locale)),
                                    id: None,
                                    username: None,
                                };
                                return ctx.send_message(&response).await;
                            }
                        };
                        for perm in &group_perms {
                            if !requesting_user.has_permission(*perm) {
                                let response = ServerMessage::UserUpdateResponse {
                                    success: false,
                                    error: Some(err_permission_denied(ctx.locale)),
                                    id: None,
                                    username: None,
                                };
                                return ctx.send_message(&response).await;
                            }
                        }
                    }

                    (false, Some(new_group_id))
                }
            } else {
                (false, None)
            }
        } else {
            (false, None)
        }
    } else {
        (false, None)
    };

    // Handle revoke override changes
    // Parse and validate revoke permissions here; DB write happens atomically
    // inside update_user's transaction via the revokes parameter.
    let parsed_revokes: Option<Vec<Permission>> = if !is_self_edit
        && let Some(ref revoke_strings) = request.revokes
        && let Some(ref account) = target_user_account
    {
        // Determine effective group_id after any group change
        let effective_group_id = if request.remove_group == Some(true) {
            None
        } else if let Some(gid) = request.group_id {
            Some(gid)
        } else {
            account.group_id
        };

        if effective_group_id.is_some() {
            // Parse revoke permissions
            let mut parsed_revokes = Vec::new();
            for perm_str in revoke_strings {
                match Permission::parse(perm_str) {
                    Some(perm) => {
                        // Non-admins can only set revokes for permissions they have
                        if !requesting_user.is_admin && !requesting_user.has_permission(perm) {
                            warn!(user = %requesting_user.username, ip = %ctx.peer_addr, perm = %perm_str, "{}", LOG_USER_UPDATE_UNOWNED_REVOKE);
                            let response = ServerMessage::UserUpdateResponse {
                                success: false,
                                error: Some(err_permission_denied(ctx.locale)),
                                id: None,
                                username: None,
                            };
                            return ctx.send_message(&response).await;
                        }
                        parsed_revokes.push(perm);
                    }
                    None => {
                        let response = ServerMessage::UserUpdateResponse {
                            success: false,
                            error: Some(err_unknown_permission(ctx.locale, perm_str)),
                            id: None,
                            username: None,
                        };
                        return ctx.send_message(&response).await;
                    }
                }
            }

            // Non-admin merge: preserve existing revokes for permissions the
            // requester can't control (same pattern as grant permission merge)
            if !requesting_user.is_admin
                && let Ok(existing_revokes) = ctx.db.users.get_revoke_permissions(account.id).await
            {
                for existing_revoke in existing_revokes {
                    if !requesting_user.has_permission(existing_revoke)
                        && !parsed_revokes.contains(&existing_revoke)
                    {
                        parsed_revokes.push(existing_revoke);
                    }
                }
            }

            Some(parsed_revokes)
        } else {
            None
        }
    } else {
        None
    };

    // Process password change request
    let requested_password_hash = if let Some(ref password) = request.password {
        // Empty/whitespace password = no change
        if password.trim().is_empty() {
            None
        } else {
            // Validate password format
            let min_strength = ctx.db.config.get_min_password_strength().await;
            if let Err(e) =
                validators::validate_password(password, min_strength, &[&target_username])
            {
                let error_msg = match e {
                    PasswordError::Empty => err_password_empty(ctx.locale),
                    PasswordError::TooLong => {
                        err_password_too_long(ctx.locale, validators::MAX_PASSWORD_LENGTH)
                    }
                    PasswordError::TooWeak { required, .. } => {
                        err_password_too_weak(ctx.locale, required.score())
                    }
                };
                let response = ServerMessage::UserUpdateResponse {
                    success: false,
                    error: Some(error_msg),
                    id: None,
                    username: None,
                };
                return ctx.send_message(&response).await;
            }
            match hash_password_async(password.clone(), min_strength, false).await {
                Ok(hash) => Some(hash),
                Err(e) => {
                    error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_USER_UPDATE_HASH_ERROR);
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

    // Get old state before update (to detect actual changes for PermissionsUpdated and UserUpdated)
    // We need: username, is_admin, enabled, and permissions
    // NOTE: This must be captured BEFORE the group change is applied so the diff is accurate
    let (old_username, old_is_admin, old_enabled, old_permissions) = {
        // We already fetched target_user_account above, use it
        if let Some(ref account) = target_user_account {
            let perms = ctx
                .db
                .users
                .get_user_permissions(account.id)
                .await
                .unwrap_or_else(|_| Permissions::new());
            (
                account.username.clone(),
                account.is_admin,
                account.enabled,
                perms,
            )
        } else {
            // Should not happen - we already checked user exists above
            (target_username.clone(), false, true, Permissions::new())
        }
    };

    // Attempt to update the user (with atomic last-admin protection in SQL)
    match ctx
        .db
        .users
        .update_user(UpdateUserParams {
            username: &target_username,
            new_username: request.username.as_deref(),
            new_password_hash: requested_password_hash.as_deref(),
            is_admin: request.is_admin,
            enabled: request.enabled,
            permissions: parsed_permissions.as_ref(),
            revokes: parsed_revokes.as_deref(),
            remove_group: validated_remove_group,
            group_id: validated_group_id,
        })
        .await
    {
        Ok(true) => {
            // Success - send response to requester
            // Use the final username (in case it changed)
            let final_username = request
                .username
                .as_ref()
                .unwrap_or(&target_username)
                .clone();

            info!(user = %requesting_user.username, ip = %ctx.peer_addr, target = %final_username, "{}", LOG_USER_UPDATE_SUCCESS);
            let response = ServerMessage::UserUpdateResponse {
                success: true,
                error: None,
                id: Some(request.id),
                username: Some(final_username.clone()),
            };

            ctx.send_message(&response).await?;

            let group_changed = validated_remove_group || validated_group_id.is_some();

            // We'll determine if permissions actually changed after fetching new state

            // Get the updated user's account
            if let Ok(Some(updated_account)) =
                ctx.db.users.get_user_by_username(&final_username).await
            {
                // Get the final permissions
                if let Ok(final_permissions) =
                    ctx.db.users.get_user_permissions(updated_account.id).await
                {
                    // Check if anything actually changed
                    let admin_changed = old_is_admin != updated_account.is_admin;
                    let enabled_changed = old_enabled != updated_account.enabled;
                    let permissions_changed =
                        old_permissions.permissions != final_permissions.permissions;
                    let actually_changed =
                        admin_changed || enabled_changed || permissions_changed || group_changed;

                    // Always update cached permissions in UserManager for all sessions of this user
                    // (even if we don't broadcast, keeps cache in sync)
                    ctx.user_manager
                        .update_permissions(
                            updated_account.id,
                            final_permissions.permissions.clone(),
                        )
                        .await;

                    // Only notify the user if their permissions/admin/enabled status actually changed
                    if actually_changed {
                        let permission_strings: Vec<String> = final_permissions
                            .permissions
                            .iter()
                            .map(|p| p.as_str().to_string())
                            .collect();

                        // Build ServerInfo for the updated user (always include, not just for admins)
                        // Use updated permissions for field visibility
                        let has_file_reindex = updated_account.is_admin
                            || final_permissions
                                .permissions
                                .contains(&Permission::FileReindex);
                        let has_chat_join = updated_account.is_admin
                            || final_permissions
                                .permissions
                                .contains(&Permission::ChatJoin);

                        let config = ctx.db.config.get_all().await;

                        let info_values = ServerInfoValues {
                            name: config.server_name,
                            description: config.server_description,
                            public_address: config.public_address,
                            version: env!("CARGO_PKG_VERSION").to_string(),
                            image: config.server_image,
                            max_connections_per_ip: config.max_connections_per_ip,
                            max_transfers_per_ip: config.max_transfers_per_ip,
                            transfer_port: ctx.transfer_port,
                            transfer_websocket_port: ctx.transfer_websocket_port,
                            file_reindex_interval: config.file_reindex_interval,
                            persistent_channels: config.persistent_channels,
                            auto_join_channels: config.auto_join_channels,
                            min_password_strength: config.min_password_strength.score(),
                            chat_burst_limit: config.chat_burst_limit,
                            chat_rate_limit: config.chat_rate_limit,
                        };

                        let info_options = ServerInfoOptions {
                            is_admin: updated_account.is_admin,
                            has_file_reindex,
                            has_chat_join,
                            include_image: false,
                        };

                        let server_info = Some(build_server_info(&info_values, &info_options));

                        // Fetch group info for the updated user
                        let (perm_group_id, perm_group_name) =
                            if let Some(gid) = updated_account.group_id {
                                match ctx.db.groups.get_group_by_id(gid).await {
                                    Ok(Some(g)) => (Some(gid), Some(g.name)),
                                    _ => (None, None),
                                }
                            } else {
                                (None, None)
                            };

                        let permissions_update = ServerMessage::PermissionsUpdated {
                            is_admin: updated_account.is_admin,
                            permissions: permission_strings,
                            server_info,
                            group_id: perm_group_id,
                            group_name: perm_group_name,
                        };

                        // Send to all sessions belonging to the updated user
                        ctx.user_manager
                            .broadcast_to_username(&updated_account.username, &permissions_update)
                            .await;

                        // If voice_listen was revoked, kick user from voice
                        let had_voice_listen = old_permissions
                            .permissions
                            .contains(&Permission::VoiceListen);
                        let has_voice_listen = final_permissions
                            .permissions
                            .contains(&Permission::VoiceListen);

                        if had_voice_listen && !has_voice_listen {
                            // Get all session IDs for this user and remove them from voice
                            let session_ids = ctx
                                .user_manager
                                .get_session_ids_for_user(&updated_account.username)
                                .await;

                            for session_id in session_ids {
                                if let Some(info) =
                                    ctx.voice_registry.remove_by_session_id(session_id).await
                                {
                                    // Get the leaving user's tx if still connected
                                    let leaving_user_tx = ctx
                                        .user_manager
                                        .get_user_by_session_id(session_id)
                                        .await
                                        .map(|u| u.tx.clone());

                                    // Send notifications using the consolidated helper
                                    send_voice_leave_notifications(
                                        &info,
                                        leaving_user_tx.as_ref(),
                                        ctx.user_manager,
                                        ctx.channel_manager,
                                    )
                                    .await;
                                }
                            }
                        }
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
                if let Some(false) = request.enabled {
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

                            // Remove from voice (if in voice) and UserManager, broadcast disconnection
                            remove_user_with_voice_cleanup(
                                ctx.user_manager,
                                ctx.voice_registry,
                                ctx.channel_manager,
                                session_id,
                                &user,
                            )
                            .await;
                        }
                    }
                }

                // Check if username or admin status changed
                let username_changed =
                    old_username.to_lowercase() != updated_account.username.to_lowercase();
                let admin_status_changed = old_is_admin != updated_account.is_admin;

                // If username changed, update UserManager and VoiceRegistry
                // (for regular accounts, nickname == username)
                if username_changed {
                    ctx.user_manager
                        .update_username(updated_account.id, updated_account.username.clone())
                        .await;

                    // Update nickname in voice registry for all sessions of this user
                    let session_ids = ctx
                        .user_manager
                        .get_session_ids_for_user(&updated_account.username)
                        .await;
                    for session_id in session_ids {
                        ctx.voice_registry
                            .update_nickname(session_id, updated_account.username.clone())
                            .await;
                    }
                }

                // If admin status changed, update UserManager
                if admin_status_changed {
                    ctx.user_manager
                        .update_admin_status(updated_account.id, updated_account.is_admin)
                        .await;
                }

                // Resolve group info once (used by both update_group and UserUpdated broadcast)
                let (updated_group_id, updated_group_name) =
                    if let Some(gid) = updated_account.group_id {
                        match ctx.db.groups.get_group_by_id(gid).await {
                            Ok(Some(g)) => (Some(gid), Some(g.name)),
                            _ => (None, None),
                        }
                    } else {
                        (None, None)
                    };

                // If group changed, update UserManager sessions
                if group_changed {
                    ctx.user_manager
                        .update_group(
                            updated_account.id,
                            updated_group_id,
                            updated_group_name.clone(),
                        )
                        .await;
                }

                // Broadcast UserUpdated if username, admin status, or group changed
                if username_changed || admin_status_changed || group_changed {
                    let session_ids = ctx
                        .user_manager
                        .get_session_ids_for_user(&updated_account.username)
                        .await;

                    let (login_time, locale, avatar, is_away, status) = if !session_ids.is_empty() {
                        let user_sessions = ctx
                            .user_manager
                            .get_sessions_by_username(&updated_account.username)
                            .await;

                        let login_time = user_sessions
                            .iter()
                            .map(|u| u.login_time)
                            .min()
                            .unwrap_or(0);

                        // Avatar, locale: latest login wins (stable)
                        let latest_login = user_sessions.iter().max_by_key(|u| u.login_time);

                        let locale = latest_login
                            .map(|u| u.locale.clone())
                            .unwrap_or_else(|| DEFAULT_LOCALE.to_string());

                        let avatar = latest_login.and_then(|u| u.avatar.clone());

                        // Away/status: most recently active wins (accurate presence)
                        let most_active = user_sessions.iter().max_by_key(|u| u.last_activity);

                        let is_away = most_active.is_some_and(|u| u.is_away);
                        let status = most_active.and_then(|u| u.status.clone());

                        (login_time, locale, avatar, is_away, status)
                    } else {
                        (0, DEFAULT_LOCALE.to_string(), None, false, None)
                    };

                    let user_info = UserInfo {
                        id: updated_account.id,
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
                        is_away,
                        status,
                        group_id: updated_group_id,
                        group_name: updated_group_name,
                    };

                    let user_updated = ServerMessage::UserUpdated {
                        previous_username: old_username.clone(),
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
                .get_user_by_username(&target_username)
                .await
                .ok()
                .flatten()
                .is_none()
            {
                err_user_not_found(ctx.locale, &target_username)
            } else if let Some(ref new_username) = request.username {
                // Check if the new username already exists (and it's not the same user)
                if new_username != &target_username
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
            } else if request.is_admin == Some(false) {
                err_cannot_demote_last_admin(ctx.locale)
            } else if request.enabled == Some(false) {
                err_cannot_disable_last_admin(ctx.locale)
            } else {
                err_update_failed(ctx.locale, &target_username)
            };

            let response = ServerMessage::UserUpdateResponse {
                success: false,
                error: Some(error_message.to_string()),
                id: None,
                username: None,
            };
            ctx.send_message(&response).await
        }
        Err(e) => {
            error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_USER_UPDATE_DB_ERROR);
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
    #[allow(unused_imports)]
    use crate::handlers::testing::read_login_response;
    use crate::handlers::testing::*;
    use crate::users::user::NewSessionParams;

    #[tokio::test]
    async fn test_userupdate_requires_login() {
        let mut test_ctx = create_test_context().await;

        // Look up a user id to use (doesn't matter which since not logged in)
        // Use a non-existent id since the test checks login requirement before user lookup
        let request = UserUpdateRequest {
            id: 99999,
            current_password: None,
            username: Some("alice2".to_string()),
            password: None,
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
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
        let bob = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
            })
            .await
            .unwrap();

        let request = UserUpdateRequest {
            id: bob.id,
            current_password: None,
            username: Some("bob2".to_string()),
            password: None,
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse {
                success,
                error,
                id,
                username,
            } => {
                assert!(!success);
                assert!(error.is_some());
                assert_eq!(error.unwrap(), err_permission_denied(DEFAULT_TEST_LOCALE));
                assert!(id.is_none());
                assert!(username.is_none());
            }
            _ => panic!("Expected UserUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_userupdate_cannot_edit_self_username() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let admin_user = test_ctx
            .db
            .users
            .get_user_by_username("admin")
            .await
            .unwrap()
            .unwrap();
        let request = UserUpdateRequest {
            id: admin_user.id,
            current_password: None,
            username: Some("admin2".to_string()),
            password: None,
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
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
        let admin_user = test_ctx
            .db
            .users
            .get_user_by_username("admin")
            .await
            .unwrap()
            .unwrap();
        let request = UserUpdateRequest {
            id: admin_user.id,
            current_password: Some("password".to_string()),
            username: None,
            password: None,
            is_admin: Some(false), // Trying to demote self
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
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
        let admin_user = test_ctx
            .db
            .users
            .get_user_by_username("admin")
            .await
            .unwrap()
            .unwrap();
        let request = UserUpdateRequest {
            id: admin_user.id,
            current_password: Some("password".to_string()),
            username: None,
            password: None,
            is_admin: None,
            enabled: Some(false), // Trying to disable self
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
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
        let alice_user = test_ctx
            .db
            .users
            .get_user_by_username("alice")
            .await
            .unwrap()
            .unwrap();
        let request = UserUpdateRequest {
            id: alice_user.id,
            current_password: Some("password".to_string()),
            username: None,
            password: None,
            is_admin: None,
            enabled: None,
            permissions: Some(vec!["user_edit".to_string()]), // Trying to give self more permissions
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
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
        let alice_user = test_ctx
            .db
            .users
            .get_user_by_username("alice")
            .await
            .unwrap()
            .unwrap();
        let request = UserUpdateRequest {
            id: alice_user.id,
            current_password: Some("oldpassword".to_string()),
            username: None,
            password: Some("newpassword".to_string()),
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse {
                success,
                error,
                id,
                username,
            } => {
                assert!(success, "Expected success, got error: {:?}", error);
                assert!(error.is_none());
                assert!(id.is_some());
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
        let alice_user = test_ctx
            .db
            .users
            .get_user_by_username("alice")
            .await
            .unwrap()
            .unwrap();
        let request = UserUpdateRequest {
            id: alice_user.id,
            current_password: Some("wrongpassword".to_string()),
            username: None,
            password: Some("newpassword".to_string()),
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
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
        let admin_user = test_ctx
            .db
            .users
            .get_user_by_username("admin")
            .await
            .unwrap()
            .unwrap();
        let request = UserUpdateRequest {
            id: admin_user.id,
            current_password: None,
            username: None,
            password: Some("newpassword".to_string()),
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
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
        let bob = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
            })
            .await
            .unwrap();

        let request = UserUpdateRequest {
            id: bob.id,
            current_password: None,
            username: Some("bobby".to_string()),
            password: None,
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse {
                success,
                error,
                id,
                username,
            } => {
                assert!(success);
                assert!(error.is_none());
                assert!(id.is_some());
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
            id: 99999, // Non-existent user ID
            current_password: None,
            username: Some("newname".to_string()),
            password: None,
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(
                    error.unwrap(),
                    err_user_not_found(DEFAULT_TEST_LOCALE, "99999")
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
        let admin2_user = test_ctx
            .db
            .users
            .get_user_by_username("admin2")
            .await
            .unwrap()
            .unwrap();
        let request = UserUpdateRequest {
            id: admin2_user.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: Some(false), // Demote to non-admin
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(admin1_session),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse {
                success,
                error,
                id,
                username,
            } => {
                assert!(success);
                assert!(error.is_none());
                assert!(id.is_some());
                assert_eq!(username, Some("admin2".to_string()));
            }
            _ => panic!("Expected UserUpdateResponse"),
        }

        // Now admin2 tries to demote admin1 (should fail - no permission)
        let admin1_user = test_ctx
            .db
            .users
            .get_user_by_username("admin1")
            .await
            .unwrap()
            .unwrap();
        let request = UserUpdateRequest {
            id: admin1_user.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: Some(false), // Try to demote last admin
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(admin2_session),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse {
                success,
                error,
                id,
                username,
            } => {
                assert!(!success);
                assert!(error.is_some());
                assert_eq!(error.unwrap(), err_permission_denied(DEFAULT_TEST_LOCALE));
                assert!(id.is_none());
                assert!(username.is_none());
            }
            _ => panic!("Expected UserUpdateResponse"),
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
        let bob = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
            })
            .await
            .unwrap();

        let request = UserUpdateRequest {
            id: bob.id,
            current_password: None,
            username: Some("robert".to_string()),
            password: None,
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse {
                success,
                error,
                id,
                username,
            } => {
                assert!(success);
                assert!(error.is_none());
                assert!(id.is_some());
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
        let bob = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
            })
            .await
            .unwrap();

        // Try to make bob an admin
        let request = UserUpdateRequest {
            id: bob.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: Some(true), // Try to make admin
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse {
                success,
                error,
                id,
                username,
            } => {
                assert!(!success);
                assert!(error.is_some());
                assert_eq!(error.unwrap(), err_permission_denied(DEFAULT_TEST_LOCALE));
                assert!(id.is_none());
                assert!(username.is_none());
            }
            _ => panic!("Expected UserUpdateResponse"),
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
            .create_user(db::CreateUserParams {
                username: "alice",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
            })
            .await
            .unwrap();
        let bob = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
            })
            .await
            .unwrap();

        // Try to rename bob to alice (should fail)
        let request = UserUpdateRequest {
            id: bob.id,
            current_password: None,
            username: Some("alice".to_string()),
            password: None,
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
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
        let alice = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "alice",
                hashed_password: "oldhash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
            })
            .await
            .unwrap();

        // Change alice's password
        let request = UserUpdateRequest {
            id: alice.id,
            current_password: None,
            username: None,
            password: Some("newpassword".to_string()),
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse {
                success,
                error,
                id,
                username,
            } => {
                assert!(success);
                assert!(error.is_none());
                assert!(id.is_some());
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
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
            })
            .await
            .unwrap();

        // Give bob some permissions
        let request = UserUpdateRequest {
            id: bob.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: None,
            enabled: None,
            permissions: Some(vec!["user_list".to_string(), "chat_send".to_string()]),
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse {
                success,
                error,
                id,
                username,
            } => {
                assert!(success);
                assert!(error.is_none());
                assert!(id.is_some());
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
        let alice = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "alice",
                hashed_password: original_hash,
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
            })
            .await
            .unwrap();

        // Try to edit alice with empty password (should not change password)
        let request = UserUpdateRequest {
            id: alice.id,
            current_password: None,
            username: None,
            password: Some("".to_string()), // Empty password
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse {
                success,
                error,
                id,
                username,
            } => {
                assert!(success);
                assert!(error.is_none());
                assert!(id.is_some());
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
            id: alice.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: None,
            enabled: None,
            permissions: Some(vec!["user_list".to_string()]), // Bob only grants user_list
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(bob_session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse {
                success,
                error,
                id,
                username,
            } => {
                assert!(success, "Update should succeed with merged permissions");
                assert!(error.is_none());
                assert!(id.is_some());
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
    async fn test_userupdate_unknown_revoke_permission_returns_error() {
        let mut test_ctx = create_test_context().await;

        // Create target user with a group
        let group = test_ctx
            .db
            .groups
            .create_group(
                "Staff",
                false,
                &db::Permissions::from(&[db::Permission::ChatSend]),
            )
            .await
            .unwrap();

        let admin_session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create bob and assign to group
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: "hashed",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &db::Permissions::new(),
                group_id: Some(group.id),
                revokes: &[],
            })
            .await
            .unwrap();

        // Try to set an unknown revoke permission
        let bob_user = test_ctx
            .db
            .users
            .get_user_by_username("bob")
            .await
            .unwrap()
            .unwrap();
        let request = UserUpdateRequest {
            id: bob_user.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: Some(vec!["totally_fake_permission".to_string()]),
            session_id: Some(admin_session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(!success, "Should fail for unknown revoke permission");
                let err = error.unwrap();
                assert!(
                    err.contains("totally_fake_permission"),
                    "Error should mention the unknown permission: {}",
                    err
                );
            }
            _ => panic!("Expected UserUpdateResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_userupdate_revokes_survive_when_permissions_also_provided() {
        let mut test_ctx = create_test_context().await;

        // Create a group with chat_send and user_kick
        let group = test_ctx
            .db
            .groups
            .create_group(
                "Staff",
                false,
                &db::Permissions::from(&[db::Permission::ChatSend, db::Permission::UserKick]),
            )
            .await
            .unwrap();

        let admin_session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create bob in the group
        let bob = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: "hashed",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &db::Permissions::new(),
                group_id: Some(group.id),
                revokes: &[],
            })
            .await
            .unwrap();

        // Send UserUpdate with BOTH permissions (grant override) and
        // revokes (revoke override). Before the fix, set_permissions_in_tx
        // would DELETE all user_permissions rows (including revokes just written),
        // then re-insert only grants — silently losing the revokes.
        let request = UserUpdateRequest {
            id: bob.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: None,
            enabled: None,
            permissions: Some(vec!["ban_create".to_string()]),
            group_id: None,
            remove_group: None,
            revokes: Some(vec!["user_kick".to_string()]),
            session_id: Some(admin_session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse { success, .. } => {
                assert!(success, "UserUpdate should succeed");
            }
            _ => panic!("Expected UserUpdateResponse, got {:?}", response),
        }

        // Verify revokes survived
        let revokes = test_ctx
            .db
            .users
            .get_revoke_permissions(bob.id)
            .await
            .unwrap();
        assert_eq!(
            revokes,
            vec![db::Permission::UserKick],
            "Revoke override should survive when permissions is also provided"
        );

        // Verify the grant override is also present
        let effective = test_ctx
            .db
            .users
            .get_user_permissions(bob.id)
            .await
            .unwrap();
        let effective_vec = effective.to_vec();
        assert!(
            effective_vec.contains(&db::Permission::BanCreate),
            "Grant override (ban_create) should be present"
        );
        assert!(
            effective_vec.contains(&db::Permission::ChatSend),
            "Group permission (chat_send) should be present"
        );
        assert!(
            !effective_vec.contains(&db::Permission::UserKick),
            "Revoked permission (user_kick) should NOT be in effective set"
        );
    }

    #[tokio::test]
    async fn test_userupdate_unknown_grant_permission_returns_error() {
        let mut test_ctx = create_test_context().await;

        let admin_session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create bob
        let bob = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: "hashed",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &db::Permissions::new(),
                group_id: None,
                revokes: &[],
            })
            .await
            .unwrap();

        // Try to set an unknown grant permission
        let request = UserUpdateRequest {
            id: bob.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: None,
            enabled: None,
            permissions: Some(vec![
                "chat_send".to_string(),
                "totally_fake_permission".to_string(),
            ]),
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(admin_session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(!success, "Should fail for unknown grant permission");
                let err = error.unwrap();
                assert!(
                    err.contains("totally_fake_permission"),
                    "Error should mention the unknown permission: {}",
                    err
                );
            }
            _ => panic!("Expected UserUpdateResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_userupdate_cannot_disable_self() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Try to disable self (will be caught by self-edit check)
        let admin_user = test_ctx
            .db
            .users
            .get_user_by_username("admin")
            .await
            .unwrap()
            .unwrap();
        let request = UserUpdateRequest {
            id: admin_user.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: None,
            enabled: Some(false), // Try to disable
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Should send error response, not disconnect");
        let response = read_server_message(&mut test_ctx).await;
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
        let admin2_user = test_ctx
            .db
            .users
            .get_user_by_username("admin2")
            .await
            .unwrap()
            .unwrap();
        let request = UserUpdateRequest {
            id: admin2_user.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: None,
            enabled: Some(false),
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(admin1_session),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
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
        let admin2_user = test_ctx
            .db
            .users
            .get_user_by_username("admin2")
            .await
            .unwrap()
            .unwrap();
        let request = UserUpdateRequest {
            id: admin2_user.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: None,
            enabled: Some(true),
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(admin1_session),
        };
        let _ = handle_user_update(request, &mut test_ctx.handler_context()).await;
        let _ = read_server_message(&mut test_ctx).await;

        // Demote admin2 and admin3 so admin1 is the only admin
        let admin2_user = test_ctx
            .db
            .users
            .get_user_by_username("admin2")
            .await
            .unwrap()
            .unwrap();
        let request = UserUpdateRequest {
            id: admin2_user.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: Some(false),
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(admin1_session),
        };
        let _ = handle_user_update(request, &mut test_ctx.handler_context()).await;
        let _ = read_server_message(&mut test_ctx).await;

        let admin3_user = test_ctx
            .db
            .users
            .get_user_by_username("admin3")
            .await
            .unwrap()
            .unwrap();
        let request = UserUpdateRequest {
            id: admin3_user.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: Some(false),
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(admin1_session),
        };
        let _ = handle_user_update(request, &mut test_ctx.handler_context()).await;
        let _ = read_server_message(&mut test_ctx).await;

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
            .update_user(db::UpdateUserParams {
                username: &admin1.username,
                new_username: None,
                new_password_hash: None,
                is_admin: None,
                enabled: Some(false),
                permissions: None,
                revokes: None,
                remove_group: false,
                group_id: None,
            })
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
            .create_user(db::CreateUserParams {
                username: "editor",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &perms,
                group_id: None,
                revokes: &[],
            })
            .await
            .unwrap();

        // Add editor to UserManager
        let editor_session = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                user_id: editor.id,
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
                group_id: None,
                group_name: None,
                last_activity: std::time::Instant::now(),
            })
            .await
            .expect("Failed to add user");

        // Non-admin editor tries to edit admin - should fail
        let admin_user = test_ctx
            .db
            .users
            .get_user_by_username("admin")
            .await
            .unwrap()
            .unwrap();
        let request = UserUpdateRequest {
            id: admin_user.id,
            current_password: None,
            username: None,
            password: Some("newpassword".to_string()),
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(editor_session),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Should send error response, not disconnect");
        let response = read_server_message(&mut test_ctx).await;
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
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
            })
            .await
            .unwrap();

        // Verify bob is enabled
        assert!(bob.enabled, "Bob should be enabled initially");

        // Disable bob
        let request = UserUpdateRequest {
            id: bob.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: None,
            enabled: Some(false), // Disable
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse {
                success,
                error,
                id,
                username,
            } => {
                assert!(success, "Should successfully disable user");
                assert!(error.is_none());
                assert!(id.is_some());
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
            id: bob.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: None,
            enabled: Some(true), // Enable
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse {
                success,
                error,
                id,
                username,
            } => {
                assert!(success, "Should successfully re-enable user");
                assert!(error.is_none());
                assert!(id.is_some());
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
        let bob_user = test_ctx
            .db
            .users
            .get_user_by_username("bob")
            .await
            .unwrap()
            .unwrap();
        let request = UserUpdateRequest {
            id: bob_user.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: None,
            enabled: Some(false), // Disable
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
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
            .create_user(db::CreateUserParams {
                username: "admin1",
                hashed_password: "hash1",
                is_admin: true,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
            })
            .await
            .unwrap();
        let admin2 = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "admin2",
                hashed_password: "hash2",
                is_admin: true,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
            })
            .await
            .unwrap();

        // Login both admins
        let admin1_session = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                user_id: admin1.id,
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
                group_id: None,
                group_name: None,
                last_activity: std::time::Instant::now(),
            })
            .await
            .expect("Failed to add user");

        let _admin2_session = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                user_id: admin2.id,
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
                group_id: None,
                group_name: None,
                last_activity: std::time::Instant::now(),
            })
            .await
            .expect("Failed to add user");

        // Admin1 demotes admin2 to non-admin (should succeed - 2 admins exist)
        let request = UserUpdateRequest {
            id: admin2.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: Some(false), // Demote to non-admin
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(admin1_session),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse {
                success,
                error,
                id,
                username,
            } => {
                assert!(
                    success,
                    "Should successfully demote admin2 (2 admins exist)"
                );
                assert!(error.is_none());
                assert!(id.is_some());
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
            .update_user(db::UpdateUserParams {
                username: "admin2",
                new_username: None,
                new_password_hash: None,
                is_admin: None,
                enabled: None,
                permissions: Some(&perms),
                revokes: None,
                remove_group: false,
                group_id: None,
            })
            .await
            .unwrap();

        // Admin2 tries to demote admin1 (last admin) - should fail at DB level atomically
        // Note: This bypasses the "non-admin cannot change admin status" check by using
        // the database directly to test the atomic SQL protection
        let result = test_ctx
            .db
            .users
            .update_user(db::UpdateUserParams {
                username: "admin1",
                new_username: None,
                new_password_hash: None,
                is_admin: Some(false), // Try to demote last admin
                enabled: None,
                permissions: None,
                revokes: None,
                remove_group: false,
                group_id: None,
            })
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
            .create_user(db::CreateUserParams {
                username: "shared_acct",
                hashed_password: &get_cached_password_hash("sharedpass"),
                is_admin: false,
                is_shared: true,
                enabled: true,
                permissions: &db::Permissions::new(),
                group_id: None,
                revokes: &[],
            })
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
        let _login_response = read_login_response(&mut test_ctx).await;

        // Try to change own password
        let shared_user = test_ctx
            .db
            .users
            .get_user_by_username("shared_acct")
            .await
            .unwrap()
            .unwrap();
        let request = UserUpdateRequest {
            id: shared_user.id,
            current_password: Some("sharedpass".to_string()),
            username: None,
            password: Some("newpassword".to_string()),
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: shared_session_id,
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Should send error response");

        let response = read_server_message(&mut test_ctx).await;
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
            .create_user(db::CreateUserParams {
                username: "shared_acct",
                hashed_password: &get_cached_password_hash("sharedpass"),
                is_admin: false,
                is_shared: true,
                enabled: true,
                permissions: &db::Permissions::new(),
                group_id: None,
                revokes: &[],
            })
            .await
            .expect("shared account creation should succeed");

        // Try to update shared account with forbidden permissions
        let shared_user = test_ctx
            .db
            .users
            .get_user_by_username("shared_acct")
            .await
            .unwrap()
            .unwrap();
        let request = UserUpdateRequest {
            id: shared_user.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: None,
            enabled: None,
            permissions: Some(vec![
                "chat_send".to_string(),   // allowed
                "user_kick".to_string(),   // forbidden
                "news_create".to_string(), // forbidden
            ]),
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(admin_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Should send error response");

        let response = read_server_message(&mut test_ctx).await;
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
            .create_user(db::CreateUserParams {
                username: "shared_acct",
                hashed_password: &get_cached_password_hash("sharedpass"),
                is_admin: false,
                is_shared: true,
                enabled: true,
                permissions: &db::Permissions::new(),
                group_id: None,
                revokes: &[],
            })
            .await
            .expect("shared account creation should succeed");

        // Update shared account with only allowed permissions
        let shared_user = test_ctx
            .db
            .users
            .get_user_by_username("shared_acct")
            .await
            .unwrap()
            .unwrap();
        let request = UserUpdateRequest {
            id: shared_user.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: None,
            enabled: None,
            permissions: Some(vec![
                "chat_send".to_string(),
                "chat_receive".to_string(),
                "user_list".to_string(),
                "user_message".to_string(),
            ]),
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(admin_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Should succeed");

        let response = read_server_message(&mut test_ctx).await;
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
        let hashed = hash_password(
            password,
            nexus_common::validators::PasswordStrength::Weak,
            true,
        )
        .expect("hash should work");
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "admin",
                hashed_password: &hashed,
                is_admin: true,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
            })
            .await
            .unwrap();

        // Login as admin
        let admin_id = test_ctx
            .user_manager
            .add_user(crate::users::user::NewSessionParams {
                session_id: 0,
                user_id: 1,
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
                group_id: None,
                group_name: None,
                last_activity: std::time::Instant::now(),
            })
            .await
            .unwrap();

        // Try to rename guest account
        let guest_user = test_ctx
            .db
            .users
            .get_user_by_username("guest")
            .await
            .unwrap()
            .unwrap();
        let request = UserUpdateRequest {
            id: guest_user.id,
            current_password: None,
            username: Some("notguest".to_string()),
            password: None,
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(admin_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Handler should return Ok");

        let response = read_server_message(&mut test_ctx).await;
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
        let hashed = hash_password(
            password,
            nexus_common::validators::PasswordStrength::Weak,
            true,
        )
        .expect("hash should work");
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "admin",
                hashed_password: &hashed,
                is_admin: true,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
            })
            .await
            .unwrap();

        // Login as admin
        let admin_id = test_ctx
            .user_manager
            .add_user(crate::users::user::NewSessionParams {
                session_id: 0,
                user_id: 1,
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
                group_id: None,
                group_name: None,
                last_activity: std::time::Instant::now(),
            })
            .await
            .unwrap();

        // Update guest account permissions
        // Enable the guest account (should be allowed)
        let guest_user = test_ctx
            .db
            .users
            .get_user_by_username("guest")
            .await
            .unwrap()
            .unwrap();
        let request = UserUpdateRequest {
            id: guest_user.id,
            current_password: None,
            username: None, // Not renaming
            password: None,
            is_admin: None,
            enabled: Some(true), // Enable guest
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(admin_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Handler should return Ok");

        let response = read_server_message(&mut test_ctx).await;
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
        let hashed = hash_password(
            password,
            nexus_common::validators::PasswordStrength::Weak,
            true,
        )
        .expect("hash should work");
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "admin",
                hashed_password: &hashed,
                is_admin: true,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
            })
            .await
            .unwrap();

        // Login as admin
        let admin_id = test_ctx
            .user_manager
            .add_user(crate::users::user::NewSessionParams {
                session_id: 0,
                user_id: 1,
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
                group_id: None,
                group_name: None,
                last_activity: std::time::Instant::now(),
            })
            .await
            .unwrap();

        // Update guest account permissions (should succeed with allowed permissions)
        let guest_user = test_ctx
            .db
            .users
            .get_user_by_username("guest")
            .await
            .unwrap()
            .unwrap();
        let request = UserUpdateRequest {
            id: guest_user.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: None,
            enabled: None,
            permissions: Some(vec![
                "chat_send".to_string(),
                "chat_receive".to_string(),
                "user_list".to_string(),
            ]),
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(admin_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Handler should return Ok");

        let response = read_server_message(&mut test_ctx).await;
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
        let hashed = hash_password(
            password,
            nexus_common::validators::PasswordStrength::Weak,
            true,
        )
        .expect("hash should work");
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "admin",
                hashed_password: &hashed,
                is_admin: true,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
            })
            .await
            .unwrap();

        // Login as admin
        let admin_id = test_ctx
            .user_manager
            .add_user(crate::users::user::NewSessionParams {
                session_id: 0,
                user_id: 1,
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
                group_id: None,
                group_name: None,
                last_activity: std::time::Instant::now(),
            })
            .await
            .unwrap();

        // Try to change guest account password
        let guest_user = test_ctx
            .db
            .users
            .get_user_by_username("guest")
            .await
            .unwrap()
            .unwrap();
        let request = UserUpdateRequest {
            id: guest_user.id,
            current_password: None,
            username: None,
            password: Some("newpassword".to_string()),
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(admin_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Handler should return Ok");

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(!success, "Should fail to change guest password");
                assert!(error.is_some(), "Should have error message");
            }
            _ => panic!("Expected UserUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_userupdate_no_permissions_updated_when_unchanged() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create bob with some permissions
        let mut bob_perms = Permissions::new();
        bob_perms.permissions.insert(Permission::UserList);
        bob_perms.permissions.insert(Permission::ChatSend);
        let bob = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &bob_perms,
                group_id: None,
                revokes: &[],
            })
            .await
            .unwrap();

        // Add bob to UserManager so he can receive messages
        let bob_session = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                user_id: bob.id,
                username: "bob".to_string(),
                is_admin: false,
                is_shared: false,
                permissions: bob_perms.permissions.clone(),
                address: test_ctx.peer_addr,
                created_at: 0,
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: DEFAULT_TEST_LOCALE.to_string(),
                avatar: None,
                nickname: "bob".to_string(),
                is_away: false,
                status: None,
                group_id: None,
                group_name: None,
                last_activity: std::time::Instant::now(),
            })
            .await
            .unwrap();

        // Update bob with the SAME permissions (no actual change)
        let request = UserUpdateRequest {
            id: bob.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: None,
            enabled: None,
            permissions: Some(vec!["user_list".to_string(), "chat_send".to_string()]),
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(admin_session),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        // Read the UserUpdateResponse
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected UserUpdateResponse, got {:?}", response),
        }

        // Verify NO PermissionsUpdated was sent (rx should be empty)
        let result = test_ctx.rx.try_recv();
        assert!(
            result.is_err(),
            "Should NOT receive PermissionsUpdated when permissions unchanged, got {:?}",
            result
        );

        // Clean up
        let _ = bob_session;
    }

    #[tokio::test]
    async fn test_userupdate_permissions_updated_sent_when_changed() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create bob with some permissions
        let mut bob_perms = Permissions::new();
        bob_perms.permissions.insert(Permission::UserList);
        let bob = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &bob_perms,
                group_id: None,
                revokes: &[],
            })
            .await
            .unwrap();

        // Add bob to UserManager so he can receive messages
        let bob_session = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                user_id: bob.id,
                username: "bob".to_string(),
                is_admin: false,
                is_shared: false,
                permissions: bob_perms.permissions.clone(),
                address: test_ctx.peer_addr,
                created_at: 0,
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: DEFAULT_TEST_LOCALE.to_string(),
                avatar: None,
                nickname: "bob".to_string(),
                is_away: false,
                status: None,
                group_id: None,
                group_name: None,
                last_activity: std::time::Instant::now(),
            })
            .await
            .unwrap();

        // Update bob with DIFFERENT permissions (add chat_send)
        let request = UserUpdateRequest {
            id: bob.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: None,
            enabled: None,
            permissions: Some(vec!["user_list".to_string(), "chat_send".to_string()]),
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(admin_session),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        // Read the UserUpdateResponse
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected UserUpdateResponse, got {:?}", response),
        }

        // Verify PermissionsUpdated WAS sent (permissions changed)
        let (msg, _) = test_ctx
            .rx
            .recv()
            .await
            .expect("Should receive PermissionsUpdated");
        match msg {
            ServerMessage::PermissionsUpdated {
                is_admin,
                permissions,
                ..
            } => {
                assert!(!is_admin);
                assert!(permissions.contains(&"user_list".to_string()));
                assert!(permissions.contains(&"chat_send".to_string()));
            }
            _ => panic!("Expected PermissionsUpdated, got {:?}", msg),
        }

        // Clean up
        let _ = bob_session;
    }

    #[tokio::test]
    async fn test_userupdate_no_permissions_updated_for_password_only_change() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create bob with some permissions
        let mut bob_perms = Permissions::new();
        bob_perms.permissions.insert(Permission::UserList);
        let bob = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &bob_perms,
                group_id: None,
                revokes: &[],
            })
            .await
            .unwrap();

        // Add bob to UserManager
        let bob_session = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                user_id: bob.id,
                username: "bob".to_string(),
                is_admin: false,
                is_shared: false,
                permissions: bob_perms.permissions.clone(),
                address: test_ctx.peer_addr,
                created_at: 0,
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: DEFAULT_TEST_LOCALE.to_string(),
                avatar: None,
                nickname: "bob".to_string(),
                is_away: false,
                status: None,
                group_id: None,
                group_name: None,
                last_activity: std::time::Instant::now(),
            })
            .await
            .unwrap();

        // Update bob's password only (no permissions change)
        let request = UserUpdateRequest {
            id: bob.id,
            current_password: None,
            username: None,
            password: Some("newpassword".to_string()),
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(admin_session),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        // Read the UserUpdateResponse
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected UserUpdateResponse, got {:?}", response),
        }

        // Verify NO PermissionsUpdated was sent (only password changed)
        let result = test_ctx.rx.try_recv();
        assert!(
            result.is_err(),
            "Should NOT receive PermissionsUpdated for password-only change"
        );

        // Clean up
        let _ = bob_session;
    }

    #[tokio::test]
    async fn test_userupdate_permissions_updated_sent_when_admin_status_changes() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create bob as non-admin
        let bob = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
            })
            .await
            .unwrap();

        // Add bob to UserManager so he can receive messages
        let bob_session = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                user_id: bob.id,
                username: "bob".to_string(),
                is_admin: false,
                is_shared: false,
                permissions: std::collections::HashSet::new(),
                address: test_ctx.peer_addr,
                created_at: 0,
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: DEFAULT_TEST_LOCALE.to_string(),
                avatar: None,
                nickname: "bob".to_string(),
                is_away: false,
                status: None,
                group_id: None,
                group_name: None,
                last_activity: std::time::Instant::now(),
            })
            .await
            .unwrap();

        // Promote bob to admin (no permissions change, but admin status changes)
        let request = UserUpdateRequest {
            id: bob.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: Some(true),
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(admin_session),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        // Read the UserUpdateResponse
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected UserUpdateResponse, got {:?}", response),
        }

        // Verify PermissionsUpdated WAS sent (admin status changed)
        let (msg, _) = test_ctx
            .rx
            .recv()
            .await
            .expect("Should receive PermissionsUpdated when admin status changes");
        match msg {
            ServerMessage::PermissionsUpdated {
                is_admin,
                server_info,
                ..
            } => {
                assert!(is_admin, "Bob should now be admin");
                // Admins get server_info with max_connections_per_ip
                assert!(server_info.is_some(), "Admin should receive server_info");
            }
            _ => panic!("Expected PermissionsUpdated, got {:?}", msg),
        }

        // Clean up
        let _ = bob_session;
    }

    #[tokio::test]
    async fn test_userupdate_permissions_updated_sent_when_enabled_status_changes() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create bob as enabled
        let bob = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
            })
            .await
            .unwrap();

        // Add bob to UserManager so he can receive messages
        let bob_session = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                user_id: bob.id,
                username: "bob".to_string(),
                is_admin: false,
                is_shared: false,
                permissions: std::collections::HashSet::new(),
                address: test_ctx.peer_addr,
                created_at: 0,
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: DEFAULT_TEST_LOCALE.to_string(),
                avatar: None,
                nickname: "bob".to_string(),
                is_away: false,
                status: None,
                group_id: None,
                group_name: None,
                last_activity: std::time::Instant::now(),
            })
            .await
            .unwrap();

        // Disable bob (no permissions change, but enabled status changes)
        let request = UserUpdateRequest {
            id: bob.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: None,
            enabled: Some(false),
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(admin_session),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        // Read the UserUpdateResponse
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected UserUpdateResponse, got {:?}", response),
        }

        // When user is disabled, they get disconnected with an Error message first,
        // then PermissionsUpdated is sent (but they may not receive it since they're being disconnected)
        // The key is that the PermissionsUpdated IS generated for enabled status change
        //
        // Actually, looking at the code flow: PermissionsUpdated is broadcast first,
        // then the disconnect happens. So we should receive PermissionsUpdated.
        let (msg, _) = test_ctx
            .rx
            .recv()
            .await
            .expect("Should receive PermissionsUpdated when enabled status changes");
        match msg {
            ServerMessage::PermissionsUpdated { is_admin, .. } => {
                assert!(!is_admin);
            }
            // Could also be the disconnect Error message
            ServerMessage::Error { .. } => {
                // This is also acceptable - means the disconnect happened
            }
            _ => panic!("Expected PermissionsUpdated or Error, got {:?}", msg),
        }

        // Clean up
        let _ = bob_session;
    }

    #[tokio::test]
    async fn test_userupdate_no_permissions_updated_when_admin_status_unchanged() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create bob as non-admin
        let bob = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
            })
            .await
            .unwrap();

        // Add bob to UserManager so he can receive messages
        let bob_session = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                user_id: bob.id,
                username: "bob".to_string(),
                is_admin: false,
                is_shared: false,
                permissions: std::collections::HashSet::new(),
                address: test_ctx.peer_addr,
                created_at: 0,
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: DEFAULT_TEST_LOCALE.to_string(),
                avatar: None,
                nickname: "bob".to_string(),
                is_away: false,
                status: None,
                group_id: None,
                group_name: None,
                last_activity: std::time::Instant::now(),
            })
            .await
            .unwrap();

        // Set bob's admin status to false (same as current - no change)
        let request = UserUpdateRequest {
            id: bob.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: Some(false), // Same as current
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(admin_session),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        // Read the UserUpdateResponse
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected UserUpdateResponse, got {:?}", response),
        }

        // Verify NO PermissionsUpdated was sent (admin status unchanged)
        let result = test_ctx.rx.try_recv();
        assert!(
            result.is_err(),
            "Should NOT receive PermissionsUpdated when admin status unchanged"
        );

        // Clean up
        let _ = bob_session;
    }

    #[tokio::test]
    async fn test_userupdate_voice_listen_revoked_kicks_from_voice() {
        use std::collections::HashSet;

        let mut test_ctx = create_test_context().await;

        // Create a user with voice_listen permission
        let mut voice_perms = Permissions::new();
        voice_perms.permissions.insert(Permission::VoiceListen);
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "voiceuser",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &voice_perms,
                group_id: None,
                revokes: &[],
            })
            .await
            .unwrap();

        // Login as admin
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Login as voiceuser
        let perms: HashSet<Permission> = [Permission::VoiceListen].iter().copied().collect();
        let voice_user_session = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                user_id: 2,
                username: "voiceuser".to_string(),
                nickname: "voiceuser".to_string(),
                is_admin: false,
                is_shared: false,
                permissions: perms,
                address: test_ctx.peer_addr,
                created_at: 0,
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: DEFAULT_TEST_LOCALE.to_string(),
                avatar: None,
                is_away: false,
                status: None,
                group_id: None,
                group_name: None,
                last_activity: std::time::Instant::now(),
            })
            .await
            .expect("Failed to add voice user session");

        // Have the voice user join voice in a channel
        // First, join a channel
        let _ = test_ctx
            .channel_manager
            .join("#general", voice_user_session)
            .await;

        // Create a voice session for the user
        let voice_session = crate::voice::VoiceSession::new(
            "voiceuser".to_string(),
            vec!["#general".to_string()],
            voice_user_session,
            test_ctx.peer_addr.ip(),
        );
        test_ctx.voice_registry.add(voice_session).await;

        // Verify user is in voice
        assert!(
            test_ctx
                .voice_registry
                .has_session(voice_user_session)
                .await
        );

        // Now revoke voice_listen permission via UserUpdate
        let voiceuser_db = test_ctx
            .db
            .users
            .get_user_by_username("voiceuser")
            .await
            .unwrap()
            .unwrap();
        let request = UserUpdateRequest {
            id: voiceuser_db.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: None,
            enabled: None,
            permissions: Some(vec![]), // Remove all permissions including voice_listen
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(admin_session),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        // Read the UserUpdateResponse
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected UserUpdateResponse, got {:?}", response),
        }

        // Verify user was kicked from voice
        assert!(
            !test_ctx
                .voice_registry
                .has_session(voice_user_session)
                .await,
            "User should be kicked from voice when voice_listen is revoked"
        );
    }

    #[tokio::test]
    async fn test_userupdate_voice_talk_revoked_stays_in_voice() {
        use std::collections::HashSet;

        let mut test_ctx = create_test_context().await;

        // Create a user with voice_listen and voice_talk permissions
        let mut voice_perms = Permissions::new();
        voice_perms.permissions.insert(Permission::VoiceListen);
        voice_perms.permissions.insert(Permission::VoiceTalk);
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "voiceuser",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &voice_perms,
                group_id: None,
                revokes: &[],
            })
            .await
            .unwrap();

        // Login as admin
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Login as voiceuser
        let perms: HashSet<Permission> = [Permission::VoiceListen, Permission::VoiceTalk]
            .iter()
            .copied()
            .collect();
        let voice_user_session = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                user_id: 2,
                username: "voiceuser".to_string(),
                nickname: "voiceuser".to_string(),
                is_admin: false,
                is_shared: false,
                permissions: perms,
                address: test_ctx.peer_addr,
                created_at: 0,
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: DEFAULT_TEST_LOCALE.to_string(),
                avatar: None,
                is_away: false,
                status: None,
                group_id: None,
                group_name: None,
                last_activity: std::time::Instant::now(),
            })
            .await
            .expect("Failed to add voice user session");

        // Have the voice user join voice in a channel
        let _ = test_ctx
            .channel_manager
            .join("#general", voice_user_session)
            .await;

        // Create a voice session for the user
        let voice_session = crate::voice::VoiceSession::new(
            "voiceuser".to_string(),
            vec!["#general".to_string()],
            voice_user_session,
            test_ctx.peer_addr.ip(),
        );
        test_ctx.voice_registry.add(voice_session).await;

        // Verify user is in voice
        assert!(
            test_ctx
                .voice_registry
                .has_session(voice_user_session)
                .await
        );

        // Now revoke only voice_talk permission (keep voice_listen)
        let voiceuser_db = test_ctx
            .db
            .users
            .get_user_by_username("voiceuser")
            .await
            .unwrap()
            .unwrap();
        let request = UserUpdateRequest {
            id: voiceuser_db.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: None,
            enabled: None,
            permissions: Some(vec!["voice_listen".to_string()]), // Keep voice_listen, remove voice_talk
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(admin_session),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        // Read the UserUpdateResponse
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected UserUpdateResponse, got {:?}", response),
        }

        // Verify user is STILL in voice (only voice_talk was revoked)
        assert!(
            test_ctx
                .voice_registry
                .has_session(voice_user_session)
                .await,
            "User should stay in voice when only voice_talk is revoked (can still listen)"
        );
    }

    #[tokio::test]
    async fn test_user_update_assign_group() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a group
        let group = test_ctx
            .db
            .groups
            .create_group(
                "Mods",
                false,
                &db::Permissions::from(&[db::Permission::ChatSend, db::Permission::UserKick]),
            )
            .await
            .unwrap();

        // Create a user without a group
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
            })
            .await
            .unwrap();

        // Verify user has no group initially
        let bob = test_ctx
            .db
            .users
            .get_user_by_username("bob")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(bob.group_id, None);

        // Update user to assign group
        let request = UserUpdateRequest {
            id: bob.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: Some(group.id),
            remove_group: None,
            revokes: None,
            session_id: Some(admin_session),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse {
                success,
                error,
                id,
                username,
            } => {
                assert!(success, "Should successfully assign group");
                assert!(error.is_none(), "Should have no error");
                assert!(id.is_some());
                assert_eq!(username, Some("bob".to_string()));
            }
            _ => panic!("Expected UserUpdateResponse"),
        }

        // Verify user is now in the group
        let bob = test_ctx
            .db
            .users
            .get_user_by_username("bob")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(bob.group_id, Some(group.id));
    }

    #[tokio::test]
    async fn test_user_update_remove_group() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a group
        let group = test_ctx
            .db
            .groups
            .create_group(
                "Mods",
                false,
                &db::Permissions::from(&[db::Permission::ChatSend, db::Permission::UserKick]),
            )
            .await
            .unwrap();

        // Create a user assigned to the group
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: Some(group.id),
                revokes: &[],
            })
            .await
            .unwrap();

        // Verify user is in the group initially
        let bob = test_ctx
            .db
            .users
            .get_user_by_username("bob")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(bob.group_id, Some(group.id));

        // Update user to remove group
        let request = UserUpdateRequest {
            id: bob.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: Some(true),
            revokes: None,
            session_id: Some(admin_session),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse {
                success,
                error,
                id,
                username,
            } => {
                assert!(success, "Should successfully remove group");
                assert!(error.is_none(), "Should have no error");
                assert!(id.is_some());
                assert_eq!(username, Some("bob".to_string()));
            }
            _ => panic!("Expected UserUpdateResponse"),
        }

        // Verify user no longer has a group
        let bob = test_ctx
            .db
            .users
            .get_user_by_username("bob")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(bob.group_id, None);
    }

    #[tokio::test]
    async fn test_userupdate_non_admin_cannot_remove_group_with_unowned_perms() {
        let mut test_ctx = create_test_context().await;

        // Login as non-admin editor with UserEdit + ChatSend (but NOT UserKick)
        let editor_session = login_user(
            &mut test_ctx,
            "editor",
            "password",
            &[db::Permission::UserEdit, db::Permission::ChatSend],
            false,
        )
        .await;

        // Create a group with a permission the editor doesn't have
        let group = test_ctx
            .db
            .groups
            .create_group(
                "Mods",
                false,
                &db::Permissions::from(&[db::Permission::ChatSend, db::Permission::UserKick]),
            )
            .await
            .unwrap();

        // Create a user assigned to that group
        let bob = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: Some(group.id),
                revokes: &[],
            })
            .await
            .unwrap();

        // Non-admin editor tries to remove bob from the group
        let request = UserUpdateRequest {
            id: bob.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: Some(true),
            revokes: None,
            session_id: Some(editor_session),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Should send error, not disconnect");

        // Should be rejected — editor doesn't have UserKick
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse {
                success,
                error,
                id,
                username,
            } => {
                assert!(!success);
                assert!(error.is_some());
                let err_msg = error.unwrap();
                assert!(
                    err_msg.contains("ermission"),
                    "Should be a permission error, got: {err_msg}"
                );
                assert!(id.is_none());
                assert!(username.is_none());
            }
            other => panic!("Expected UserUpdateResponse (permission denied), got: {other:?}"),
        }

        // Verify bob is still in the group
        let bob = test_ctx
            .db
            .users
            .get_user_by_username("bob")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            bob.group_id,
            Some(group.id),
            "Group should not have been removed"
        );
    }

    #[tokio::test]
    async fn test_userupdate_non_admin_cannot_assign_group_with_unowned_perms() {
        let mut test_ctx = create_test_context().await;

        // Login as non-admin editor with UserEdit + ChatSend (but NOT UserKick)
        let editor_session = login_user(
            &mut test_ctx,
            "editor",
            "password",
            &[db::Permission::UserEdit, db::Permission::ChatSend],
            false,
        )
        .await;

        // Create a group with a permission the editor doesn't have
        let group = test_ctx
            .db
            .groups
            .create_group(
                "Mods",
                false,
                &db::Permissions::from(&[db::Permission::ChatSend, db::Permission::UserKick]),
            )
            .await
            .unwrap();

        // Create a user without a group
        let bob = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
            })
            .await
            .unwrap();

        // Non-admin editor tries to assign bob to the group
        let request = UserUpdateRequest {
            id: bob.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: Some(group.id),
            remove_group: None,
            revokes: None,
            session_id: Some(editor_session),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Should send error, not disconnect");

        // Should be rejected — editor doesn't have UserKick
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse {
                success,
                error,
                id,
                username,
            } => {
                assert!(!success);
                assert!(error.is_some());
                let err_msg = error.unwrap();
                assert!(
                    err_msg.contains("ermission"),
                    "Should be a permission error, got: {err_msg}"
                );
                assert!(id.is_none());
                assert!(username.is_none());
            }
            other => panic!("Expected UserUpdateResponse (permission denied), got: {other:?}"),
        }

        // Verify bob still has no group
        let bob = test_ctx
            .db
            .users
            .get_user_by_username("bob")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(bob.group_id, None, "Group should not have been assigned");
    }

    #[tokio::test]
    async fn test_userupdate_non_admin_can_edit_user_in_high_privilege_group() {
        let mut test_ctx = create_test_context().await;

        // Login as non-admin editor with UserEdit + ChatSend (but NOT UserKick)
        let editor_session = login_user(
            &mut test_ctx,
            "editor",
            "password",
            &[db::Permission::UserEdit, db::Permission::ChatSend],
            false,
        )
        .await;

        // Create a group with a permission the editor doesn't have
        let group = test_ctx
            .db
            .groups
            .create_group(
                "Mods",
                false,
                &db::Permissions::from(&[db::Permission::ChatSend, db::Permission::UserKick]),
            )
            .await
            .unwrap();

        // Create a user assigned to the high-privilege group
        let bob = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: Some(group.id),
                revokes: &[],
            })
            .await
            .unwrap();

        // Non-admin editor renames bob — NOT changing group
        let request = UserUpdateRequest {
            id: bob.id,
            current_password: None,
            username: Some("robert".to_string()),
            password: None,
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            session_id: Some(editor_session),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse {
                success,
                error,
                id,
                username,
            } => {
                assert!(success, "Should succeed — group not being changed");
                assert!(error.is_none());
                assert!(id.is_some());
                assert_eq!(username, Some("robert".to_string()));
            }
            other => panic!("Expected UserUpdateResponse, got: {other:?}"),
        }

        // Verify bob was renamed and still in the group
        let bob = test_ctx
            .db
            .users
            .get_user_by_username("robert")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(bob.group_id, Some(group.id), "Group should be unchanged");
    }

    #[tokio::test]
    async fn test_userupdate_non_admin_cannot_revoke_unowned_permission() {
        let mut test_ctx = create_test_context().await;

        // Login as non-admin editor with UserEdit + ChatSend (but NOT UserKick)
        let editor_session = login_user(
            &mut test_ctx,
            "editor",
            "password",
            &[db::Permission::UserEdit, db::Permission::ChatSend],
            false,
        )
        .await;

        // Create a group with ChatSend + UserKick
        let group = test_ctx
            .db
            .groups
            .create_group(
                "Mods",
                false,
                &db::Permissions::from(&[db::Permission::ChatSend, db::Permission::UserKick]),
            )
            .await
            .unwrap();

        // Create a user assigned to the group
        let bob = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: Some(group.id),
                revokes: &[],
            })
            .await
            .unwrap();

        // Non-admin editor tries to revoke UserKick (which editor doesn't have)
        let request = UserUpdateRequest {
            id: bob.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: Some(vec!["user_kick".to_string()]),
            session_id: Some(editor_session),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Should send error, not disconnect");

        // Should be rejected — editor doesn't have UserKick
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse {
                success,
                error,
                id,
                username,
            } => {
                assert!(!success);
                assert!(error.is_some());
                let err_msg = error.unwrap();
                assert!(
                    err_msg.contains("ermission"),
                    "Should be a permission error, got: {err_msg}"
                );
                assert!(id.is_none());
                assert!(username.is_none());
            }
            other => panic!("Expected UserUpdateResponse (permission denied), got: {other:?}"),
        }

        // Verify bob has no revokes
        let revokes = test_ctx
            .db
            .users
            .get_revoke_permissions(
                test_ctx
                    .db
                    .users
                    .get_user_by_username("bob")
                    .await
                    .unwrap()
                    .unwrap()
                    .id,
            )
            .await
            .unwrap();
        assert!(revokes.is_empty(), "No revokes should have been set");
    }

    #[tokio::test]
    async fn test_userupdate_non_admin_cannot_move_from_high_privilege_group() {
        let mut test_ctx = create_test_context().await;

        // Login as non-admin editor with UserEdit + ChatSend (but NOT BanCreate)
        let editor_session = login_user(
            &mut test_ctx,
            "editor",
            "password",
            &[db::Permission::UserEdit, db::Permission::ChatSend],
            false,
        )
        .await;

        // Create a high-privilege group with BanCreate (editor doesn't have this)
        let high_group = test_ctx
            .db
            .groups
            .create_group(
                "Admins",
                false,
                &db::Permissions::from(&[db::Permission::BanCreate]),
            )
            .await
            .unwrap();

        // Create a low-privilege group with ChatSend (editor has this)
        let low_group = test_ctx
            .db
            .groups
            .create_group(
                "Basic",
                false,
                &db::Permissions::from(&[db::Permission::ChatSend]),
            )
            .await
            .unwrap();

        // Create bob in the high-privilege group
        let bob = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: Some(high_group.id),
                revokes: &[],
            })
            .await
            .unwrap();

        // Non-admin editor tries to move bob from high-privilege to low-privilege group
        // This should be rejected — editor doesn't have BanCreate from the old group
        let request = UserUpdateRequest {
            id: bob.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: Some(low_group.id),
            remove_group: None,
            revokes: None,
            session_id: Some(editor_session),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse {
                success, error, id, ..
            } => {
                assert!(!success, "Should reject — editor can't control old group");
                assert!(error.is_some());
                assert!(id.is_none());
            }
            other => panic!("Expected UserUpdateResponse, got: {other:?}"),
        }

        // Verify bob is still in the high-privilege group
        let bob = test_ctx
            .db
            .users
            .get_user_by_username("bob")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(bob.group_id, Some(high_group.id));
    }

    #[tokio::test]
    async fn test_userupdate_non_admin_revoke_merge_preserves_unowned_revokes() {
        let mut test_ctx = create_test_context().await;

        // Login as non-admin editor with UserEdit + ChatSend (but NOT BanCreate)
        let editor_session = login_user(
            &mut test_ctx,
            "editor",
            "password",
            &[db::Permission::UserEdit, db::Permission::ChatSend],
            false,
        )
        .await;

        // Create a group with ChatSend + BanCreate
        let group = test_ctx
            .db
            .groups
            .create_group(
                "Mods",
                false,
                &db::Permissions::from(&[db::Permission::ChatSend, db::Permission::BanCreate]),
            )
            .await
            .unwrap();

        // Create bob assigned to the group, with an existing revoke for BanCreate
        // (set by admin — editor doesn't have BanCreate)
        let bob = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: Some(group.id),
                revokes: &[db::Permission::BanCreate],
            })
            .await
            .unwrap();

        // Verify initial state: bob has BanCreate revoke
        let revokes = test_ctx
            .db
            .users
            .get_revoke_permissions(bob.id)
            .await
            .unwrap();
        assert_eq!(revokes.len(), 1);
        assert!(revokes.contains(&db::Permission::BanCreate));

        // Non-admin editor adds a ChatSend revoke (which they control)
        // The existing BanCreate revoke (which they DON'T control) should be preserved
        let request = UserUpdateRequest {
            id: bob.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: Some(vec!["chat_send".to_string()]),
            session_id: Some(editor_session),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(success, "Should succeed: {error:?}");
            }
            other => panic!("Expected UserUpdateResponse, got: {other:?}"),
        }

        // Verify: both revokes exist — ChatSend (requested) + BanCreate (preserved)
        let revokes = test_ctx
            .db
            .users
            .get_revoke_permissions(bob.id)
            .await
            .unwrap();
        assert_eq!(
            revokes.len(),
            2,
            "Should have 2 revokes (requested + preserved), got: {revokes:?}"
        );
        assert!(
            revokes.contains(&db::Permission::ChatSend),
            "ChatSend revoke should be set (editor requested it)"
        );
        assert!(
            revokes.contains(&db::Permission::BanCreate),
            "BanCreate revoke should be preserved (editor can't control it)"
        );
    }
}
