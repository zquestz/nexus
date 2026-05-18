//! UserUpdate message handler

use std::io;
use std::sync::atomic::Ordering;

use tokio::io::AsyncWrite;
use tracing::{error, info, warn};

use nexus_common::is_shared_account_permission;
use nexus_common::protocol::{ServerMessage, UserInfo};
use nexus_common::validators::{
    self, BandwidthWeightError, MIN_BANDWIDTH_WEIGHT, PasswordError, PermissionsError,
    UsernameError, validate_bandwidth_weight,
};

use crate::constants::{
    DEFAULT_LOCALE, HANDLER_USER_UPDATE, LOG_USER_UPDATE_ADMIN, LOG_USER_UPDATE_DB_ERROR,
    LOG_USER_UPDATE_DB_ERROR_GROUP, LOG_USER_UPDATE_DB_ERROR_GROUP_PERMS,
    LOG_USER_UPDATE_DB_ERROR_LOOKUP, LOG_USER_UPDATE_DB_ERROR_TARGET,
    LOG_USER_UPDATE_DB_ERROR_USER, LOG_USER_UPDATE_HASH_ERROR, LOG_USER_UPDATE_NOT_LOGGED_IN,
    LOG_USER_UPDATE_PASSWORD_VERIFY, LOG_USER_UPDATE_PERMISSION_DENIED, LOG_USER_UPDATE_SUCCESS,
    LOG_USER_UPDATE_UNOWNED_PERMISSION, LOG_USER_UPDATE_UNOWNED_REVOKE,
};

#[cfg(test)]
use super::testing::DEFAULT_TEST_LOCALE;
use super::{
    HandlerContext, err_account_disabled_by_admin, err_admin_cannot_have_group, err_authentication,
    err_bandwidth_weight_delegation, err_bandwidth_weight_inherit_would_elevate,
    err_bandwidth_weight_zero, err_cannot_change_guest_password, err_cannot_demote_last_admin,
    err_cannot_disable_last_admin, err_cannot_edit_admin, err_cannot_edit_self,
    err_cannot_rename_guest, err_current_password_incorrect, err_current_password_required,
    err_database, err_group_not_found, err_group_shared_mismatch, err_not_logged_in,
    err_password_empty, err_password_too_long, err_password_too_weak, err_permission_denied,
    err_permission_grant_revoke_conflict, err_permissions_contains_newlines,
    err_permissions_empty_permission, err_permissions_invalid_characters,
    err_permissions_permission_too_long, err_permissions_too_many, err_shared_cannot_be_admin,
    err_shared_cannot_self_edit, err_shared_invalid_permissions, err_unknown_permission,
    err_update_failed, err_user_not_found, err_username_empty, err_username_exists,
    err_username_invalid, err_username_too_long, remove_user_with_voice_cleanup,
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
    pub bandwidth_weight: Option<u16>,
    pub inherit_bandwidth_weight: Option<bool>,
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
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_USER_UPDATE))
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
                .send_error_and_disconnect(
                    &err_authentication(ctx.locale),
                    Some(HANDLER_USER_UPDATE),
                )
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
                .send_error_and_disconnect(&err_database(ctx.locale), Some(HANDLER_USER_UPDATE))
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

    // Self-edit gate: a non-admin can edit only their own account, and the
    // set of fields they may change is more restrictive than for an admin
    // editing someone else. Drives the password / shared-account / forbidden-
    // field branches below.
    let is_self_edit = target_username.to_lowercase() == requesting_user.username.to_lowercase();

    if is_self_edit {
        // Shared accounts cannot self-edit (no password to change, no other
        // fields admissible).
        if requesting_user.is_shared {
            let response = ServerMessage::UserUpdateResponse {
                success: false,
                error: Some(err_shared_cannot_self_edit(ctx.locale)),
                id: None,
                username: None,
            };
            return ctx.send_message(&response).await;
        }

        // Defense in depth: these fields are never accepted on self-edit,
        // even from an admin. Client UI hard-disables them on self-rows.
        if request.is_admin.is_some()
            || request.enabled.is_some()
            || request.permissions.is_some()
            || request.revokes.is_some()
            || request.remove_group == Some(true)
        {
            let response = ServerMessage::UserUpdateResponse {
                success: false,
                error: Some(err_cannot_edit_self(ctx.locale)),
                id: None,
                username: None,
            };
            return ctx.send_message(&response).await;
        }

        // Admin self-edit: group_id is rejected by the admin XOR group
        // invariant (admins cannot be members of a group). Non-admin self
        // edits also can't set group_id, but that's caught by the
        // non-admin-restriction block below with a different error.
        if requesting_user.is_admin && request.group_id.is_some() {
            let response = ServerMessage::UserUpdateResponse {
                success: false,
                error: Some(err_admin_cannot_have_group(ctx.locale)),
                id: None,
                username: None,
            };
            return ctx.send_message(&response).await;
        }

        // Non-admin self-edits are restricted to password change. Admin
        // self-edits additionally permit username and the bandwidth-weight
        // fields. (group_id rejected above for admins; admins can't have
        // groups.)
        if !requesting_user.is_admin
            && (request.username.is_some()
                || request.group_id.is_some()
                || request.bandwidth_weight.is_some()
                || request.inherit_bandwidth_weight.is_some())
        {
            let response = ServerMessage::UserUpdateResponse {
                success: false,
                error: Some(err_cannot_edit_self(ctx.locale)),
                id: None,
                username: None,
            };
            return ctx.send_message(&response).await;
        }

        // Password change (admin or non-admin) requires current_password
        // verification. Skipped entirely when no password change requested.
        if let Some(ref new_password) = request.password
            && !new_password.trim().is_empty()
        {
            let Some(ref current_password) = request.current_password else {
                let response = ServerMessage::UserUpdateResponse {
                    success: false,
                    error: Some(err_current_password_required(ctx.locale)),
                    id: None,
                    username: None,
                };
                return ctx.send_message(&response).await;
            };

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
                        .send_error_and_disconnect(
                            &err_database(ctx.locale),
                            Some(HANDLER_USER_UPDATE),
                        )
                        .await;
                }
            };

            match verify_password_async(current_password.to_string(), password_hash.clone()).await {
                Ok(true) => {}
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
                        .send_error_and_disconnect(
                            &err_database(ctx.locale),
                            Some(HANDLER_USER_UPDATE),
                        )
                        .await;
                }
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
                        .send_error_and_disconnect(
                            &err_database(ctx.locale),
                            Some(HANDLER_USER_UPDATE),
                        )
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
                .send_error_and_disconnect(&err_database(ctx.locale), Some(HANDLER_USER_UPDATE))
                .await;
        }
    };

    // Admin XOR group invariant. Reject requests that would leave the target
    // as admin AND set a group_id. Promotion from non-admin to admin is fine
    // (DB layer auto-clears group_id); the rejection only fires when
    // group_id assignment coincides with the target ending up admin.
    let target_final_is_admin = request
        .is_admin
        .unwrap_or_else(|| target_user_account.as_ref().is_some_and(|a| a.is_admin));
    if target_final_is_admin && request.group_id.is_some() {
        let response = ServerMessage::UserUpdateResponse {
            success: false,
            error: Some(err_admin_cannot_have_group(ctx.locale)),
            id: None,
            username: None,
        };
        return ctx.send_message(&response).await;
    }

    // Admin XOR shared invariant. `is_shared` is set at create-time and
    // never modified by UserUpdate, so the only path into the bad
    // combination is promoting an existing shared account to admin.
    //
    // Note the asymmetry with admin XOR group: that invariant auto-cleans
    // in `db/users.rs::update_user` (nulls group_id + wipes permission
    // rows on promotion) because clearing a group is benign — admins
    // resolve to all permissions regardless. Admin XOR shared instead
    // rejects here because demoting `is_shared` orphans the per-session
    // nicknames a shared account carries; we make the admin explicitly
    // delete and recreate rather than silently destroy that identity.
    if request.is_admin == Some(true) && target_user_account.as_ref().is_some_and(|a| a.is_shared) {
        let response = ServerMessage::UserUpdateResponse {
            success: false,
            error: Some(err_shared_cannot_be_admin(ctx.locale)),
            id: None,
            username: None,
        };
        return ctx.send_message(&response).await;
    }

    // Bandwidth weight delegation: non-admins can change a user's effective
    // bandwidth only to a value at or below their own resolved weight.
    // - `bandwidth_weight: Some(N)` → reject when `N > requester`.
    // - `inherit_bandwidth_weight: Some(true)` → reject when the target's
    //   inherited weight (admin-default → group → 1) > requester. Clearing
    //   the override would let the user fall back to a higher tier.
    // Admins bypass both checks.
    //
    // When `inherit_bandwidth_weight: Some(true)`, the `bandwidth_weight`
    // value is discarded by the DB layer (inherit wins). Skip the value
    // check in that case so a defensive client sending both fields isn't
    // rejected on a moot value.
    if !requesting_user.is_admin {
        let requester_weight = requesting_user.bandwidth_weight.load(Ordering::Relaxed);
        // Track the rejection reason as an already-translated error string —
        // the two paths use distinct i18n keys (see `errors.rs` for the
        // contract). The set path fires on `bandwidth_weight: Some(w > req)`;
        // the inherit path fires when clearing the override would land the
        // target on an inherited tier above the requester.
        let mut delegation_error: Option<String> = None;
        if request.inherit_bandwidth_weight != Some(true)
            && let Some(w) = request.bandwidth_weight
            && w > requester_weight
        {
            delegation_error = Some(err_bandwidth_weight_delegation(ctx.locale));
        }
        if delegation_error.is_none() && request.inherit_bandwidth_weight == Some(true) {
            // The inherited weight to compare against must reflect the
            // POST-update group: if the same request also changes group_id
            // or sets remove_group, the OLD group's weight is irrelevant.
            // Otherwise (group unchanged), pass the target's current
            // group_id so the resolver joins on the same row it would
            // post-update.
            let proposed_group_id: Option<i64> = if request.remove_group == Some(true) {
                None
            } else if let Some(new_gid) = request.group_id {
                Some(new_gid)
            } else {
                target_user_account.as_ref().and_then(|a| a.group_id)
            };
            let inherited = match ctx
                .db
                .users
                .get_inherited_bandwidth_weight(request.id, proposed_group_id)
                .await
            {
                Ok(w) => w,
                Err(e) => {
                    error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_USER_UPDATE_DB_ERROR_TARGET);
                    return ctx
                        .send_error_and_disconnect(
                            &err_database(ctx.locale),
                            Some(HANDLER_USER_UPDATE),
                        )
                        .await;
                }
            };
            if inherited > requester_weight {
                delegation_error = Some(err_bandwidth_weight_inherit_would_elevate(ctx.locale));
            }
        }
        if let Some(error) = delegation_error {
            let response = ServerMessage::UserUpdateResponse {
                success: false,
                error: Some(error),
                id: None,
                username: None,
            };
            return ctx.send_message(&response).await;
        }
    }
    // Skip zero-validation too when inherit takes precedence (value discarded).
    if request.inherit_bandwidth_weight != Some(true)
        && let Some(w) = request.bandwidth_weight
        && let Err(BandwidthWeightError::Zero) = validate_bandwidth_weight(w)
    {
        let response = ServerMessage::UserUpdateResponse {
            success: false,
            error: Some(err_bandwidth_weight_zero(ctx.locale, MIN_BANDWIDTH_WEIGHT)),
            id: None,
            username: None,
        };
        return ctx.send_message(&response).await;
    }

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

        // No pre-tx merge for non-admins: the surgical
        // `OwnedSubset` write path inside `update_user` only touches
        // rows for permissions in the requester's owned set, so
        // unowned rows (including any an admin just granted) are
        // preserved automatically. The old snapshot-merge-then-replace
        // approach raced against concurrent admin writes; this passes
        // the requester's literal request straight through.

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
                                    Some(HANDLER_USER_UPDATE),
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

                    // Non-admin delegation: cannot promote a user to a group
                    // whose bandwidth weight exceeds the requester's own
                    // resolved weight. Closes the escalation where a moderator
                    // could move themselves (or others) into a higher-weight
                    // group whose permissions they happen to fully possess.
                    if !requesting_user.is_admin
                        && group.bandwidth_weight
                            > requesting_user.bandwidth_weight.load(Ordering::Relaxed)
                    {
                        let response = ServerMessage::UserUpdateResponse {
                            success: false,
                            error: Some(err_bandwidth_weight_delegation(ctx.locale)),
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

            // No pre-tx merge for non-admins — same reasoning as the
            // grant-merge removal above. The `OwnedSubset` write path
            // inside `update_user` only touches revoke rows for
            // permissions in the requester's owned set, so unowned
            // revoke rows survive untouched. (The old merge also
            // silently swallowed DB read errors via `if let Ok(...)`,
            // which dropped all unowned-revoke preservation on any
            // transient read failure; that path is gone.)

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
                        .send_error_and_disconnect(
                            &err_database(ctx.locale),
                            Some(HANDLER_USER_UPDATE),
                        )
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

    // The `(user_id, permission)` PK allows only one row per permission;
    // a request that names the same permission as both grant and revoke
    // would otherwise be resolved by write order. Fail upfront instead.
    if let (Some(grants), Some(revokes)) = (parsed_permissions.as_ref(), parsed_revokes.as_ref()) {
        for revoke in revokes {
            if grants.permissions.contains(revoke) {
                let response = ServerMessage::UserUpdateResponse {
                    success: false,
                    error: Some(err_permission_grant_revoke_conflict(
                        ctx.locale,
                        revoke.as_str(),
                    )),
                    id: None,
                    username: None,
                };
                return ctx.send_message(&response).await;
            }
        }
    }

    let owned_for_scope: Vec<Permission> = if requesting_user.is_admin {
        Vec::new()
    } else {
        requesting_user.permissions.iter().copied().collect()
    };
    let permission_write_scope = if requesting_user.is_admin {
        crate::db::PermissionWriteScope::ReplaceAll
    } else {
        crate::db::PermissionWriteScope::OwnedSubset(&owned_for_scope)
    };
    let requester_bandwidth_max = if requesting_user.is_admin {
        None
    } else {
        Some(requesting_user.bandwidth_weight.load(Ordering::Relaxed))
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
            bandwidth_weight: request.bandwidth_weight,
            inherit_bandwidth_weight: request.inherit_bandwidth_weight == Some(true),
            requester_is_admin: requesting_user.is_admin,
            permission_write_scope,
            requester_bandwidth_max,
        })
        .await
    {
        Ok(crate::db::UpdateUserResult::Updated {
            account: updated_account,
            resolved_bandwidth_weight,
            permissions: final_permissions,
        }) => {
            info!(
                user = %requesting_user.username,
                ip = %ctx.peer_addr,
                target = %updated_account.username,
                is_admin = updated_account.is_admin,
                "{}", LOG_USER_UPDATE_SUCCESS
            );
            // Response is sent at the end of this arm — a slow admin
            // socket must not stall the security-relevant cascades.
            let response = ServerMessage::UserUpdateResponse {
                success: true,
                error: None,
                id: Some(request.id),
                username: Some(updated_account.username.clone()),
            };

            let group_changed = validated_remove_group || validated_group_id.is_some();
            let admin_status_changed = old_is_admin != updated_account.is_admin;
            let permissions_changed = old_permissions.permissions != final_permissions.permissions;

            // Atomic flip: `UserSession::has_permission` short-circuits
            // on `is_admin`, so a split write would widen the
            // demoted-admin window across every await between the two.
            if admin_status_changed || permissions_changed {
                ctx.user_manager
                    .update_auth_state(
                        updated_account.id,
                        updated_account.is_admin,
                        final_permissions.permissions.clone(),
                    )
                    .await;
            }

            let (updated_group_id, updated_group_name) = if let Some(gid) = updated_account.group_id
            {
                match ctx.db.groups.get_group_by_id(gid).await {
                    Ok(Some(g)) => (Some(gid), Some(g.name)),
                    _ => (None, None),
                }
            } else {
                (None, None)
            };

            // Rename the cache identity before any username-keyed
            // cascade below — otherwise PermissionsUpdated / voice
            // cleanup / disable disconnect would miss every session.
            let username_changed =
                old_username.to_lowercase() != updated_account.username.to_lowercase();

            if username_changed {
                ctx.user_manager
                    .update_username(updated_account.id, updated_account.username.clone())
                    .await;

                // Shared accounts keep per-session nicknames (chosen at
                // login) — same invariant `UserManager::update_username`
                // honors for `user.nickname`.
                if !updated_account.is_shared {
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
            }

            {
                let enabled_changed = old_enabled != updated_account.enabled;
                let actually_changed =
                    admin_status_changed || enabled_changed || permissions_changed || group_changed;

                if actually_changed {
                    let permission_strings: Vec<String> = final_permissions
                        .permissions
                        .iter()
                        .map(|p| p.as_str().to_string())
                        .collect();

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
                        max_outbound_rate: config.max_outbound_rate,
                        scheduler_chunk_size: config.scheduler_chunk_size,
                    };

                    let info_options = ServerInfoOptions {
                        is_admin: updated_account.is_admin,
                        has_file_reindex,
                        has_chat_join,
                        include_image: false,
                    };

                    let server_info = Some(build_server_info(&info_values, &info_options));

                    let permissions_update = ServerMessage::PermissionsUpdated {
                        is_admin: updated_account.is_admin,
                        permissions: permission_strings,
                        server_info,
                        group_id: updated_group_id,
                        group_name: updated_group_name.clone(),
                    };

                    // Send to all sessions belonging to the updated user
                    ctx.user_manager
                        .broadcast_to_username(&updated_account.username, &permissions_update)
                        .await;

                    // Admin-aware: admins hold `VoiceListen` implicitly
                    // via the `has_permission` bypass, so a demoted admin
                    // without an explicit grant still loses it.
                    let had_voice_listen = old_is_admin
                        || old_permissions
                            .permissions
                            .contains(&Permission::VoiceListen);
                    let has_voice_listen = updated_account.is_admin
                        || final_permissions
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

            // Send a disabled-by-admin Error then drop the tx. The
            // connection loop's `rx.recv()` returns None once all
            // senders are dropped and the TCP connection closes.
            // `connection.rs` cleanup won't re-broadcast
            // UserDisconnected because the user is already gone from
            // the manager.
            if let Some(false) = request.enabled {
                let session_ids = ctx
                    .user_manager
                    .get_session_ids_for_user(&updated_account.username)
                    .await;

                for session_id in session_ids {
                    if let Some(user) = ctx.user_manager.get_user_by_session_id(session_id).await {
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

            // Promotion auto-clears group_id in the DB, so admin status
            // change must refresh the cached group too even when the
            // request didn't touch group_id directly.
            if group_changed || admin_status_changed {
                ctx.user_manager
                    .update_group(
                        updated_account.id,
                        updated_group_id,
                        updated_group_name.clone(),
                    )
                    .await;
            }

            // Inherit wins over an explicit weight (matches the DB
            // layer's precedence) — check it first so a request that
            // also carries the pre-update value still registers as a
            // change.
            let bandwidth_weight_request_change = if request.inherit_bandwidth_weight == Some(true)
            {
                target_account.bandwidth_weight.is_some()
            } else if let Some(new) = request.bandwidth_weight {
                target_account.bandwidth_weight != Some(new)
            } else {
                false
            };

            let broadcast_should_fire = username_changed
                || admin_status_changed
                || group_changed
                || bandwidth_weight_request_change;
            let cache_should_refresh =
                bandwidth_weight_request_change || group_changed || admin_status_changed;
            let bw_only_trigger = bandwidth_weight_request_change
                && !username_changed
                && !admin_status_changed
                && !group_changed;

            let sessions = if broadcast_should_fire {
                ctx.user_manager
                    .get_sessions_by_username(&updated_account.username)
                    .await
            } else {
                Vec::new()
            };

            // All sessions of one user agree on the cached weight
            // (`update_bandwidth_weight` fan-out invariant); first
            // session is authoritative. `None` means offline.
            let old_resolved: Option<u16> = sessions
                .first()
                .map(|s| s.bandwidth_weight.load(Ordering::Relaxed));

            if cache_should_refresh {
                ctx.user_manager
                    .update_bandwidth_weight(updated_account.id, resolved_bandwidth_weight)
                    .await;
            }

            // Suppress a bandwidth-only broadcast when the effective
            // weight didn't move for an online user. Offline users
            // (`None`) always broadcast — `None != Some(_)` — so
            // observers converge once the user logs in.
            let suppress_for_bw_only =
                bw_only_trigger && old_resolved == Some(resolved_bandwidth_weight);

            if broadcast_should_fire && !suppress_for_bw_only {
                let session_ids: Vec<u32> = sessions.iter().map(|s| s.session_id).collect();

                let (login_time, locale, avatar, is_away, status) = if !sessions.is_empty() {
                    let login_time = sessions.iter().map(|u| u.login_time).min().unwrap_or(0);

                    // Avatar, locale: latest login wins (stable)
                    let latest_login = sessions.iter().max_by_key(|u| u.login_time);

                    let locale = latest_login
                        .map(|u| u.locale.clone())
                        .unwrap_or_else(|| DEFAULT_LOCALE.to_string());

                    let avatar = latest_login.and_then(|u| u.avatar.clone());

                    // Away/status: most recently active wins (accurate presence)
                    let most_active = sessions.iter().max_by_key(|u| u.last_activity);

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
                    // Resolved in-transaction by `update_user`, so
                    // it's always Some — no admin-aware fallback
                    // layer needed.
                    bandwidth_weight: Some(resolved_bandwidth_weight),
                };

                let user_updated = ServerMessage::UserUpdated {
                    previous_username: old_username.clone(),
                    user: user_info,
                };
                ctx.user_manager
                    .broadcast_to_permission(user_updated, Permission::UserList)
                    .await;
            }

            ctx.send_message(&response).await
        }
        Ok(crate::db::UpdateUserResult::BlockedForGroupAuth) => {
            // In-tx group-auth race — admin altered the target group
            // between the handler's pre-check and the tx. Conservative
            // message: we don't know which of the four conditions raced.
            warn!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_USER_UPDATE_PERMISSION_DENIED);
            let response = ServerMessage::UserUpdateResponse {
                success: false,
                error: Some(err_update_failed(ctx.locale, &target_username)),
                id: None,
                username: None,
            };
            ctx.send_message(&response).await
        }
        Ok(crate::db::UpdateUserResult::Blocked) => {
            // Update was blocked (user not found, last admin, duplicate
            // username, or non-admin requester racing a concurrent
            // promotion of the target).
            let target_after = ctx
                .db
                .users
                .get_user_by_username(&target_username)
                .await
                .ok()
                .flatten();
            let error_message = if target_after.is_none() {
                err_user_not_found(ctx.locale, &target_username)
            } else if !requesting_user.is_admin && target_after.as_ref().is_some_and(|u| u.is_admin)
            {
                // Race: handler's pre-check at the top saw target as
                // non-admin; an admin promoted them between then and the
                // SQL UPDATE, which then refused the write. Surface the
                // same error the pre-check would have produced.
                warn!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_USER_UPDATE_ADMIN);
                err_cannot_edit_admin(ctx.locale)
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
                error: Some(error_message),
                id: None,
                username: None,
            };
            ctx.send_message(&response).await
        }
        Err(e) => {
            error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_USER_UPDATE_DB_ERROR);
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some(HANDLER_USER_UPDATE))
                .await;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
                bandwidth_weight: None,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
    async fn test_userupdate_non_admin_cannot_edit_own_username() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let alice_user = test_ctx
            .db
            .users
            .get_user_by_username("alice")
            .await
            .unwrap()
            .unwrap();
        let request = UserUpdateRequest {
            id: alice_user.id,
            current_password: None,
            username: Some("alice2".to_string()),
            password: None,
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
    async fn test_userupdate_admin_can_edit_own_username() {
        let mut test_ctx = create_test_context().await;

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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
            session_id: Some(session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse {
                success,
                error,
                username,
                ..
            } => {
                assert!(success, "admin self-rename should succeed: {:?}", error);
                assert_eq!(username, Some("admin2".to_string()));
            }
            _ => panic!("Expected UserUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_userupdate_admin_can_edit_own_bandwidth_weight() {
        let mut test_ctx = create_test_context().await;

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
            username: None,
            password: None,
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            bandwidth_weight: Some(42),
            inherit_bandwidth_weight: None,
            session_id: Some(session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(
                    success,
                    "admin self-bandwidth-weight should succeed: {:?}",
                    error
                );
            }
            _ => panic!("Expected UserUpdateResponse"),
        }

        let updated = test_ctx
            .db
            .users
            .get_user_by_username("admin")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated.bandwidth_weight, Some(42));
    }

    #[tokio::test]
    async fn test_userupdate_admin_can_clear_own_bandwidth_weight() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let admin_user = test_ctx
            .db
            .users
            .get_user_by_username("admin")
            .await
            .unwrap()
            .unwrap();

        // First, set a per-user override
        test_ctx
            .db
            .users
            .update_user(db::UpdateUserParams {
                username: "admin",
                new_username: None,
                new_password_hash: None,
                is_admin: None,
                enabled: None,
                permissions: None,
                revokes: None,
                remove_group: false,
                group_id: None,
                bandwidth_weight: Some(99),
                inherit_bandwidth_weight: false,
                requester_is_admin: true,
                permission_write_scope: db::PermissionWriteScope::ReplaceAll,
                requester_bandwidth_max: None,
            })
            .await
            .unwrap();

        let request = UserUpdateRequest {
            id: admin_user.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            bandwidth_weight: None,
            inherit_bandwidth_weight: Some(true),
            session_id: Some(session_id),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(
                    success,
                    "admin self-clear-bandwidth should succeed: {:?}",
                    error
                );
            }
            _ => panic!("Expected UserUpdateResponse"),
        }

        let updated = test_ctx
            .db
            .users
            .get_user_by_username("admin")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated.bandwidth_weight, None);
    }

    #[tokio::test]
    async fn test_userupdate_non_admin_cannot_edit_own_bandwidth_weight() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let alice_user = test_ctx
            .db
            .users
            .get_user_by_username("alice")
            .await
            .unwrap()
            .unwrap();
        let request = UserUpdateRequest {
            id: alice_user.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            bandwidth_weight: Some(42),
            inherit_bandwidth_weight: None,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
    async fn test_userupdate_cannot_edit_self_revokes() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

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
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: Some(vec!["chat_send".to_string()]), // Trying to revoke from self
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
    async fn test_userupdate_cannot_edit_self_remove_group() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

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
            permissions: None,
            group_id: None,
            remove_group: Some(true), // Trying to clear own group membership
            revokes: None,
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
    async fn test_userupdate_admin_self_edit_group_id_rejected() {
        // Admin self-edit: setting group_id on self is rejected with the
        // admin-XOR-group error (replacing the prior silent-ignore behavior).
        let mut test_ctx = create_test_context().await;

        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let group = test_ctx
            .db
            .groups
            .create_group(
                "Staff",
                false,
                &db::Permissions::from(&[db::Permission::ChatSend]),
                1,
            )
            .await
            .unwrap();

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
            enabled: None,
            permissions: None,
            group_id: Some(group.id),
            remove_group: None,
            revokes: None,
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
            session_id: Some(admin_session),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(
                    error.unwrap(),
                    err_admin_cannot_have_group(DEFAULT_TEST_LOCALE)
                );
            }
            _ => panic!("Expected UserUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_userupdate_assign_group_to_existing_admin_rejected() {
        // Non-self edit: admin A tries to assign a group to existing admin B.
        // Should reject with err_admin_cannot_have_group (admin XOR group).
        let mut test_ctx = create_test_context().await;

        let admin_a_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a second admin via DB (no group, satisfies CHECK).
        let admin_b = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "adminb",
                hashed_password: "hash",
                is_admin: true,
                is_shared: false,
                enabled: true,
                permissions: &db::Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        let group = test_ctx
            .db
            .groups
            .create_group(
                "Staff",
                false,
                &db::Permissions::from(&[db::Permission::ChatSend]),
                1,
            )
            .await
            .unwrap();

        let request = UserUpdateRequest {
            id: admin_b.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: Some(group.id),
            remove_group: None,
            revokes: None,
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
            session_id: Some(admin_a_session),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(
                    error.unwrap(),
                    err_admin_cannot_have_group(DEFAULT_TEST_LOCALE)
                );
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
                bandwidth_weight: None,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
                bandwidth_weight: None,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
                bandwidth_weight: None,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
                bandwidth_weight: None,
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
                bandwidth_weight: None,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
                bandwidth_weight: None,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
                bandwidth_weight: None,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
                bandwidth_weight: None,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
                1,
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
                bandwidth_weight: None,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
                1,
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
                bandwidth_weight: None,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
                bandwidth_weight: None,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
                bandwidth_weight: None,
                inherit_bandwidth_weight: false,
                requester_is_admin: true,
                permission_write_scope: db::PermissionWriteScope::ReplaceAll,
                requester_bandwidth_max: None,
            })
            .await
            .unwrap();
        assert!(
            matches!(result, db::UpdateUserResult::Blocked),
            "Should not be able to disable the last admin"
        );

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
                bandwidth_weight: None,
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
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
                bandwidth_weight: None,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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

    /// A request that names the same permission in both `permissions`
    /// (grants) and `revokes` is rejected upfront with a clear error.
    /// The `(user_id, permission)` primary key only allows one row per
    /// permission, so the write path would otherwise have to pick a
    /// winner by ordering — the new pre-tx check makes the intent
    /// explicit and fails the request with
    /// `err_permission_grant_revoke_conflict` rather than letting the
    /// DB resolve it implicitly.
    ///
    /// Note: revokes only parse to `Some(...)` when the target has (or
    /// is being assigned to) a group — handler logic drops revokes for
    /// ungrouped users since there's nothing for them to revoke. This
    /// test puts the target in a group so both lists reach the overlap
    /// check.
    #[tokio::test]
    async fn test_userupdate_rejects_overlapping_grant_and_revoke() {
        let mut test_ctx = create_test_context().await;

        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let group = test_ctx
            .db
            .groups
            .create_group(
                "Staff",
                false,
                &Permissions::from(&[Permission::ChatSend]),
                1,
            )
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
                group_id: Some(group.id),
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        // Same permission in both lists — should fail upfront.
        let request = UserUpdateRequest {
            id: bob.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: None,
            enabled: None,
            permissions: Some(vec!["chat_send".to_string()]),
            group_id: None,
            remove_group: None,
            revokes: Some(vec!["chat_send".to_string()]),
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
            session_id: Some(admin_session),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;
        assert!(
            result.is_ok(),
            "should return an error response, not disconnect"
        );

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(!success, "overlapping grant + revoke must be rejected");
                let msg = error.expect("error message must be present");
                assert!(
                    msg.contains("chat_send"),
                    "error must name the conflicting permission, got: {msg}"
                );
            }
            other => panic!("expected UserUpdateResponse, got {:?}", other),
        }

        // Bob's DB rows must be unchanged — no partial write. Bob still
        // has zero override rows (group provides ChatSend; no grant or
        // revoke overrides were stored).
        let perms_after = test_ctx
            .db
            .users
            .get_user_permissions(bob.id)
            .await
            .unwrap();
        // Effective perms == group perms only; no override rows landed.
        assert_eq!(
            perms_after.permissions.len(),
            1,
            "only the group's ChatSend should resolve, no overrides written"
        );
        assert!(perms_after.permissions.contains(&Permission::ChatSend));
    }

    /// Regression: a single `UserUpdate` that both renames AND disables a
    /// user must still disconnect the active session. Pre-fix, the cache
    /// rename ran after the disable-disconnect lookup, so
    /// `get_session_ids_for_user(&new_username)` found nothing while the
    /// cache still carried the old username — the renamed user stayed
    /// online despite being disabled.
    #[tokio::test]
    async fn test_userupdate_rename_and_disable_disconnects_session() {
        let mut test_ctx = create_test_context().await;

        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;
        let bob_session = login_user(&mut test_ctx, "bob", "password", &[], false).await;

        assert!(
            test_ctx
                .user_manager
                .get_user_by_session_id(bob_session)
                .await
                .is_some(),
            "bob should be in user manager"
        );

        let bob_user = test_ctx
            .db
            .users
            .get_user_by_username("bob")
            .await
            .unwrap()
            .unwrap();

        // Rename AND disable in one request — the bug only manifests
        // when both fields move together.
        let request = UserUpdateRequest {
            id: bob_user.id,
            current_password: None,
            username: Some("bob2".to_string()),
            password: None,
            is_admin: None,
            enabled: Some(false),
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
            session_id: Some(admin_session),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());

        assert!(
            test_ctx
                .user_manager
                .get_user_by_session_id(bob_session)
                .await
                .is_none(),
            "bob's session must be removed from user manager after a rename + disable"
        );

        let bob_after = test_ctx
            .db
            .users
            .get_user_by_username("bob2")
            .await
            .unwrap()
            .unwrap();
        assert!(!bob_after.enabled, "bob2 should be disabled in DB");
    }

    /// Regression: demoting an admin who is currently in voice must
    /// kick them from voice, even if they had no explicit
    /// `VoiceListen` grant. Admins implicitly hold every permission via
    /// `UserSession::has_permission`'s bypass; the previous check
    /// looked only at the stored grants and so saw `had_voice_listen
    /// = false` for any admin without an explicit grant, skipping the
    /// cleanup and leaving the demoted admin receiving relayed audio.
    #[tokio::test]
    async fn test_userupdate_demoted_admin_loses_voice() {
        use std::collections::HashSet;

        let mut test_ctx = create_test_context().await;

        // First admin (the requester) — stays admin so the second
        // demotion isn't blocked by last-admin protection.
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Second admin (the demotion target). No explicit VoiceListen
        // grant — they rely on the admin bypass.
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "voiceadmin",
                hashed_password: "hash",
                is_admin: true,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        let voiceadmin_db = test_ctx
            .db
            .users
            .get_user_by_username("voiceadmin")
            .await
            .unwrap()
            .unwrap();

        // Add the admin's session to UserManager with `is_admin: true`
        // and an empty permission set (matches how admins are seeded
        // at login — bypass lives in `has_permission`, not in the set).
        let voiceadmin_session = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                user_id: voiceadmin_db.id,
                username: "voiceadmin".to_string(),
                nickname: "voiceadmin".to_string(),
                is_admin: true,
                is_shared: false,
                permissions: HashSet::new(),
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
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
                last_activity: std::time::Instant::now(),
            })
            .await
            .expect("Failed to add voiceadmin session");

        // Put voiceadmin in voice.
        let voice_session = crate::voice::VoiceSession::new(
            "voiceadmin".to_string(),
            vec!["#general".to_string()],
            voiceadmin_session,
            test_ctx.peer_addr.ip(),
        );
        test_ctx.voice_registry.add(voice_session).await;
        assert!(
            test_ctx
                .voice_registry
                .has_session(voiceadmin_session)
                .await,
            "voiceadmin must start out in the voice registry"
        );

        // Admin demotes voiceadmin to non-admin. No explicit permissions
        // are granted, so `final_permissions` resolves to empty —
        // matching the exact scenario the admin-bypass-aware check is
        // there to catch.
        let request = UserUpdateRequest {
            id: voiceadmin_db.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: Some(false),
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
            session_id: Some(admin_session),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());

        // Voice cleanup must have fired — pre-fix this assertion failed
        // because `had_voice_listen` looked at the empty stored grant
        // set and saw `false`, so the cleanup branch never ran.
        assert!(
            !test_ctx
                .voice_registry
                .has_session(voiceadmin_session)
                .await,
            "demoted admin must be kicked from voice (admin bypass meant they were effectively voiced)"
        );

        // Cache flip is also visible: the demoted session is no longer
        // `is_admin` in UserManager either, so future privileged
        // requests would fail at the cached check.
        let session_after = test_ctx
            .user_manager
            .get_user_by_session_id(voiceadmin_session)
            .await
            .expect("session must still be present in UserManager (only voice was cleaned)");
        assert!(
            !session_after.is_admin,
            "session cache must reflect the demotion immediately after the DB commit"
        );
    }

    /// Regression: renaming a SHARED account must not rewrite the
    /// voice-registry nickname of that account's in-voice sessions.
    /// Shared accounts keep per-session nicknames chosen at login —
    /// `UserManager::update_username` honors that by skipping
    /// `user.nickname` for shared sessions. The handler's
    /// voice-registry update used to lack the same gate, so a renamed
    /// shared account's voice nickname would silently flip from the
    /// participant-chosen handle to the account username.
    #[tokio::test]
    async fn test_userupdate_rename_shared_preserves_voice_nickname() {
        use std::collections::HashSet;

        let mut test_ctx = create_test_context().await;

        // Create a shared account in the DB.
        let mut perms = Permissions::new();
        perms.permissions.insert(Permission::VoiceListen);
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "lounge",
                hashed_password: "",
                is_admin: false,
                is_shared: true,
                enabled: true,
                permissions: &perms,
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let lounge_db = test_ctx
            .db
            .users
            .get_user_by_username("lounge")
            .await
            .unwrap()
            .unwrap();

        // Shared session with a nickname DIFFERENT from the account name —
        // this is the participant's chosen handle and must survive a rename.
        let session_perms: HashSet<Permission> =
            [Permission::VoiceListen].iter().copied().collect();
        let lounge_session = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                user_id: lounge_db.id,
                username: "lounge".to_string(),
                nickname: "vibes".to_string(),
                is_admin: false,
                is_shared: true,
                permissions: session_perms,
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
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
                last_activity: std::time::Instant::now(),
            })
            .await
            .expect("Failed to add shared session");

        // Put that session in voice under the chosen nickname.
        let voice_session = crate::voice::VoiceSession::new(
            "vibes".to_string(),
            vec!["#general".to_string()],
            lounge_session,
            test_ctx.peer_addr.ip(),
        );
        test_ctx.voice_registry.add(voice_session).await;

        // Admin renames the shared account.
        let request = UserUpdateRequest {
            id: lounge_db.id,
            current_password: None,
            username: Some("lounge2".to_string()),
            password: None,
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
            session_id: Some(admin_session),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());

        // Voice nickname must be unchanged — the participant chose "vibes",
        // a rename of the *account* shouldn't move it.
        let voice_after = test_ctx
            .voice_registry
            .get_by_session_id(lounge_session)
            .await
            .expect("voice session must still exist after rename");
        assert_eq!(
            voice_after.nickname, "vibes",
            "shared-account rename must not rewrite the per-session voice nickname"
        );

        // Sanity: the UserManager session's nickname is also unchanged
        // (this part was already correct in `update_username`).
        let session_after = test_ctx
            .user_manager
            .get_user_by_session_id(lounge_session)
            .await
            .expect("session must still be in UserManager");
        assert_eq!(session_after.nickname, "vibes");
        assert_eq!(session_after.username, "lounge2");
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
                bandwidth_weight: None,
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
                bandwidth_weight: None,
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
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
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
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
                bandwidth_weight: None,
                inherit_bandwidth_weight: false,
                requester_is_admin: true,
                permission_write_scope: db::PermissionWriteScope::ReplaceAll,
                requester_bandwidth_max: None,
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
                bandwidth_weight: None,
                inherit_bandwidth_weight: false,
                requester_is_admin: true,
                permission_write_scope: db::PermissionWriteScope::ReplaceAll,
                requester_bandwidth_max: None,
            })
            .await;

        // Should return Ok(UpdateUserResult::Blocked) - update blocked by atomic SQL protection
        assert!(result.is_ok());
        assert!(
            matches!(result.unwrap(), db::UpdateUserResult::Blocked),
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
    async fn test_userupdate_shared_user_cannot_self_edit() {
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
                bandwidth_weight: None,
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

        // Shared accounts are blocked from any self-edit, regardless of field.
        // The password is just a convenient probe.
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
            session_id: shared_session_id,
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Should send error response");

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(!success, "Shared user must not be able to self-edit");
                assert_eq!(
                    error.unwrap(),
                    err_shared_cannot_self_edit(DEFAULT_TEST_LOCALE)
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
                bandwidth_weight: None,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
                bandwidth_weight: None,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
                bandwidth_weight: None,
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
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
                bandwidth_weight: None,
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
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
                bandwidth_weight: None,
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
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
                bandwidth_weight: None,
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
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
                bandwidth_weight: None,
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
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
                bandwidth_weight: None,
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
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
                bandwidth_weight: None,
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
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
                bandwidth_weight: None,
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
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
                bandwidth_weight: None,
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
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
                bandwidth_weight: None,
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
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
                bandwidth_weight: None,
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
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
                bandwidth_weight: None,
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
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
                1,
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
                bandwidth_weight: None,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
                1,
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
                bandwidth_weight: None,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
                1,
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
                bandwidth_weight: None,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
                1,
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
                bandwidth_weight: None,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
    async fn test_userupdate_non_admin_cannot_promote_to_higher_weight_group() {
        let mut test_ctx = create_test_context().await;

        // editor's group: weight 10 with [UserEdit, ChatSend]
        let editor_group = test_ctx
            .db
            .groups
            .create_group(
                "Editors",
                false,
                &db::Permissions::from(&[db::Permission::UserEdit, db::Permission::ChatSend]),
                10,
            )
            .await
            .unwrap();

        // Login editor and assign them to their group (weight 10 inherited).
        let editor_session = login_user(
            &mut test_ctx,
            "editor",
            "password",
            &[db::Permission::UserEdit, db::Permission::ChatSend],
            false,
        )
        .await;
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;
        let editor = test_ctx
            .db
            .users
            .get_user_by_username("editor")
            .await
            .unwrap()
            .unwrap();
        // Admin moves editor into the editors group so their session weight is 10.
        let assign_request = UserUpdateRequest {
            id: editor.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: Some(editor_group.id),
            remove_group: None,
            revokes: None,
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
            session_id: Some(admin_session),
        };
        let result = handle_user_update(assign_request, &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());
        let _ = read_server_message(&mut test_ctx).await;

        // High-weight group with the same permission set so the permission
        // delegation rule alone would pass.
        let high_group = test_ctx
            .db
            .groups
            .create_group(
                "PowerMods",
                false,
                &db::Permissions::from(&[db::Permission::UserEdit, db::Permission::ChatSend]),
                50,
            )
            .await
            .unwrap();

        // A target user the editor wants to promote.
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
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        // Editor tries to assign bob to the higher-weight group.
        let request = UserUpdateRequest {
            id: bob.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: Some(high_group.id),
            remove_group: None,
            revokes: None,
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
            session_id: Some(editor_session),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;
        assert!(result.is_ok(), "Should send error, not disconnect");

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(
                    error.unwrap(),
                    err_bandwidth_weight_delegation(DEFAULT_TEST_LOCALE)
                );
            }
            _ => panic!("Expected UserUpdateResponse"),
        }

        let bob = test_ctx
            .db
            .users
            .get_user_by_username("bob")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            bob.group_id, None,
            "Group should not have been assigned (escalation blocked)"
        );
    }

    #[tokio::test]
    async fn test_userupdate_non_admin_can_assign_to_equal_weight_group() {
        let mut test_ctx = create_test_context().await;

        let editor_group = test_ctx
            .db
            .groups
            .create_group(
                "Editors",
                false,
                &db::Permissions::from(&[db::Permission::UserEdit, db::Permission::ChatSend]),
                10,
            )
            .await
            .unwrap();
        let editor_session = login_user(
            &mut test_ctx,
            "editor",
            "password",
            &[db::Permission::UserEdit, db::Permission::ChatSend],
            false,
        )
        .await;
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;
        let editor = test_ctx
            .db
            .users
            .get_user_by_username("editor")
            .await
            .unwrap()
            .unwrap();
        let assign_request = UserUpdateRequest {
            id: editor.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: Some(editor_group.id),
            remove_group: None,
            revokes: None,
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
            session_id: Some(admin_session),
        };
        let result = handle_user_update(assign_request, &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());
        let _ = read_server_message(&mut test_ctx).await;

        // Same weight as editor's group: permitted.
        let peer_group = test_ctx
            .db
            .groups
            .create_group(
                "Reviewers",
                false,
                &db::Permissions::from(&[db::Permission::ChatSend]),
                10,
            )
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
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        let request = UserUpdateRequest {
            id: bob.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: Some(peer_group.id),
            remove_group: None,
            revokes: None,
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
            session_id: Some(editor_session),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(
                    success,
                    "equal-weight assignment should succeed: {:?}",
                    error
                );
            }
            _ => panic!("Expected UserUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_userupdate_admin_can_assign_to_any_weight_group() {
        let mut test_ctx = create_test_context().await;

        // Admin's own weight resolves to DEFAULT_ADMIN_BANDWIDTH_WEIGHT (50),
        // but the bypass means even weight-1000 groups are fair game.
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;
        let high_group = test_ctx
            .db
            .groups
            .create_group("VIP", false, &db::Permissions::new(), 1000)
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
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        let request = UserUpdateRequest {
            id: bob.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: Some(high_group.id),
            remove_group: None,
            revokes: None,
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
            session_id: Some(admin_session),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(success, "admin bypass should succeed: {:?}", error);
            }
            _ => panic!("Expected UserUpdateResponse"),
        }
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
                1,
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
                bandwidth_weight: None,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
                1,
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
                bandwidth_weight: None,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
                1,
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
                1,
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
                bandwidth_weight: None,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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
                1,
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
                bandwidth_weight: None,
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
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

    /// Helper for the delegation tests below: log a non-admin user in and
    /// move them into a group at `requester_weight`, so their cached session
    /// weight reflects that value. Returns the (admin_session, editor_session,
    /// editor_id, editor_group_id) tuple.
    async fn setup_editor_with_weight(
        test_ctx: &mut crate::handlers::testing::TestContext,
        requester_weight: u16,
    ) -> (u32, u32, i64, i64) {
        let editor_group = test_ctx
            .db
            .groups
            .create_group(
                "Editors",
                false,
                &db::Permissions::from(&[db::Permission::UserEdit, db::Permission::ChatSend]),
                requester_weight,
            )
            .await
            .unwrap();
        let editor_session = login_user(
            test_ctx,
            "editor",
            "password",
            &[db::Permission::UserEdit, db::Permission::ChatSend],
            false,
        )
        .await;
        let admin_session = login_user(test_ctx, "admin", "password", &[], true).await;
        let editor = test_ctx
            .db
            .users
            .get_user_by_username("editor")
            .await
            .unwrap()
            .unwrap();
        handle_user_update(
            UserUpdateRequest {
                id: editor.id,
                current_password: None,
                username: None,
                password: None,
                is_admin: None,
                enabled: None,
                permissions: None,
                group_id: Some(editor_group.id),
                remove_group: None,
                revokes: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: None,
                session_id: Some(admin_session),
            },
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();
        let _ = read_server_message(test_ctx).await;
        (admin_session, editor_session, editor.id, editor_group.id)
    }

    #[tokio::test]
    async fn test_userupdate_non_admin_can_set_lower_bandwidth_weight() {
        let mut test_ctx = create_test_context().await;
        // Editor has resolved weight 25.
        let (_admin_session, editor_session, _editor_id, _editor_group_id) =
            setup_editor_with_weight(&mut test_ctx, 25).await;

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
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        // Setting bob's weight to 10 (≤ 25) is allowed under delegation.
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
            revokes: None,
            bandwidth_weight: Some(10),
            inherit_bandwidth_weight: None,
            session_id: Some(editor_session),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(success, "delegation-OK weight should succeed: {:?}", error);
            }
            _ => panic!("Expected UserUpdateResponse"),
        }

        let bob = test_ctx
            .db
            .users
            .get_user_by_username("bob")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(bob.bandwidth_weight, Some(10));
    }

    #[tokio::test]
    async fn test_userupdate_non_admin_cannot_set_higher_bandwidth_weight() {
        let mut test_ctx = create_test_context().await;
        let (_admin_session, editor_session, _editor_id, _editor_group_id) =
            setup_editor_with_weight(&mut test_ctx, 25).await;

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
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        // Setting bob's weight to 100 (> 25) is rejected.
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
            revokes: None,
            bandwidth_weight: Some(100),
            inherit_bandwidth_weight: None,
            session_id: Some(editor_session),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(
                    error.unwrap(),
                    err_bandwidth_weight_delegation(DEFAULT_TEST_LOCALE)
                );
            }
            _ => panic!("Expected UserUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_userupdate_non_admin_cannot_inherit_when_inherited_exceeds_weight() {
        let mut test_ctx = create_test_context().await;
        let (_admin_session, editor_session, _editor_id, _editor_group_id) =
            setup_editor_with_weight(&mut test_ctx, 25).await;

        // Bob is in a HIGH-weight group (100) with override=10. Effective = 10.
        // Editor (weight 25) wants to clear bob's override: post-clear effective
        // would be 100 (the group's weight) > 25 → reject.
        let high_group = test_ctx
            .db
            .groups
            .create_group("Heavy", false, &db::Permissions::new(), 100)
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
                group_id: Some(high_group.id),
                revokes: &[],
                bandwidth_weight: Some(10),
            })
            .await
            .unwrap();

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
            revokes: None,
            bandwidth_weight: None,
            inherit_bandwidth_weight: Some(true),
            session_id: Some(editor_session),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(!success);
                // Inherit path uses its own key (distinct from the set-path's
                // `err_bandwidth_weight_delegation` — see errors.rs).
                assert_eq!(
                    error.unwrap(),
                    err_bandwidth_weight_inherit_would_elevate(DEFAULT_TEST_LOCALE)
                );
            }
            _ => panic!("Expected UserUpdateResponse"),
        }

        // Bob's override should be unchanged.
        let bob = test_ctx
            .db
            .users
            .get_user_by_username("bob")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(bob.bandwidth_weight, Some(10));
    }

    /// Regression: the inherit-delegation check must resolve the inherited
    /// weight against the POST-update group, not the pre-update one. If a
    /// non-admin requests `{remove_group: true, inherit: true}` on a user
    /// who's currently in a high-weight group, the OLD group's weight is
    /// irrelevant — the user will inherit `DEFAULT_BANDWIDTH_WEIGHT` after
    /// the group is removed. Rejecting based on the old group is a false
    /// rejection on a request that would actually *lower* the user.
    #[tokio::test]
    async fn test_userupdate_inherit_delegation_uses_post_update_group_on_remove() {
        let mut test_ctx = create_test_context().await;
        let (_admin_session, editor_session, _editor_id, _editor_group_id) =
            setup_editor_with_weight(&mut test_ctx, 25).await;

        // Bob currently in a HIGH-weight group (100) with override(75).
        // Editor (weight 25) sends `{remove_group: true, inherit: true}`.
        // POST-update: no group, no override → effective = DEFAULT (1).
        // Pre-fix this rejected because the resolver used the OLD group (100).
        let high_group = test_ctx
            .db
            .groups
            .create_group("Heavy", false, &db::Permissions::new(), 100)
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
                group_id: Some(high_group.id),
                revokes: &[],
                bandwidth_weight: Some(75),
            })
            .await
            .unwrap();

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
            bandwidth_weight: None,
            inherit_bandwidth_weight: Some(true),
            session_id: Some(editor_session),
        };
        handle_user_update(request, &mut test_ctx.handler_context())
            .await
            .unwrap();
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(
                    success,
                    "delegation must use POST-update group, allowing this drop-down: {:?}",
                    error
                );
            }
            other => panic!("Expected UserUpdateResponse, got {:?}", other),
        }

        // DB reflects both changes: no group, no override.
        let bob_after = test_ctx
            .db
            .users
            .get_user_by_username("bob")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(bob_after.group_id, None);
        assert_eq!(bob_after.bandwidth_weight, None);
    }

    /// Regression companion: same `inherit: true` request, but the proposed
    /// NEW group also exceeds the requester's weight. The delegation check
    /// must reject based on the new group's weight, not the old. (Without
    /// the post-update fix, this might still reject for the WRONG reason —
    /// based on the old group's weight, which happens to also exceed.
    /// Picking values where old < requester < new exercises the bug
    /// cleanly: the new check rejects, the old check would have allowed.)
    #[tokio::test]
    async fn test_userupdate_inherit_delegation_uses_post_update_group_on_assign() {
        let mut test_ctx = create_test_context().await;
        let (_admin_session, editor_session, _editor_id, _editor_group_id) =
            setup_editor_with_weight(&mut test_ctx, 25).await;

        // Bob currently in a LOW-weight group (5) — below the editor (25).
        // Editor sends `{group_id: Some(high), inherit: true}` where the
        // NEW group has weight 100 (> 25). Pre-fix the resolver used the
        // OLD group (5) and would have passed the delegation check; the
        // separate new-group check at lines 765-776 would catch it, but
        // the inherit check should reject too — defense-in-depth standalone.
        let low_group = test_ctx
            .db
            .groups
            .create_group("Light", false, &db::Permissions::new(), 5)
            .await
            .unwrap();
        let high_group = test_ctx
            .db
            .groups
            .create_group("Heavy", false, &db::Permissions::new(), 100)
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
                group_id: Some(low_group.id),
                revokes: &[],
                bandwidth_weight: Some(10),
            })
            .await
            .unwrap();

        let request = UserUpdateRequest {
            id: bob.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: Some(high_group.id),
            remove_group: None,
            revokes: None,
            bandwidth_weight: None,
            inherit_bandwidth_weight: Some(true),
            session_id: Some(editor_session),
        };
        handle_user_update(request, &mut test_ctx.handler_context())
            .await
            .unwrap();
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(
                    !success,
                    "delegation must reject based on NEW group's weight"
                );
                // Inherit path uses its own key.
                assert_eq!(
                    error.unwrap(),
                    err_bandwidth_weight_inherit_would_elevate(DEFAULT_TEST_LOCALE)
                );
            }
            other => panic!("Expected UserUpdateResponse, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_userupdate_admin_can_set_any_bandwidth_weight() {
        let mut test_ctx = create_test_context().await;
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;
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
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        // Admin sets bob's weight to a value far above any non-admin's reach.
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
            revokes: None,
            bandwidth_weight: Some(10_000),
            inherit_bandwidth_weight: None,
            session_id: Some(admin_session),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(success, "admin bypass should succeed: {:?}", error);
            }
            _ => panic!("Expected UserUpdateResponse"),
        }
        let bob = test_ctx
            .db
            .users
            .get_user_by_username("bob")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(bob.bandwidth_weight, Some(10_000));
    }

    #[tokio::test]
    async fn test_userupdate_admin_promotion_refreshes_cached_weight() {
        // Promoting a non-admin to admin should refresh the cached
        // bandwidth_weight (admins skip group lookup and resolve to
        // DEFAULT_ADMIN_BANDWIDTH_WEIGHT). Without the refresh, the
        // scheduler would read the stale pre-promotion value.
        use std::sync::atomic::Ordering;

        let mut test_ctx = create_test_context().await;
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Bob logs in as a non-admin → cached weight starts at default 1.
        let bob_session = login_user(&mut test_ctx, "bob", "password", &[], false).await;
        let bob = test_ctx
            .db
            .users
            .get_user_by_username("bob")
            .await
            .unwrap()
            .unwrap();

        let initial_weight = test_ctx
            .user_manager
            .get_user_by_session_id(bob_session)
            .await
            .unwrap()
            .bandwidth_weight
            .load(Ordering::Relaxed);
        assert_eq!(initial_weight, 1, "non-admin starts at default");

        // Admin promotes bob to admin.
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
            session_id: Some(admin_session),
        };
        handle_user_update(request, &mut test_ctx.handler_context())
            .await
            .unwrap();
        let _ = read_server_message(&mut test_ctx).await; // response
        while test_ctx.rx.try_recv().is_ok() {} // drain cascade

        let promoted_weight = test_ctx
            .user_manager
            .get_user_by_session_id(bob_session)
            .await
            .unwrap()
            .bandwidth_weight
            .load(Ordering::Relaxed);
        assert_eq!(
            promoted_weight,
            nexus_common::validators::DEFAULT_ADMIN_BANDWIDTH_WEIGHT,
            "promotion to admin should refresh cached weight to admin default"
        );
    }

    #[tokio::test]
    async fn test_userupdate_admin_promotion_clears_cached_group() {
        // Promoting an in-group non-admin to admin auto-clears their
        // group_id in the DB. The session cache must follow — otherwise
        // get_sessions_by_group_id keeps returning this admin and UserList
        // reports them as still in the old group until re-login.
        let mut test_ctx = create_test_context().await;
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let group = test_ctx
            .db
            .groups
            .create_group(
                "Staff",
                false,
                &db::Permissions::from(&[db::Permission::ChatSend]),
                1,
            )
            .await
            .unwrap();

        // Bob logs in as a non-admin, then gets assigned to Staff via DB
        // + session cache update.
        let bob_session = login_user(&mut test_ctx, "bob", "password", &[], false).await;
        let bob = test_ctx
            .db
            .users
            .get_user_by_username("bob")
            .await
            .unwrap()
            .unwrap();
        test_ctx
            .db
            .users
            .update_user(db::UpdateUserParams {
                username: "bob",
                new_username: None,
                new_password_hash: None,
                is_admin: None,
                enabled: None,
                permissions: None,
                revokes: None,
                remove_group: false,
                group_id: Some(group.id),
                bandwidth_weight: None,
                inherit_bandwidth_weight: false,
                requester_is_admin: true,
                permission_write_scope: db::PermissionWriteScope::ReplaceAll,
                requester_bandwidth_max: None,
            })
            .await
            .unwrap();
        test_ctx
            .user_manager
            .update_group(bob.id, Some(group.id), Some("Staff".to_string()))
            .await;

        // Sanity check: session has the group set before promotion.
        let pre = test_ctx
            .user_manager
            .get_user_by_session_id(bob_session)
            .await
            .unwrap();
        assert_eq!(pre.group_id, Some(group.id), "pre-promotion group_id");
        assert_eq!(pre.group_name.as_deref(), Some("Staff"));

        // Promote bob to admin.
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
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
            session_id: Some(admin_session),
        };
        handle_user_update(request, &mut test_ctx.handler_context())
            .await
            .unwrap();
        let _ = read_server_message(&mut test_ctx).await; // response
        while test_ctx.rx.try_recv().is_ok() {} // drain cascade

        let post = test_ctx
            .user_manager
            .get_user_by_session_id(bob_session)
            .await
            .unwrap();
        assert_eq!(
            post.group_id, None,
            "promotion must clear cached group_id (admin XOR group)"
        );
        assert_eq!(
            post.group_name, None,
            "promotion must clear cached group_name"
        );
    }

    /// Invariant: a single `UserUpdate` call that touches multiple
    /// UserInfo-visible fields must produce exactly one `UserUpdated`
    /// broadcast per receiver — not one per changed field. Mirrors the
    /// same invariant pinned for `group_update.rs`. If a future refactor
    /// splits the broadcast back into per-field emits, this test fails
    /// before the multi-broadcast hits the wire.
    #[tokio::test]
    async fn test_userupdate_one_broadcast_per_receiver_for_combined_field_change() {
        let mut test_ctx = create_test_context().await;

        let _admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Bob: created in DB, registered as an online session that holds
        // user_list so his tx is a legitimate broadcast target. Admin and bob
        // share `test_ctx.tx`, so one broadcast call lands twice in the queue.
        // UserList must be granted in the DB (not just the session cache) —
        // user_update re-syncs cached permissions from the DB partway through,
        // so a test-only session-cache grant would be wiped before the
        // broadcast iterates eligible recipients.
        let mut bob_db_perms = db::Permissions::new();
        bob_db_perms.permissions.insert(db::Permission::UserList);
        let bob = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: &get_cached_password_hash("password"),
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &bob_db_perms,
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        let mut bob_session_perms = HashSet::new();
        bob_session_perms.insert(db::Permission::UserList);
        let _bob_session = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                user_id: bob.id,
                username: "bob".to_string(),
                is_admin: false,
                is_shared: false,
                permissions: bob_session_perms,
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
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
                last_activity: std::time::Instant::now(),
            })
            .await
            .unwrap();

        // Combined change: username + bandwidth_weight in one call. Both fields
        // trigger the broadcast condition; the consolidated emit should still
        // fire once. (Avoiding is_admin / group / permissions changes here
        // keeps the queue clean of PermissionsUpdated noise — those are a
        // different message type and have their own broadcast.)
        let admin_session = test_ctx
            .user_manager
            .get_sessions_by_username("admin")
            .await
            .first()
            .map(|s| s.session_id);
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
            bandwidth_weight: Some(100),
            inherit_bandwidth_weight: None,
            session_id: admin_session,
        };
        handle_user_update(request, &mut test_ctx.handler_context())
            .await
            .unwrap();

        // First message is the UserUpdateResponse delivered to admin (the
        // request caller). Drain the rest and count UserUpdated for "robert".
        let _response = read_server_message(&mut test_ctx).await;

        let mut user_updated_count = 0;
        let mut other_msgs = Vec::new();
        while let Ok((msg, _)) = test_ctx.rx.try_recv() {
            match msg {
                ServerMessage::UserUpdated { user, .. } if user.username == "robert" => {
                    assert_eq!(user.bandwidth_weight, Some(100));
                    user_updated_count += 1;
                }
                other => other_msgs.push(format!("{:?}", other)),
            }
        }
        assert_eq!(
            user_updated_count, 2,
            "combined username+bandwidth change must emit one broadcast per receiver \
             (one fan-out × 2 sessions on the shared tx = 2), not one per changed field (would be 4). other msgs: {:?}",
            other_msgs
        );
    }

    /// Regression: when a request carries both `bandwidth_weight: Some(N)`
    /// and `inherit_bandwidth_weight: Some(true)`, the DB layer's write
    /// precedence makes inherit win — the stored override is cleared to
    /// NULL regardless of `N`. The handler's broadcast detector must mirror
    /// that precedence; otherwise a request where `N` happens to equal the
    /// pre-update stored value would suppress the broadcast even though
    /// the effective weight just changed (override → inherited baseline).
    #[tokio::test]
    async fn test_userupdate_broadcasts_when_inherit_wins_over_matching_explicit() {
        let mut test_ctx = create_test_context().await;

        let _admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Bob has a per-user override of 50. UserList in DB so the broadcast
        // reaches him (the cached perms get re-synced by the handler).
        let mut bob_db_perms = db::Permissions::new();
        bob_db_perms.permissions.insert(db::Permission::UserList);
        let bob = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: &get_cached_password_hash("password"),
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &bob_db_perms,
                group_id: None,
                revokes: &[],
                bandwidth_weight: Some(50),
            })
            .await
            .unwrap();

        let mut bob_session_perms = HashSet::new();
        bob_session_perms.insert(db::Permission::UserList);
        let _bob_session = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                user_id: bob.id,
                username: "bob".to_string(),
                is_admin: false,
                is_shared: false,
                permissions: bob_session_perms,
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
                bandwidth_weight: 50,
                last_activity: std::time::Instant::now(),
            })
            .await
            .unwrap();

        // Hostile/defensive client: both fields present, with bandwidth_weight
        // matching bob's current stored value. Inherit wins → override cleared
        // → effective weight drops to baseline 1.
        let admin_session = test_ctx
            .user_manager
            .get_sessions_by_username("admin")
            .await
            .first()
            .map(|s| s.session_id);
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
            revokes: None,
            bandwidth_weight: Some(50),           // matches current stored
            inherit_bandwidth_weight: Some(true), // but inherit wins
            session_id: admin_session,
        };
        handle_user_update(request, &mut test_ctx.handler_context())
            .await
            .unwrap();

        let _response = read_server_message(&mut test_ctx).await;

        // Must see at least one UserUpdated for bob with the new effective
        // weight (DEFAULT_BANDWIDTH_WEIGHT = 1, the inherited baseline).
        let mut saw_broadcast = false;
        while let Ok((msg, _)) = test_ctx.rx.try_recv() {
            if let ServerMessage::UserUpdated { user, .. } = msg
                && user.username == "bob"
            {
                assert_eq!(
                    user.bandwidth_weight,
                    Some(nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT),
                    "broadcast must carry the post-update inherited weight"
                );
                saw_broadcast = true;
            }
        }
        assert!(
            saw_broadcast,
            "broadcast must fire when inherit clears an override, even if the request \
             carried a bandwidth_weight that happened to match the cleared value"
        );

        // DB row confirms the precedence: override is now NULL.
        let after = test_ctx
            .db
            .users
            .get_user_by_id(bob.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            after.bandwidth_weight, None,
            "inherit_bandwidth_weight: Some(true) must clear the override regardless of bandwidth_weight value"
        );
    }

    // =========================================================================
    // Admin XOR shared invariant: handler-layer enforcement
    // =========================================================================

    /// Handler enforcement of admin XOR shared: promoting a shared account
    /// to admin must be rejected before any DB write. The schema CHECK is
    /// the storage-layer safety net; this test pins the clean translated
    /// error path on top of it.
    #[tokio::test]
    async fn test_userupdate_rejects_promoting_shared_to_admin() {
        let mut test_ctx = create_test_context().await;
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let shared = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "shared_acct",
                hashed_password: &get_cached_password_hash("password"),
                is_admin: false,
                is_shared: true,
                enabled: true,
                permissions: &db::Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        let request = UserUpdateRequest {
            id: shared.id,
            current_password: None,
            username: None,
            password: None,
            is_admin: Some(true),
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
            session_id: Some(admin_session),
        };
        handle_user_update(request, &mut test_ctx.handler_context())
            .await
            .unwrap();
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(
                    error.unwrap(),
                    crate::handlers::err_shared_cannot_be_admin(DEFAULT_TEST_LOCALE)
                );
            }
            other => panic!("Expected UserUpdateResponse, got {:?}", other),
        }

        // Verify the DB row was NOT mutated.
        let after = test_ctx
            .db
            .users
            .get_user_by_id(shared.id)
            .await
            .unwrap()
            .unwrap();
        assert!(!after.is_admin, "shared account must remain non-admin");
        assert!(after.is_shared, "shared status must be preserved");
    }

    // =========================================================================
    // Self-edit gate matrix
    //
    // The gate sits at the top of `handle_user_update` and has five sequential
    // checkpoints; each test below pins one outcome class. A shared helper
    // (`assert_self_edit_outcome`) sets up the requesting user, runs the
    // handler with a caller-supplied request builder, and asserts the
    // expected outcome.
    // =========================================================================

    enum SelfEditOutcome {
        Success,
        Error(String),
    }

    /// Per-case request builder used by the gate-matrix tests. Type-aliased
    /// to avoid `type_complexity` clippy warnings on each `Vec<(_, ...)>`.
    type SelfEditRequestBuilder = fn(i64, Option<u32>) -> UserUpdateRequest;

    /// Run a self-edit through the handler and assert the outcome. `build_request`
    /// receives the requesting user's database id and session_id (so each call
    /// site only specifies the field(s) under test) and returns a fully-formed
    /// `UserUpdateRequest` targeting self.
    async fn assert_self_edit_outcome<F>(
        requesting_is_admin: bool,
        requesting_is_shared: bool,
        build_request: F,
        expected: SelfEditOutcome,
    ) where
        F: FnOnce(i64, Option<u32>) -> UserUpdateRequest,
    {
        let mut test_ctx = create_test_context().await;

        let (session_id, user_id) = if requesting_is_shared {
            // Shared account: account username "shared_acct" + nickname "alice".
            // Self-edit is keyed on account username, not nickname.
            let sid =
                login_shared_user(&mut test_ctx, "shared_acct", "password", "alice", &[]).await;
            let user = test_ctx
                .db
                .users
                .get_user_by_username("shared_acct")
                .await
                .unwrap()
                .unwrap();
            (sid, user.id)
        } else {
            let sid =
                login_user(&mut test_ctx, "alice", "password", &[], requesting_is_admin).await;
            let user = test_ctx
                .db
                .users
                .get_user_by_username("alice")
                .await
                .unwrap()
                .unwrap();
            (sid, user.id)
        };

        let request = build_request(user_id, Some(session_id));
        handle_user_update(request, &mut test_ctx.handler_context())
            .await
            .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => match expected {
                SelfEditOutcome::Success => {
                    assert!(success, "expected success, got error: {:?}", error);
                }
                SelfEditOutcome::Error(expected_msg) => {
                    assert!(!success, "expected error, got success");
                    assert_eq!(error.unwrap(), expected_msg);
                }
            },
            other => panic!("Expected UserUpdateResponse, got {:?}", other),
        }
    }

    /// Build a UserUpdateRequest with every field defaulted to None, ready
    /// for tests to fill in one field per case.
    fn empty_self_edit_request(user_id: i64, session_id: Option<u32>) -> UserUpdateRequest {
        UserUpdateRequest {
            id: user_id,
            current_password: None,
            username: None,
            password: None,
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            bandwidth_weight: None,
            inherit_bandwidth_weight: None,
            session_id,
        }
    }

    /// Gate checkpoint 1: shared accounts are blocked from self-edit at the
    /// top of the gate, before any field-specific checks. One representative
    /// case is enough; the short-circuit fires before the field matters.
    #[tokio::test]
    async fn test_self_edit_shared_account_rejected_for_any_field() {
        // Probe with `password` — even the most innocuous field is rejected.
        assert_self_edit_outcome(
            false, // requesting_is_admin
            true,  // requesting_is_shared
            |user_id, session_id| UserUpdateRequest {
                current_password: Some("password".to_string()),
                password: Some("newpassword".to_string()),
                ..empty_self_edit_request(user_id, session_id)
            },
            SelfEditOutcome::Error(err_shared_cannot_self_edit(DEFAULT_TEST_LOCALE)),
        )
        .await;
    }

    /// Gate checkpoint 2: the "forbidden on self" field set is blocked for
    /// admins. These fields are never allowed in a self-edit request,
    /// regardless of whether the caller is admin or not.
    #[tokio::test]
    async fn test_self_edit_forbidden_fields_rejected_for_admin() {
        let cases: Vec<(&str, SelfEditRequestBuilder)> = vec![
            ("is_admin", |id, sid| UserUpdateRequest {
                is_admin: Some(false),
                ..empty_self_edit_request(id, sid)
            }),
            ("enabled", |id, sid| UserUpdateRequest {
                enabled: Some(false),
                ..empty_self_edit_request(id, sid)
            }),
            ("permissions", |id, sid| UserUpdateRequest {
                permissions: Some(vec!["chat_send".to_string()]),
                ..empty_self_edit_request(id, sid)
            }),
            ("revokes", |id, sid| UserUpdateRequest {
                revokes: Some(vec!["chat_send".to_string()]),
                ..empty_self_edit_request(id, sid)
            }),
            ("remove_group=true", |id, sid| UserUpdateRequest {
                remove_group: Some(true),
                ..empty_self_edit_request(id, sid)
            }),
        ];

        for (label, builder) in cases {
            eprintln!("self-edit admin forbidden case: {}", label);
            assert_self_edit_outcome(
                true,  // requesting_is_admin
                false, // requesting_is_shared
                builder,
                SelfEditOutcome::Error(err_cannot_edit_self(DEFAULT_TEST_LOCALE)),
            )
            .await;
        }
    }

    /// Gate checkpoint 2 mirror: same forbidden fields are also rejected for
    /// a non-admin caller. The check doesn't depend on `requesting_is_admin`.
    #[tokio::test]
    async fn test_self_edit_forbidden_fields_rejected_for_non_admin() {
        let cases: Vec<(&str, SelfEditRequestBuilder)> = vec![
            ("is_admin", |id, sid| UserUpdateRequest {
                is_admin: Some(true),
                ..empty_self_edit_request(id, sid)
            }),
            ("enabled", |id, sid| UserUpdateRequest {
                enabled: Some(false),
                ..empty_self_edit_request(id, sid)
            }),
            ("permissions", |id, sid| UserUpdateRequest {
                permissions: Some(vec!["chat_send".to_string()]),
                ..empty_self_edit_request(id, sid)
            }),
            ("revokes", |id, sid| UserUpdateRequest {
                revokes: Some(vec!["chat_send".to_string()]),
                ..empty_self_edit_request(id, sid)
            }),
            ("remove_group=true", |id, sid| UserUpdateRequest {
                remove_group: Some(true),
                ..empty_self_edit_request(id, sid)
            }),
        ];

        for (label, builder) in cases {
            eprintln!("self-edit non-admin forbidden case: {}", label);
            assert_self_edit_outcome(
                false, // requesting_is_admin
                false, // requesting_is_shared
                builder,
                SelfEditOutcome::Error(err_cannot_edit_self(DEFAULT_TEST_LOCALE)),
            )
            .await;
        }
    }

    /// Gate checkpoint 3: admin self-edit setting `group_id` is rejected by
    /// the admin XOR group invariant, with a distinct error (different from
    /// the generic `err_cannot_edit_self`).
    #[tokio::test]
    async fn test_self_edit_admin_group_id_rejected() {
        assert_self_edit_outcome(
            true,  // requesting_is_admin
            false, // requesting_is_shared
            |id, sid| UserUpdateRequest {
                group_id: Some(1),
                ..empty_self_edit_request(id, sid)
            },
            SelfEditOutcome::Error(err_admin_cannot_have_group(DEFAULT_TEST_LOCALE)),
        )
        .await;
    }

    /// Gate checkpoint 4: non-admin self-edit cannot change username,
    /// group_id, or either bandwidth_weight field. (group_id falls through
    /// the admin XOR group gate above and lands here with a generic error.)
    #[tokio::test]
    async fn test_self_edit_non_admin_restricted_fields_rejected() {
        let cases: Vec<(&str, SelfEditRequestBuilder)> = vec![
            ("username", |id, sid| UserUpdateRequest {
                username: Some("newalice".to_string()),
                ..empty_self_edit_request(id, sid)
            }),
            ("group_id", |id, sid| UserUpdateRequest {
                group_id: Some(1),
                ..empty_self_edit_request(id, sid)
            }),
            ("bandwidth_weight", |id, sid| UserUpdateRequest {
                bandwidth_weight: Some(50),
                ..empty_self_edit_request(id, sid)
            }),
            ("inherit_bandwidth_weight", |id, sid| UserUpdateRequest {
                inherit_bandwidth_weight: Some(true),
                ..empty_self_edit_request(id, sid)
            }),
        ];

        for (label, builder) in cases {
            eprintln!("self-edit non-admin restricted case: {}", label);
            assert_self_edit_outcome(
                false, // requesting_is_admin
                false, // requesting_is_shared
                builder,
                SelfEditOutcome::Error(err_cannot_edit_self(DEFAULT_TEST_LOCALE)),
            )
            .await;
        }
    }

    /// The permitted face of the gate: what each caller class can actually
    /// change about themselves. Admin can change username, bandwidth_weight,
    /// inherit_bandwidth_weight, and password. Non-admin can change password.
    #[tokio::test]
    async fn test_self_edit_allowed_fields_succeed() {
        // Admin self-edit: username
        assert_self_edit_outcome(
            true,
            false,
            |id, sid| UserUpdateRequest {
                username: Some("newalice".to_string()),
                ..empty_self_edit_request(id, sid)
            },
            SelfEditOutcome::Success,
        )
        .await;

        // Admin self-edit: bandwidth_weight
        assert_self_edit_outcome(
            true,
            false,
            |id, sid| UserUpdateRequest {
                bandwidth_weight: Some(75),
                ..empty_self_edit_request(id, sid)
            },
            SelfEditOutcome::Success,
        )
        .await;

        // Admin self-edit: inherit_bandwidth_weight
        assert_self_edit_outcome(
            true,
            false,
            |id, sid| UserUpdateRequest {
                inherit_bandwidth_weight: Some(true),
                ..empty_self_edit_request(id, sid)
            },
            SelfEditOutcome::Success,
        )
        .await;

        // Admin self-edit: password (with correct current_password)
        assert_self_edit_outcome(
            true,
            false,
            |id, sid| UserUpdateRequest {
                current_password: Some("password".to_string()),
                password: Some("newpassword".to_string()),
                ..empty_self_edit_request(id, sid)
            },
            SelfEditOutcome::Success,
        )
        .await;

        // Non-admin self-edit: password (with correct current_password)
        assert_self_edit_outcome(
            false,
            false,
            |id, sid| UserUpdateRequest {
                current_password: Some("password".to_string()),
                password: Some("newpassword".to_string()),
                ..empty_self_edit_request(id, sid)
            },
            SelfEditOutcome::Success,
        )
        .await;
    }

    // ========================================================================
    // No-op broadcast suppression: a bandwidth-only update should not
    // emit `UserUpdated` when the user's *effective* weight didn't
    // actually move. The check compares the pre-update cached resolved
    // value (as `Option<u16>`) against the post-update resolved value
    // returned by `update_user`. Offline users (`None`) always broadcast
    // because `None != Some(_)` — matching how the handler treats
    // offline users for every other trigger. Cross-trigger updates
    // (bw + username, bw + group, etc.) bypass suppression because the
    // non-bandwidth field carries visible info on its own.
    // ========================================================================

    /// Bandwidth-only update to an OFFLINE user → broadcast still fires.
    /// `old_resolved` is `None` for an offline target, which fails
    /// equality against any `Some(_)` resolved value, ensuring an
    /// offline user who logs in concurrently (or a UserList holder that
    /// tracks offline users) converges on the post-update value.
    /// Consistent with how non-bandwidth triggers treat offline users.
    #[tokio::test]
    async fn test_userupdate_bandwidth_only_offline_broadcasts() {
        let mut test_ctx = create_test_context().await;

        let _admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Bob exists in the DB but has NO active session — offline.
        let bob = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: &get_cached_password_hash("password"),
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &db::Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        let admin_session = test_ctx
            .user_manager
            .get_sessions_by_username("admin")
            .await
            .first()
            .map(|s| s.session_id);
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
            revokes: None,
            bandwidth_weight: Some(200),
            inherit_bandwidth_weight: None,
            session_id: admin_session,
        };
        handle_user_update(request, &mut test_ctx.handler_context())
            .await
            .unwrap();

        // First message: UserUpdateResponse to admin.
        let _response = read_server_message(&mut test_ctx).await;

        let mut saw_broadcast = false;
        while let Ok((msg, _)) = test_ctx.rx.try_recv() {
            if let ServerMessage::UserUpdated { user, .. } = msg
                && user.username == "bob"
            {
                assert_eq!(
                    user.bandwidth_weight,
                    Some(200),
                    "broadcast must carry the post-update resolved weight"
                );
                saw_broadcast = true;
            }
        }
        assert!(
            saw_broadcast,
            "offline-user bw-only update must still broadcast UserUpdated \
             (None ≠ any Some(_) resolved value)"
        );
    }

    /// Bandwidth-only update where the resolved value doesn't move →
    /// no broadcast. Constructed using a non-admin user with no group
    /// (resolved = DEFAULT_BANDWIDTH_WEIGHT) and writing an override
    /// equal to that default. The DB row changes (NULL → Some(default)),
    /// so `bandwidth_weight_request_change` is true, but resolved stays
    /// at DEFAULT_BANDWIDTH_WEIGHT (override wins per the resolver but
    /// the override value IS the default). The cached session value
    /// matches the new resolved → suppression fires.
    #[tokio::test]
    async fn test_userupdate_bandwidth_only_same_resolved_suppresses_broadcast() {
        let mut test_ctx = create_test_context().await;

        let _admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Bob: non-admin, no group, no override. Resolved = DEFAULT_BANDWIDTH_WEIGHT.
        let mut bob_db_perms = db::Permissions::new();
        bob_db_perms.permissions.insert(db::Permission::UserList);
        let bob = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: &get_cached_password_hash("password"),
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &bob_db_perms,
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        let mut bob_session_perms = HashSet::new();
        bob_session_perms.insert(db::Permission::UserList);
        let _bob_session = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                user_id: bob.id,
                username: "bob".to_string(),
                is_admin: false,
                is_shared: false,
                permissions: bob_session_perms,
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
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
                last_activity: std::time::Instant::now(),
            })
            .await
            .unwrap();

        // Write an override equal to the existing resolved value. DB row
        // changes (NULL → Some(DEFAULT)) so the dirty bit fires, but
        // resolved stays at DEFAULT_BANDWIDTH_WEIGHT — exactly the value
        // already in bob's session cache. Suppression must fire.
        let admin_session = test_ctx
            .user_manager
            .get_sessions_by_username("admin")
            .await
            .first()
            .map(|s| s.session_id);
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
            revokes: None,
            bandwidth_weight: Some(nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT),
            inherit_bandwidth_weight: None,
            session_id: admin_session,
        };
        handle_user_update(request, &mut test_ctx.handler_context())
            .await
            .unwrap();

        let _response = read_server_message(&mut test_ctx).await;

        let mut user_updated_count = 0;
        let mut other_msgs = Vec::new();
        while let Ok((msg, _)) = test_ctx.rx.try_recv() {
            match msg {
                ServerMessage::UserUpdated { user, .. } if user.username == "bob" => {
                    user_updated_count += 1;
                }
                other => other_msgs.push(format!("{:?}", other)),
            }
        }
        assert_eq!(
            user_updated_count, 0,
            "bw-only update where resolved value doesn't move must suppress the broadcast. \
             other msgs: {:?}",
            other_msgs
        );
    }

    /// Suppression must NOT fire when a non-bandwidth field also
    /// changed. Same "bw side is a no-op" setup as the same-resolved
    /// suppression test (non-admin, override=DEFAULT, resolved
    /// unchanged), but combined with a username rename. Username is a
    /// UserInfo-visible change, so the broadcast must fire regardless
    /// of whether the bandwidth side suppressed on its own.
    #[tokio::test]
    async fn test_userupdate_combined_change_bypasses_l2_suppression() {
        let mut test_ctx = create_test_context().await;

        let _admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let mut bob_db_perms = db::Permissions::new();
        bob_db_perms.permissions.insert(db::Permission::UserList);
        let bob = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: &get_cached_password_hash("password"),
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &bob_db_perms,
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        let mut bob_session_perms = HashSet::new();
        bob_session_perms.insert(db::Permission::UserList);
        let _bob_session = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                user_id: bob.id,
                username: "bob".to_string(),
                is_admin: false,
                is_shared: false,
                permissions: bob_session_perms,
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
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
                last_activity: std::time::Instant::now(),
            })
            .await
            .unwrap();

        // Combined: rename + bandwidth override that resolves to the
        // same value. Without the username change suppression would
        // fire; with it, the broadcast must still fire.
        let admin_session = test_ctx
            .user_manager
            .get_sessions_by_username("admin")
            .await
            .first()
            .map(|s| s.session_id);
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
            bandwidth_weight: Some(nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT),
            inherit_bandwidth_weight: None,
            session_id: admin_session,
        };
        handle_user_update(request, &mut test_ctx.handler_context())
            .await
            .unwrap();

        let _response = read_server_message(&mut test_ctx).await;

        let mut saw_broadcast_for_robert = false;
        while let Ok((msg, _)) = test_ctx.rx.try_recv() {
            if let ServerMessage::UserUpdated { user, .. } = msg
                && user.username == "robert"
            {
                saw_broadcast_for_robert = true;
            }
        }
        assert!(
            saw_broadcast_for_robert,
            "cross-trigger update (username + bandwidth) must bypass suppression and broadcast"
        );
    }
}
