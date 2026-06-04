//! UserUpdate message handler

use std::io;
use std::sync::atomic::Ordering;

use tokio::io::AsyncWrite;
use tracing::{error, info, warn};

use nexus_common::is_shared_account_permission;
use nexus_common::names::fold_name;
use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{
    self, BandwidthWeightError, MIN_BANDWIDTH_WEIGHT, PasswordError, PermissionsError,
    UsernameError, validate_bandwidth_weight,
};

use crate::constants::{
    HANDLER_USER_UPDATE, LOG_USER_UPDATE_ADMIN, LOG_USER_UPDATE_DB_ERROR,
    LOG_USER_UPDATE_DB_ERROR_DUPLICATE_CHECK, LOG_USER_UPDATE_DB_ERROR_GROUP,
    LOG_USER_UPDATE_DB_ERROR_GROUP_PERMS, LOG_USER_UPDATE_DB_ERROR_LOOKUP,
    LOG_USER_UPDATE_DB_ERROR_PERMISSIONS, LOG_USER_UPDATE_DB_ERROR_TARGET,
    LOG_USER_UPDATE_DB_ERROR_USER, LOG_USER_UPDATE_FILE_AREA_BUSY,
    LOG_USER_UPDATE_FILE_AREA_MIGRATE_FAILED, LOG_USER_UPDATE_FILE_AREA_MIGRATED,
    LOG_USER_UPDATE_FILE_AREA_ROLLBACK_FAILED, LOG_USER_UPDATE_FILE_AREA_TARGET_EXISTS,
    LOG_USER_UPDATE_HASH_ERROR, LOG_USER_UPDATE_NOT_LOGGED_IN, LOG_USER_UPDATE_PASSWORD_VERIFY,
    LOG_USER_UPDATE_PERMISSION_DENIED, LOG_USER_UPDATE_SUCCESS, LOG_USER_UPDATE_UNOWNED_PERMISSION,
    LOG_USER_UPDATE_UNOWNED_REVOKE,
};

#[cfg(test)]
use super::DirectWriter;
#[cfg(test)]
use super::testing::DEFAULT_TEST_LOCALE;
use super::{
    HandlerContext, Outcome, dispatch_outcome, err_account_disabled_by_admin,
    err_admin_cannot_have_group, err_bandwidth_weight_delegation,
    err_bandwidth_weight_inherit_would_elevate, err_bandwidth_weight_zero,
    err_cannot_change_guest_password, err_cannot_demote_last_admin, err_cannot_disable_last_admin,
    err_cannot_edit_admin, err_cannot_edit_self, err_cannot_rename_guest,
    err_current_password_incorrect, err_current_password_required, err_database,
    err_group_not_found, err_group_shared_mismatch, err_internal_error, err_not_logged_in,
    err_password_empty, err_password_too_long, err_password_too_weak, err_permission_denied,
    err_permission_grant_revoke_conflict, err_permissions_contains_newlines,
    err_permissions_empty_permission, err_permissions_invalid_characters,
    err_permissions_permission_too_long, err_permissions_too_many, err_personal_file_area_busy,
    err_personal_file_area_exists, err_personal_file_area_migration_failed,
    err_personal_file_area_rollback_failed_warning, err_shared_cannot_be_admin,
    err_shared_cannot_self_edit, err_shared_invalid_permissions, err_unknown_permission,
    err_update_failed, err_user_not_found, err_username_empty, err_username_exists,
    err_username_invalid, err_username_is_active_nickname, err_username_too_long,
    remove_users_with_cleanup_locked, send_reason_and_disconnect, update_egress_user_weight,
};
use super::{
    ServerInfoOptions, ServerInfoValues, broadcast_chat_user_renamed,
    broadcast_user_updated_for_members, build_server_info,
};
#[cfg(test)]
use crate::db::hash_password;
use crate::db::sql::GUEST_USERNAME;
use crate::db::{
    Permission, Permissions, UpdateUserParams, hash_password_async, verify_password_async,
};
use crate::files::{
    UserAreaMigration, UserAreaMigrationError, migrate_user_area_on_username_change,
};
use crate::users::manager::UserManager;
use crate::voice::send_voice_leave_notifications;

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

pub async fn handle_user_update<W>(
    request: UserUpdateRequest,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let Some(requesting_session_id) = request.session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_USER_UPDATE_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_USER_UPDATE))
            .await;
    };

    // Guard lives only inside this block; all socket sends happen after it.
    let outcome = 'locked: {
        let _state_guard = ctx.user_manager.lock_user_state().await;
        let requesting_user = match ctx
            .user_manager
            .get_user_by_session_id(requesting_session_id)
            .await
        {
            Some(u) => u,
            None => {
                break 'locked Outcome::Disconnect;
            }
        };

        let target_account = match ctx.db.users.get_user_by_id(request.id).await {
            Ok(Some(account)) => account,
            Ok(None) => {
                break 'locked Outcome::Send(Box::new(ServerMessage::UserUpdateResponse {
                    success: false,
                    error: Some(err_user_not_found(ctx.locale, &request.id.to_string())),
                    id: None,
                    username: None,
                }));
            }
            Err(e) => {
                error!(user = %requesting_user.username, ip = %ctx.peer_addr, target = %request.id, err = %e, "{}", LOG_USER_UPDATE_DB_ERROR_LOOKUP);
                break 'locked Outcome::Send(Box::new(ServerMessage::UserUpdateResponse {
                    success: false,
                    error: Some(err_database(ctx.locale)),
                    id: None,
                    username: None,
                }));
            }
        };
        let target_username = target_account.username.clone();

        if let Err(e) = validators::validate_username(&target_username) {
            let error_msg = match e {
                UsernameError::Empty => err_username_empty(ctx.locale),
                UsernameError::TooLong => {
                    err_username_too_long(ctx.locale, validators::MAX_USERNAME_LENGTH)
                }
                UsernameError::InvalidCharacters => err_username_invalid(ctx.locale),
            };
            break 'locked Outcome::Send(Box::new(ServerMessage::UserUpdateResponse {
                success: false,
                error: Some(error_msg),
                id: None,
                username: None,
            }));
        }

        // Self-edit allows a more restrictive field set than editing others;
        // drives the password / shared-account / forbidden-field branches below.
        let is_self_edit = fold_name(&target_username) == fold_name(&requesting_user.username);

        if is_self_edit {
            // Shared accounts have no password and no admissible self-edit fields.
            if requesting_user.is_shared {
                break 'locked Outcome::Send(Box::new(ServerMessage::UserUpdateResponse {
                    success: false,
                    error: Some(err_shared_cannot_self_edit(ctx.locale)),
                    id: None,
                    username: None,
                }));
            }

            // Defense in depth: these fields are never accepted on self-edit,
            // even from an admin. Client UI hard-disables them on self-rows.
            if request.is_admin.is_some()
                || request.enabled.is_some()
                || request.permissions.is_some()
                || request.revokes.is_some()
                || request.remove_group == Some(true)
            {
                break 'locked Outcome::Send(Box::new(ServerMessage::UserUpdateResponse {
                    success: false,
                    error: Some(err_cannot_edit_self(ctx.locale)),
                    id: None,
                    username: None,
                }));
            }

            // Admin self-edit: group_id violates admin XOR group. Non-admin
            // group_id is caught below with a different error.
            if requesting_user.is_admin && request.group_id.is_some() {
                break 'locked Outcome::Send(Box::new(ServerMessage::UserUpdateResponse {
                    success: false,
                    error: Some(err_admin_cannot_have_group(ctx.locale)),
                    id: None,
                    username: None,
                }));
            }

            // Non-admin self-edit allows only password; admins additionally
            // allow username and bandwidth-weight fields.
            if !requesting_user.is_admin
                && (request.username.is_some()
                    || request.group_id.is_some()
                    || request.bandwidth_weight.is_some()
                    || request.inherit_bandwidth_weight.is_some())
            {
                break 'locked Outcome::Send(Box::new(ServerMessage::UserUpdateResponse {
                    success: false,
                    error: Some(err_cannot_edit_self(ctx.locale)),
                    id: None,
                    username: None,
                }));
            }

            // Password change requires current_password verification.
            if let Some(ref new_password) = request.password
                && !new_password.trim().is_empty()
            {
                let Some(ref current_password) = request.current_password else {
                    break 'locked Outcome::Send(Box::new(ServerMessage::UserUpdateResponse {
                        success: false,
                        error: Some(err_current_password_required(ctx.locale)),
                        id: None,
                        username: None,
                    }));
                };

                let password_hash = match ctx.db.users.get_user_by_username(&target_username).await
                {
                    Ok(Some(user)) => user.hashed_password,
                    Ok(None) => {
                        break 'locked Outcome::Send(Box::new(ServerMessage::UserUpdateResponse {
                            success: false,
                            error: Some(err_user_not_found(ctx.locale, &target_username)),
                            id: None,
                            username: None,
                        }));
                    }
                    Err(e) => {
                        error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_USER_UPDATE_DB_ERROR_USER);
                        break 'locked Outcome::Send(Box::new(ServerMessage::UserUpdateResponse {
                            success: false,
                            error: Some(err_database(ctx.locale)),
                            id: None,
                            username: None,
                        }));
                    }
                };

                match verify_password_async(current_password.to_string(), password_hash.clone())
                    .await
                {
                    Ok(true) => {}
                    Ok(false) => {
                        break 'locked Outcome::Send(Box::new(ServerMessage::UserUpdateResponse {
                            success: false,
                            error: Some(err_current_password_incorrect(ctx.locale)),
                            id: None,
                            username: None,
                        }));
                    }
                    Err(e) => {
                        // Argon2id failure isn't a protocol violation.
                        error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_USER_UPDATE_PASSWORD_VERIFY);
                        break 'locked Outcome::Send(Box::new(ServerMessage::UserUpdateResponse {
                            success: false,
                            error: Some(err_internal_error(ctx.locale)),
                            id: None,
                            username: None,
                        }));
                    }
                }
            }
        } else {
            if !requesting_user.has_permission(Permission::UserEdit) {
                warn!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_USER_UPDATE_PERMISSION_DENIED);
                break 'locked Outcome::Send(Box::new(ServerMessage::UserUpdateResponse {
                    success: false,
                    error: Some(err_permission_denied(ctx.locale)),
                    id: None,
                    username: None,
                }));
            }

            // Non-admins cannot edit admin users.
            if !requesting_user.is_admin {
                match ctx.db.users.get_user_by_username(&target_username).await {
                    Ok(Some(target_user)) if target_user.is_admin => {
                        warn!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_USER_UPDATE_ADMIN);
                        break 'locked Outcome::Send(Box::new(ServerMessage::UserUpdateResponse {
                            success: false,
                            error: Some(err_cannot_edit_admin(ctx.locale)),
                            id: None,
                            username: None,
                        }));
                    }
                    Ok(Some(_)) => {} // Target is not admin, proceed
                    Ok(None) => {
                        break 'locked Outcome::Send(Box::new(ServerMessage::UserUpdateResponse {
                            success: false,
                            error: Some(err_user_not_found(ctx.locale, &target_username)),
                            id: None,
                            username: None,
                        }));
                    }
                    Err(e) => {
                        error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_USER_UPDATE_DB_ERROR_TARGET);
                        break 'locked Outcome::Send(Box::new(ServerMessage::UserUpdateResponse {
                            success: false,
                            error: Some(err_database(ctx.locale)),
                            id: None,
                            username: None,
                        }));
                    }
                }
            }
        }

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
            break 'locked Outcome::Send(Box::new(ServerMessage::UserUpdateResponse {
                success: false,
                error: Some(error_msg),
                id: None,
                username: None,
            }));
        }

        // The guest account cannot be renamed.
        if let Some(ref new_username) = request.username
            && fold_name(&target_username) == GUEST_USERNAME
            && fold_name(new_username) != GUEST_USERNAME
        {
            break 'locked Outcome::Send(Box::new(ServerMessage::UserUpdateResponse {
                success: false,
                error: Some(err_cannot_rename_guest(ctx.locale)),
                id: None,
                username: None,
            }));
        }

        // A username can't take a nickname an active session already holds (they
        // share one namespace; login enforces the inverse). Gated on a real
        // case-insensitive change so a no-op resubmit isn't self-rejected.
        if let Some(ref new_username) = request.username
            && fold_name(&target_username) != fold_name(new_username)
            && ctx.user_manager.is_nickname_in_use(new_username).await
        {
            break 'locked Outcome::Send(Box::new(ServerMessage::UserUpdateResponse {
                success: false,
                error: Some(err_username_is_active_nickname(ctx.locale)),
                id: None,
                username: None,
            }));
        }

        // The guest account password cannot be changed.
        if let Some(ref new_password) = request.password
            && !new_password.trim().is_empty()
            && fold_name(&target_username) == GUEST_USERNAME
        {
            break 'locked Outcome::Send(Box::new(ServerMessage::UserUpdateResponse {
                success: false,
                error: Some(err_cannot_change_guest_password(ctx.locale)),
                id: None,
                username: None,
            }));
        }

        // Last-admin protection is enforced atomically in update_user()'s SQL.

        // Only admins may change a user's admin flag (self-edit already rejected).
        if !is_self_edit && request.is_admin.is_some() && !requesting_user.is_admin {
            break 'locked Outcome::Send(Box::new(ServerMessage::UserUpdateResponse {
                success: false,
                error: Some(err_permission_denied(ctx.locale)),
                id: None,
                username: None,
            }));
        }

        // Needed for shared-account permission validation below.
        let target_user_account = match ctx.db.users.get_user_by_username(&target_username).await {
            Ok(Some(account)) => Some(account),
            Ok(None) => {
                break 'locked Outcome::Send(Box::new(ServerMessage::UserUpdateResponse {
                    success: false,
                    error: Some(err_user_not_found(ctx.locale, &target_username)),
                    id: None,
                    username: None,
                }));
            }
            Err(e) => {
                error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_USER_UPDATE_DB_ERROR_TARGET);
                break 'locked Outcome::Send(Box::new(ServerMessage::UserUpdateResponse {
                    success: false,
                    error: Some(err_database(ctx.locale)),
                    id: None,
                    username: None,
                }));
            }
        };

        // Admin XOR group: reject ending up admin AND setting group_id. Plain
        // promotion is fine (DB auto-clears group_id); only the coincidence rejects.
        let target_final_is_admin = request
            .is_admin
            .unwrap_or_else(|| target_user_account.as_ref().is_some_and(|a| a.is_admin));
        if target_final_is_admin && request.group_id.is_some() {
            break 'locked Outcome::Send(Box::new(ServerMessage::UserUpdateResponse {
                success: false,
                error: Some(err_admin_cannot_have_group(ctx.locale)),
                id: None,
                username: None,
            }));
        }

        // Admin XOR shared: `is_shared` is create-time only, so the sole bad path
        // is promoting a shared account to admin. Unlike admin XOR group (which
        // auto-cleans, since clearing a group is benign for an admin), we reject
        // here — clearing `is_shared` would orphan per-session nicknames, so the
        // admin must explicitly delete and recreate rather than lose that identity.
        if request.is_admin == Some(true)
            && target_user_account.as_ref().is_some_and(|a| a.is_shared)
        {
            break 'locked Outcome::Send(Box::new(ServerMessage::UserUpdateResponse {
                success: false,
                error: Some(err_shared_cannot_be_admin(ctx.locale)),
                id: None,
                username: None,
            }));
        }

        // Bandwidth delegation (admins bypass): a non-admin can only set a user's
        // effective weight at or below their own. `Some(N)` rejects when N > requester;
        // `inherit: Some(true)` rejects when the target's inherited weight > requester
        // (clearing the override would let them fall back to a higher tier).
        // Inherit wins over `bandwidth_weight` in the DB, so skip the value check when
        // inherit is set (a defensive client sending both isn't rejected on a moot value).
        if !requesting_user.is_admin {
            let requester_weight = requesting_user.bandwidth_weight.load(Ordering::Relaxed);
            // The set and inherit paths use distinct i18n keys.
            let mut delegation_error: Option<String> = None;
            if request.inherit_bandwidth_weight != Some(true)
                && let Some(w) = request.bandwidth_weight
                && w > requester_weight
            {
                delegation_error = Some(err_bandwidth_weight_delegation(ctx.locale));
            }
            if delegation_error.is_none() && request.inherit_bandwidth_weight == Some(true) {
                // Resolve against the POST-update group: a concurrent group_id /
                // remove_group change makes the old group's weight irrelevant.
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
                        break 'locked Outcome::Send(Box::new(ServerMessage::UserUpdateResponse {
                            success: false,
                            error: Some(err_database(ctx.locale)),
                            id: None,
                            username: None,
                        }));
                    }
                };
                if inherited > requester_weight {
                    delegation_error = Some(err_bandwidth_weight_inherit_would_elevate(ctx.locale));
                }
            }
            if let Some(error) = delegation_error {
                break 'locked Outcome::Send(Box::new(ServerMessage::UserUpdateResponse {
                    success: false,
                    error: Some(error),
                    id: None,
                    username: None,
                }));
            }
        }
        // Skip zero-validation too when inherit takes precedence (value discarded).
        if request.inherit_bandwidth_weight != Some(true)
            && let Some(w) = request.bandwidth_weight
            && let Err(BandwidthWeightError::Zero) = validate_bandwidth_weight(w)
        {
            break 'locked Outcome::Send(Box::new(ServerMessage::UserUpdateResponse {
                success: false,
                error: Some(err_bandwidth_weight_zero(ctx.locale, MIN_BANDWIDTH_WEIGHT)),
                id: None,
                username: None,
            }));
        }

        let parsed_permissions = if let Some(ref perm_strings) = request.permissions {
            // Shared accounts accept only shared-allowed permissions.
            if let Some(ref account) = target_user_account
                && account.is_shared
            {
                let forbidden: Vec<&str> = perm_strings
                    .iter()
                    .map(|s| s.as_str())
                    .filter(|p| !is_shared_account_permission(p))
                    .collect();

                if !forbidden.is_empty() {
                    break 'locked Outcome::Send(Box::new(ServerMessage::UserUpdateResponse {
                        success: false,
                        error: Some(err_shared_invalid_permissions(
                            ctx.locale,
                            &forbidden.join(", "),
                        )),
                        id: None,
                        username: None,
                    }));
                }
            }

            if let Err(e) = validators::validate_permissions(perm_strings) {
                let error_msg = match e {
                    PermissionsError::TooMany => {
                        err_permissions_too_many(ctx.locale, nexus_common::PERMISSIONS_COUNT)
                    }
                    PermissionsError::EmptyPermission => {
                        err_permissions_empty_permission(ctx.locale)
                    }
                    PermissionsError::PermissionTooLong => err_permissions_permission_too_long(
                        ctx.locale,
                        validators::MAX_PERMISSION_LENGTH,
                    ),
                    PermissionsError::ContainsNewlines => {
                        err_permissions_contains_newlines(ctx.locale)
                    }
                    PermissionsError::InvalidCharacters => {
                        err_permissions_invalid_characters(ctx.locale)
                    }
                };
                break 'locked Outcome::Send(Box::new(ServerMessage::UserUpdateResponse {
                    success: false,
                    error: Some(error_msg),
                    id: None,
                    username: None,
                }));
            }

            let mut perms = Permissions::new();
            for perm_str in perm_strings {
                let perm = match Permission::parse(perm_str) {
                    Some(p) => p,
                    None => {
                        break 'locked Outcome::Send(Box::new(ServerMessage::UserUpdateResponse {
                            success: false,
                            error: Some(err_unknown_permission(ctx.locale, perm_str)),
                            id: None,
                            username: None,
                        }));
                    }
                };

                // Delegation: requester can only grant permissions they hold.
                if !requesting_user.has_permission(perm) {
                    warn!(user = %requesting_user.username, ip = %ctx.peer_addr, perm = %perm_str, "{}", LOG_USER_UPDATE_UNOWNED_PERMISSION);
                    break 'locked Outcome::Send(Box::new(ServerMessage::UserUpdateResponse {
                        success: false,
                        error: Some(err_permission_denied(ctx.locale)),
                        id: None,
                        username: None,
                    }));
                }

                perms.permissions.insert(perm);
            }

            // No pre-tx merge: `update_user`'s `OwnedSubset` path only touches rows
            // for the requester's owned permissions, so unowned rows (incl. ones an
            // admin just granted) survive. A snapshot-merge would race admin writes.
            Some(perms)
        } else {
            None
        };

        // DB writes happen atomically inside update_user's transaction.
        let (validated_remove_group, validated_group_id): (bool, Option<i64>) = if !is_self_edit {
            if request.remove_group == Some(true) {
                // remove_group takes precedence over group_id.
                if let Some(ref account) = target_user_account {
                    if let Some(current_group_id) = account.group_id {
                        // Delegation: requester must hold all current group permissions
                        // (removal changes effective perms the editor can't grant back).
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
                                    break 'locked Outcome::Send(Box::new(
                                        ServerMessage::UserUpdateResponse {
                                            success: false,
                                            error: Some(err_database(ctx.locale)),
                                            id: None,
                                            username: None,
                                        },
                                    ));
                                }
                            };
                            for perm in &group_perms {
                                if !requesting_user.has_permission(*perm) {
                                    break 'locked Outcome::Send(Box::new(
                                        ServerMessage::UserUpdateResponse {
                                            success: false,
                                            error: Some(err_permission_denied(ctx.locale)),
                                            id: None,
                                            username: None,
                                        },
                                    ));
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
                    if account.group_id == Some(new_group_id) {
                        (false, None) // Already in this group
                    } else {
                        // Delegation: requester must hold all current group permissions
                        // (moving away removes them, same check as remove_group).
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
                                    break 'locked Outcome::Send(Box::new(
                                        ServerMessage::UserUpdateResponse {
                                            success: false,
                                            error: Some(err_database(ctx.locale)),
                                            id: None,
                                            username: None,
                                        },
                                    ));
                                }
                            };
                            for perm in &old_group_perms {
                                if !requesting_user.has_permission(*perm) {
                                    break 'locked Outcome::Send(Box::new(
                                        ServerMessage::UserUpdateResponse {
                                            success: false,
                                            error: Some(err_permission_denied(ctx.locale)),
                                            id: None,
                                            username: None,
                                        },
                                    ));
                                }
                            }
                        }

                        let group = match ctx.db.groups.get_group_by_id(new_group_id).await {
                            Ok(Some(g)) => g,
                            Ok(None) => {
                                break 'locked Outcome::Send(Box::new(
                                    ServerMessage::UserUpdateResponse {
                                        success: false,
                                        error: Some(err_group_not_found(ctx.locale)),
                                        id: None,
                                        username: None,
                                    },
                                ));
                            }
                            Err(e) => {
                                error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_USER_UPDATE_DB_ERROR_GROUP);
                                break 'locked Outcome::Send(Box::new(
                                    ServerMessage::UserUpdateResponse {
                                        success: false,
                                        error: Some(err_database(ctx.locale)),
                                        id: None,
                                        username: None,
                                    },
                                ));
                            }
                        };

                        // Shared compatibility check
                        if account.is_shared && !group.is_shared {
                            break 'locked Outcome::Send(Box::new(
                                ServerMessage::UserUpdateResponse {
                                    success: false,
                                    error: Some(err_group_shared_mismatch(ctx.locale)),
                                    id: None,
                                    username: None,
                                },
                            ));
                        }
                        if !account.is_shared && group.is_shared {
                            break 'locked Outcome::Send(Box::new(
                                ServerMessage::UserUpdateResponse {
                                    success: false,
                                    error: Some(err_group_shared_mismatch(ctx.locale)),
                                    id: None,
                                    username: None,
                                },
                            ));
                        }

                        // Delegation: can't promote into a group whose weight exceeds
                        // the requester's — blocks self/other escalation into a
                        // higher-weight group whose permissions they happen to hold.
                        if !requesting_user.is_admin
                            && group.bandwidth_weight
                                > requesting_user.bandwidth_weight.load(Ordering::Relaxed)
                        {
                            break 'locked Outcome::Send(Box::new(
                                ServerMessage::UserUpdateResponse {
                                    success: false,
                                    error: Some(err_bandwidth_weight_delegation(ctx.locale)),
                                    id: None,
                                    username: None,
                                },
                            ));
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
                                    break 'locked Outcome::Send(Box::new(
                                        ServerMessage::UserUpdateResponse {
                                            success: false,
                                            error: Some(err_database(ctx.locale)),
                                            id: None,
                                            username: None,
                                        },
                                    ));
                                }
                            };
                            for perm in &group_perms {
                                if !requesting_user.has_permission(*perm) {
                                    break 'locked Outcome::Send(Box::new(
                                        ServerMessage::UserUpdateResponse {
                                            success: false,
                                            error: Some(err_permission_denied(ctx.locale)),
                                            id: None,
                                            username: None,
                                        },
                                    ));
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

        // Parsed here; DB write happens atomically in update_user's transaction.
        let parsed_revokes: Option<Vec<Permission>> = if !is_self_edit
            && let Some(ref revoke_strings) = request.revokes
            && let Some(ref account) = target_user_account
        {
            // Effective group_id after any group change in this request.
            let effective_group_id = if request.remove_group == Some(true) {
                None
            } else if let Some(gid) = request.group_id {
                Some(gid)
            } else {
                account.group_id
            };

            if effective_group_id.is_some() {
                let mut parsed_revokes = Vec::new();
                for perm_str in revoke_strings {
                    match Permission::parse(perm_str) {
                        Some(perm) => {
                            // Delegation: non-admins can only revoke permissions they hold.
                            if !requesting_user.is_admin && !requesting_user.has_permission(perm) {
                                warn!(user = %requesting_user.username, ip = %ctx.peer_addr, perm = %perm_str, "{}", LOG_USER_UPDATE_UNOWNED_REVOKE);
                                break 'locked Outcome::Send(Box::new(
                                    ServerMessage::UserUpdateResponse {
                                        success: false,
                                        error: Some(err_permission_denied(ctx.locale)),
                                        id: None,
                                        username: None,
                                    },
                                ));
                            }
                            parsed_revokes.push(perm);
                        }
                        None => {
                            break 'locked Outcome::Send(Box::new(
                                ServerMessage::UserUpdateResponse {
                                    success: false,
                                    error: Some(err_unknown_permission(ctx.locale, perm_str)),
                                    id: None,
                                    username: None,
                                },
                            ));
                        }
                    }
                }

                // No pre-tx merge — same as grants above: `OwnedSubset` only touches
                // revoke rows for owned permissions, so unowned revokes survive.
                Some(parsed_revokes)
            } else {
                None
            }
        } else {
            None
        };

        let requested_password_hash = if let Some(ref password) = request.password {
            // Empty/whitespace password = no change.
            if password.trim().is_empty() {
                None
            } else {
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
                    break 'locked Outcome::Send(Box::new(ServerMessage::UserUpdateResponse {
                        success: false,
                        error: Some(error_msg),
                        id: None,
                        username: None,
                    }));
                }
                // Argon2id failure isn't a protocol violation.
                match hash_password_async(password.clone(), min_strength, false).await {
                    Ok(hash) => Some(hash),
                    Err(e) => {
                        error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_USER_UPDATE_HASH_ERROR);
                        break 'locked Outcome::Send(Box::new(ServerMessage::UserUpdateResponse {
                            success: false,
                            error: Some(err_internal_error(ctx.locale)),
                            id: None,
                            username: None,
                        }));
                    }
                }
            }
        } else {
            None
        };

        // Capture old state BEFORE the update so the post-update diff drives the
        // PermissionsUpdated / UserUpdated cascade. A DB read failure must NOT fall
        // back to empty perms — that would skip the cascade and any voice cleanup.
        let (old_username, old_is_admin, old_enabled, old_permissions) = {
            if let Some(ref account) = target_user_account {
                let perms = match ctx.db.users.get_user_permissions(account.id).await {
                    Ok(p) => p,
                    Err(e) => {
                        error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_USER_UPDATE_DB_ERROR_PERMISSIONS);
                        break 'locked Outcome::Send(Box::new(ServerMessage::UserUpdateResponse {
                            success: false,
                            error: Some(err_database(ctx.locale)),
                            id: None,
                            username: None,
                        }));
                    }
                };
                (
                    account.username.clone(),
                    account.is_admin,
                    account.enabled,
                    perms,
                )
            } else {
                // Pre-checked above; defensive fallback only.
                (target_username.clone(), false, true, Permissions::new())
            }
        };

        // The `(user_id, permission)` PK allows one row per permission, so naming
        // the same permission as grant and revoke is ambiguous — fail upfront.
        if let (Some(grants), Some(revokes)) =
            (parsed_permissions.as_ref(), parsed_revokes.as_ref())
        {
            for revoke in revokes {
                if grants.permissions.contains(revoke) {
                    break 'locked Outcome::Send(Box::new(ServerMessage::UserUpdateResponse {
                        success: false,
                        error: Some(err_permission_grant_revoke_conflict(
                            ctx.locale,
                            revoke.as_str(),
                        )),
                        id: None,
                        username: None,
                    }));
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

        let group_may_change = validated_remove_group || validated_group_id.is_some();
        let admin_status_may_change = request.is_admin.is_some_and(|new| new != old_is_admin);
        let bandwidth_weight_request_may_change = if request.inherit_bandwidth_weight == Some(true)
        {
            target_user_account
                .as_ref()
                .is_some_and(|account| account.bandwidth_weight.is_some())
        } else if let Some(new) = request.bandwidth_weight {
            target_user_account
                .as_ref()
                .is_some_and(|account| account.bandwidth_weight != Some(new))
        } else {
            false
        };
        let transfer_egress_weight_may_change = ctx.transfer_registry.has_active_user(request.id)
            && (bandwidth_weight_request_may_change || group_may_change || admin_status_may_change);
        let old_transfer_resolved = if transfer_egress_weight_may_change {
            match ctx.db.users.get_resolved_bandwidth_weight(request.id).await {
                Ok(weight) => Some(weight),
                Err(e) => {
                    error!(user = %requesting_user.username, ip = %ctx.peer_addr, target_user_id = request.id, err = %e, "{}", LOG_USER_UPDATE_DB_ERROR_LOOKUP);
                    None
                }
            }
        } else {
            None
        };

        if let Some(new_username) = request.username.as_deref()
            && fold_name(new_username) != fold_name(&old_username)
        {
            match ctx.db.users.get_user_by_username(new_username).await {
                Ok(Some(_)) => {
                    break 'locked Outcome::Send(Box::new(ServerMessage::UserUpdateResponse {
                        success: false,
                        error: Some(err_username_exists(ctx.locale, new_username)),
                        id: None,
                        username: None,
                    }));
                }
                Ok(None) => {}
                Err(e) => {
                    error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_USER_UPDATE_DB_ERROR_DUPLICATE_CHECK);
                    break 'locked Outcome::Send(Box::new(ServerMessage::UserUpdateResponse {
                        success: false,
                        error: Some(err_database(ctx.locale)),
                        id: None,
                        username: None,
                    }));
                }
            }
        }

        let mut user_area_migration = if let Some(new_username) = request.username.as_deref() {
            if old_username != new_username {
                match ctx.file_root {
                    Some(file_root) => {
                        match migrate_user_area_on_username_change(
                            file_root,
                            ctx.file_activity.as_ref(),
                            &old_username,
                            new_username,
                        )
                        .await
                        {
                            Ok(migration) => {
                                if migration.was_migrated() {
                                    info!(
                                        user = %requesting_user.username,
                                        ip = %ctx.peer_addr,
                                        old_username = %old_username,
                                        new_username = %new_username,
                                        "{}",
                                        LOG_USER_UPDATE_FILE_AREA_MIGRATED
                                    );
                                }
                                migration
                            }
                            Err(UserAreaMigrationError::TargetExists) => {
                                warn!(
                                    user = %requesting_user.username,
                                    ip = %ctx.peer_addr,
                                    old_username = %old_username,
                                    new_username = %new_username,
                                    "{}",
                                    LOG_USER_UPDATE_FILE_AREA_TARGET_EXISTS
                                );
                                break 'locked Outcome::Send(Box::new(
                                    ServerMessage::UserUpdateResponse {
                                        success: false,
                                        error: Some(err_personal_file_area_exists(
                                            ctx.locale,
                                            new_username,
                                        )),
                                        id: None,
                                        username: None,
                                    },
                                ));
                            }
                            Err(UserAreaMigrationError::Busy) => {
                                warn!(
                                    user = %requesting_user.username,
                                    ip = %ctx.peer_addr,
                                    old_username = %old_username,
                                    new_username = %new_username,
                                    "{}",
                                    LOG_USER_UPDATE_FILE_AREA_BUSY
                                );
                                break 'locked Outcome::Send(Box::new(
                                    ServerMessage::UserUpdateResponse {
                                        success: false,
                                        error: Some(err_personal_file_area_busy(ctx.locale)),
                                        id: None,
                                        username: None,
                                    },
                                ));
                            }
                            Err(UserAreaMigrationError::Io(e)) => {
                                error!(
                                    user = %requesting_user.username,
                                    ip = %ctx.peer_addr,
                                    old_username = %old_username,
                                    new_username = %new_username,
                                    err = %e,
                                    "{}",
                                    LOG_USER_UPDATE_FILE_AREA_MIGRATE_FAILED
                                );
                                break 'locked Outcome::Send(Box::new(
                                    ServerMessage::UserUpdateResponse {
                                        success: false,
                                        error: Some(err_personal_file_area_migration_failed(
                                            ctx.locale,
                                        )),
                                        id: None,
                                        username: None,
                                    },
                                ));
                            }
                        }
                    }
                    None => UserAreaMigration::not_needed(),
                }
            } else {
                UserAreaMigration::not_needed()
            }
        } else {
            UserAreaMigration::not_needed()
        };

        // Update (atomic last-admin protection lives in the SQL).
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
                // Sent in the tail dispatch after the block exits, so a slow admin
                // socket can't stall the security-relevant cascades below.
                let response = ServerMessage::UserUpdateResponse {
                    success: true,
                    error: None,
                    id: Some(request.id),
                    username: Some(updated_account.username.clone()),
                };
                if user_area_migration.was_migrated() {
                    ctx.file_index.mark_dirty();
                }

                let group_changed = group_may_change;
                let admin_status_changed = old_is_admin != updated_account.is_admin;
                let permissions_changed =
                    old_permissions.permissions != final_permissions.permissions;

                // Update is_admin + perms together: `has_permission` short-circuits
                // on is_admin, so a split write widens the demoted-admin window.
                if admin_status_changed || permissions_changed {
                    ctx.user_manager
                        .update_auth_state(
                            updated_account.id,
                            updated_account.is_admin,
                            final_permissions.permissions.clone(),
                        )
                        .await;
                }

                let (updated_group_id, updated_group_name) =
                    if let Some(gid) = updated_account.group_id {
                        match ctx.db.groups.get_group_by_id(gid).await {
                            Ok(Some(g)) => (Some(gid), Some(g.name)),
                            _ => (None, None),
                        }
                    } else {
                        (None, None)
                    };

                // A casing-only edit ("alice" -> "Alice") is still a display change
                // that must propagate, so compare exact display strings, not folded
                // keys (which would treat it as no change and broadcast nothing).
                let username_changed = old_username != updated_account.username;

                // Transfers (separate port, identity snapshotted at start) cache the
                // owner's name + admin color for the connection monitor; a rename OR a
                // promote/demote can make that stale, so refresh on either. The registry
                // keeps a shared account's per-session nickname intact and reads the
                // immutable is_shared from the transfer.
                if username_changed || admin_status_changed {
                    ctx.transfer_registry.update_user(
                        updated_account.id,
                        &updated_account.username,
                        updated_account.is_admin,
                    );
                }

                {
                    let enabled_changed = old_enabled != updated_account.enabled;
                    let actually_changed = admin_status_changed
                        || enabled_changed
                        || permissions_changed
                        || group_changed;

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

                        ctx.user_manager
                            .broadcast_to_user_id(updated_account.id, &permissions_update)
                            .await;

                        // Admin-aware: admins hold VoiceListen via the bypass, so a
                        // demoted admin without an explicit grant still loses it.
                        let had_voice_listen = old_is_admin
                            || old_permissions
                                .permissions
                                .contains(&Permission::VoiceListen);
                        let has_voice_listen = updated_account.is_admin
                            || final_permissions
                                .permissions
                                .contains(&Permission::VoiceListen);

                        if had_voice_listen && !has_voice_listen {
                            for session in ctx
                                .user_manager
                                .get_sessions_by_user_id(updated_account.id)
                                .await
                            {
                                if let Some(info) = ctx
                                    .voice_registry
                                    .remove_by_session_id(session.session_id)
                                    .await
                                {
                                    send_voice_leave_notifications(
                                        &info,
                                        Some(&session.tx),
                                        ctx.user_manager,
                                        ctx.channel_manager,
                                    )
                                    .await;
                                }
                            }
                        }
                    }
                }

                // Send a disabled-by-admin Error followed by Disconnect so the
                // connection task exits after writing the reason.
                // connection.rs won't re-broadcast UserDisconnected — already removed.
                if let Some(false) = request.enabled {
                    let sessions = ctx
                        .user_manager
                        .get_sessions_by_user_id(updated_account.id)
                        .await;
                    for user in &sessions {
                        let disconnect_msg = ServerMessage::Error {
                            message: err_account_disabled_by_admin(&user.locale),
                            command: None,
                            disconnect: false,
                        };
                        send_reason_and_disconnect(user, disconnect_msg);
                    }
                    remove_users_with_cleanup_locked(
                        ctx.user_manager,
                        ctx.voice_registry,
                        ctx.channel_manager,
                        &sessions,
                        false,
                    )
                    .await;
                }

                // Update the cached username/nickname only AFTER the teardown cascades
                // above (voice-listen revoke, disable) — so their nickname-keyed cleanup
                // (VoiceUserLeft / ChatUserLeft) carries the pre-rename name that clients
                // still hold; the rename broadcast below then re-keys survivors to the new
                // name. Those cascades route by user_id, so deferring the cache write
                // doesn't change who they reach.
                if username_changed {
                    ctx.user_manager
                        .update_username(updated_account.id, updated_account.username.clone())
                        .await;

                    // Voice stores nicknames, which a shared account's per-session logins
                    // keep across a username change (same invariant
                    // `UserManager::update_username` honors), so voice is
                    // regular-accounts-only.
                    if !updated_account.is_shared {
                        ctx.voice_registry
                            .update_nickname(&old_username, &updated_account.username)
                            .await;
                    }
                }

                // Promotion auto-clears group_id in the DB, so an admin-status change
                // must refresh the cached group even when group_id wasn't touched.
                if group_changed || admin_status_changed {
                    ctx.user_manager
                        .update_group(
                            updated_account.id,
                            updated_group_id,
                            updated_group_name.clone(),
                        )
                        .await;
                }

                // Inherit wins over explicit weight (matches the DB) — check it first
                // so a request also carrying the pre-update value still counts as a change.
                let bandwidth_weight_request_change =
                    if request.inherit_bandwidth_weight == Some(true) {
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
                        .get_sessions_by_user_id(updated_account.id)
                        .await
                } else {
                    Vec::new()
                };

                // All of a user's sessions share the cached weight (update_bandwidth_state
                // fan-out invariant); first session is authoritative, `None` = offline.
                let old_resolved: Option<u16> = sessions
                    .first()
                    .map(|s| s.bandwidth_weight.load(Ordering::Relaxed));

                if cache_should_refresh {
                    ctx.user_manager
                        .update_bandwidth_state(
                            updated_account.id,
                            updated_account.bandwidth_weight,
                            resolved_bandwidth_weight,
                        )
                        .await;
                }

                // Suppress a bandwidth-only broadcast when the effective weight didn't
                // move for an online user. (Offline accounts are skipped by the
                // emptiness guard below — UserUpdated is presence-only.)
                let suppress_for_bw_only =
                    bw_only_trigger && old_resolved == Some(resolved_bandwidth_weight);
                let egress_weight_changed =
                    old_resolved.is_some_and(|old| old != resolved_bandwidth_weight);
                let transfer_egress_weight_changed = transfer_egress_weight_may_change
                    && old_transfer_resolved != Some(resolved_bandwidth_weight);

                if broadcast_should_fire && !suppress_for_bw_only {
                    // Re-read post-update so per-session (shared) broadcasts carry the
                    // edited group/admin/weight — the `sessions` snapshot above predates
                    // the cache writes.
                    let online_sessions = ctx
                        .user_manager
                        .get_sessions_by_user_id(updated_account.id)
                        .await;

                    // Presence-only: an offline account has no live entry to refresh,
                    // and there's no reconnect reconciliation, so we broadcast nothing.
                    // Online: per-session for shared accounts, one aggregated entry for
                    // regular. `old_username` is the pre-edit name so a rename still
                    // matches the client's existing entry.
                    if !online_sessions.is_empty() {
                        broadcast_user_updated_for_members(
                            ctx,
                            &online_sessions,
                            Some(&old_username),
                            |_| true,
                        )
                        .await;

                        // A username rename leaves some of the account's own sessions
                        // unaware: UserUpdated above only reaches user_list holders. Cover
                        // the rest so no session keeps a stale cached identity.
                        if old_username != updated_account.username {
                            if updated_account.is_shared {
                                // Shared accounts' per-session nicknames don't change on a
                                // username rename (so no ChatUserRenamed), but the account
                                // username each session carries does. Direct-send the
                                // per-session UserUpdated to the sessions lacking user_list
                                // (user_list holders already got theirs via the broadcast
                                // above) so they refresh their cached account username; the
                                // client re-keys by previous_username and leaves the session
                                // nickname intact.
                                for session in &online_sessions {
                                    if !session.has_permission(Permission::UserList) {
                                        let self_update = ServerMessage::UserUpdated {
                                            previous_username: old_username.clone(),
                                            user: UserManager::build_user_info_from_session(
                                                session,
                                            ),
                                        };
                                        let _ = session.tx.send_message(self_update, None);
                                    }
                                }
                            } else {
                                // A regular-account rename changes the nickname (==
                                // username), so tell every channel the user is in — via the
                                // channel stream, not the UserList-gated UserUpdated — to
                                // keep member lists and voiced sets in sync for all members
                                // (incl. non-UserList ones) and the renamed user themselves.
                                broadcast_chat_user_renamed(
                                    ctx.user_manager,
                                    ctx.channel_manager,
                                    &online_sessions,
                                    &old_username,
                                    &updated_account.username,
                                    updated_account.is_admin,
                                )
                                .await;

                                // ChatUserRenamed only reaches channels the user is in, so a
                                // renamed regular user with neither user_list nor channels
                                // would never learn of their own rename — leaving a stale
                                // cached identity (broken self-message detection).
                                // Direct-send the aggregated UserUpdated to the account's own
                                // sessions that lack user_list (the ones the broadcast above
                                // skipped); the client applies it idempotently, so the
                                // overlap with ChatUserRenamed for channel members is
                                // harmless. user_list sessions already got it above, so
                                // they're skipped to keep exactly one UserUpdated per
                                // receiver.
                                if let Some(user_info) =
                                    UserManager::build_aggregated_user_info(&online_sessions)
                                {
                                    let self_update = ServerMessage::UserUpdated {
                                        previous_username: old_username.clone(),
                                        user: user_info,
                                    };
                                    for session in &online_sessions {
                                        if !session.has_permission(Permission::UserList) {
                                            let _ =
                                                session.tx.send_message(self_update.clone(), None);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                if egress_weight_changed || transfer_egress_weight_changed {
                    update_egress_user_weight(ctx, updated_account.id, resolved_bandwidth_weight);
                }

                info!(
                    user = %requesting_user.username,
                    ip = %ctx.peer_addr,
                    target = %updated_account.username,
                    is_admin = updated_account.is_admin,
                    "{}", LOG_USER_UPDATE_SUCCESS
                );
                Outcome::Send(Box::new(response))
            }
            Ok(crate::db::UpdateUserResult::BlockedForGroupAuth) => {
                let rollback_failed = if let Some(new_username) = request.username.as_deref() {
                    rollback_personal_area_migration_if_needed(
                        ctx.file_root,
                        ctx.file_activity.as_ref(),
                        &mut user_area_migration,
                        &old_username,
                        new_username,
                    )
                    .await
                } else {
                    false
                };
                if rollback_failed {
                    ctx.file_index.mark_dirty();
                }
                // In-tx group-auth race: target group changed between pre-check and tx.
                // Conservative message — we don't know which condition raced.
                warn!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_USER_UPDATE_PERMISSION_DENIED);
                let response = ServerMessage::UserUpdateResponse {
                    success: false,
                    error: Some(append_personal_area_rollback_warning_if_needed(
                        ctx.locale,
                        err_update_failed(ctx.locale, &target_username),
                        &old_username,
                        request.username.as_deref(),
                        rollback_failed,
                    )),
                    id: None,
                    username: None,
                };
                Outcome::Send(Box::new(response))
            }
            Ok(crate::db::UpdateUserResult::Blocked) => {
                let rollback_failed = if let Some(new_username) = request.username.as_deref() {
                    rollback_personal_area_migration_if_needed(
                        ctx.file_root,
                        ctx.file_activity.as_ref(),
                        &mut user_area_migration,
                        &old_username,
                        new_username,
                    )
                    .await
                } else {
                    false
                };
                if rollback_failed {
                    ctx.file_index.mark_dirty();
                }
                // Blocked (not found / last admin / duplicate username / raced
                // promotion). Disambiguate with explicit DB reads — a silent
                // `.ok().flatten()` would let a DB error look like "not found".
                let target_after = match ctx.db.users.get_user_by_username(&target_username).await {
                    Ok(t) => t,
                    Err(e) => {
                        error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_USER_UPDATE_DB_ERROR_TARGET);
                        break 'locked Outcome::Send(Box::new(ServerMessage::UserUpdateResponse {
                            success: false,
                            error: Some(append_personal_area_rollback_warning_if_needed(
                                ctx.locale,
                                err_database(ctx.locale),
                                &old_username,
                                request.username.as_deref(),
                                rollback_failed,
                            )),
                            id: None,
                            username: None,
                        }));
                    }
                };
                let error_message = if target_after.is_none() {
                    err_user_not_found(ctx.locale, &target_username)
                } else if !requesting_user.is_admin
                    && target_after.as_ref().is_some_and(|u| u.is_admin)
                {
                    // Race: pre-check saw non-admin; an admin promoted them
                    // before the SQL UPDATE.
                    warn!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_USER_UPDATE_ADMIN);
                    err_cannot_edit_admin(ctx.locale)
                } else if let Some(ref new_username) = request.username {
                    let duplicate = if fold_name(new_username) != fold_name(&target_username) {
                        match ctx.db.users.get_user_by_username(new_username).await {
                            Ok(t) => t.is_some(),
                            Err(e) => {
                                error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_USER_UPDATE_DB_ERROR_DUPLICATE_CHECK);
                                break 'locked Outcome::Send(Box::new(
                                    ServerMessage::UserUpdateResponse {
                                        success: false,
                                        error: Some(
                                            append_personal_area_rollback_warning_if_needed(
                                                ctx.locale,
                                                err_database(ctx.locale),
                                                &old_username,
                                                request.username.as_deref(),
                                                rollback_failed,
                                            ),
                                        ),
                                        id: None,
                                        username: None,
                                    },
                                ));
                            }
                        }
                    } else {
                        false
                    };
                    if duplicate {
                        err_username_exists(ctx.locale, new_username)
                    } else {
                        // Not a duplicate, so the block must be last-admin protection.
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
                    error: Some(append_personal_area_rollback_warning_if_needed(
                        ctx.locale,
                        error_message,
                        &old_username,
                        request.username.as_deref(),
                        rollback_failed,
                    )),
                    id: None,
                    username: None,
                };
                Outcome::Send(Box::new(response))
            }
            Err(e) => {
                let rollback_failed = if let Some(new_username) = request.username.as_deref() {
                    rollback_personal_area_migration_if_needed(
                        ctx.file_root,
                        ctx.file_activity.as_ref(),
                        &mut user_area_migration,
                        &old_username,
                        new_username,
                    )
                    .await
                } else {
                    false
                };
                if rollback_failed {
                    ctx.file_index.mark_dirty();
                }
                error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_USER_UPDATE_DB_ERROR);
                Outcome::Send(Box::new(ServerMessage::UserUpdateResponse {
                    success: false,
                    error: Some(append_personal_area_rollback_warning_if_needed(
                        ctx.locale,
                        err_database(ctx.locale),
                        &old_username,
                        request.username.as_deref(),
                        rollback_failed,
                    )),
                    id: None,
                    username: None,
                }))
            }
        }
    };

    dispatch_outcome(outcome, ctx, HANDLER_USER_UPDATE).await
}

fn append_personal_area_rollback_warning_if_needed(
    locale: &str,
    error: String,
    old_username: &str,
    new_username: Option<&str>,
    rollback_failed: bool,
) -> String {
    let Some(new_username) = new_username.filter(|_| rollback_failed) else {
        return error;
    };
    format!(
        "{} {}",
        error,
        err_personal_file_area_rollback_failed_warning(locale, old_username, new_username)
    )
}

async fn rollback_personal_area_migration_if_needed(
    file_root: Option<&std::path::Path>,
    file_activity: &crate::files::FileActivityMap,
    migration: &mut UserAreaMigration,
    old_username: &str,
    new_username: &str,
) -> bool {
    if !migration.was_migrated() {
        return false;
    }

    migration.release_activity();

    let Some(file_root) = file_root else {
        return false;
    };

    if let Err(e) =
        migrate_user_area_on_username_change(file_root, file_activity, new_username, old_username)
            .await
    {
        error!(
            old_username,
            new_username,
            err = %e,
            "{}",
            LOG_USER_UPDATE_FILE_AREA_ROLLBACK_FAILED
        );
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, fs};

    use super::*;
    use crate::channels::JoinPolicy;
    use crate::db;
    use crate::egress::task::EgressSettingsCommand;
    #[allow(unused_imports)]
    use crate::handlers::testing::read_login_response;
    use crate::handlers::testing::*;
    use crate::transfers::registry::{TransferDirection, TransferRegistration};
    use crate::users::user::{ConnectionWriter, NewSessionParams, SessionRx};

    fn register_active_transfer(
        test_ctx: &mut TestContext,
        user_id: i64,
        username: &str,
        is_admin: bool,
    ) {
        let (_info, _rx) = test_ctx.transfer_registry.register(TransferRegistration {
            user_id,
            peer_addr: test_ctx.peer_addr,
            nickname: username.to_string(),
            username: username.to_string(),
            is_admin,
            is_shared: false,
            direction: TransferDirection::Download,
            path: "/files/test.bin".to_string(),
            total_size: 1024,
        });
    }

    #[test]
    fn test_personal_area_rollback_warning_appends_to_primary_error() {
        let message = append_personal_area_rollback_warning_if_needed(
            DEFAULT_TEST_LOCALE,
            "Primary failure.".to_string(),
            "alice",
            Some("alicia"),
            true,
        );

        assert!(message.starts_with("Primary failure. "));
        assert!(message.contains("rollback failed"));
        assert!(message.contains("alice"));
        assert!(message.contains("alicia"));
    }

    #[test]
    fn test_personal_area_rollback_warning_skipped_when_not_failed() {
        let message = append_personal_area_rollback_warning_if_needed(
            DEFAULT_TEST_LOCALE,
            "Primary failure.".to_string(),
            "alice",
            Some("alicia"),
            false,
        );

        assert_eq!(message, "Primary failure.");
    }

    #[tokio::test]
    async fn test_rollback_personal_area_migration_reverses_rename_and_releases_activity() {
        let temp = tempfile::TempDir::new().unwrap();
        let root = temp.path();
        let users_dir = root.join("users");
        let old_dir = users_dir.join("alice");
        let new_dir = users_dir.join("alicia");
        fs::create_dir_all(&old_dir).unwrap();
        fs::write(old_dir.join("note.txt"), "personal").unwrap();

        let activity = crate::files::FileActivityMap::new();
        let mut migration =
            migrate_user_area_on_username_change(root, &activity, "alice", "alicia")
                .await
                .unwrap();

        assert!(migration.was_migrated());
        assert!(!old_dir.exists());
        assert_eq!(
            fs::read_to_string(new_dir.join("note.txt")).unwrap(),
            "personal"
        );

        let rollback_failed = rollback_personal_area_migration_if_needed(
            Some(root),
            &activity,
            &mut migration,
            "alice",
            "alicia",
        )
        .await;

        assert!(!rollback_failed);
        assert!(!new_dir.exists());
        assert_eq!(
            fs::read_to_string(old_dir.join("note.txt")).unwrap(),
            "personal"
        );

        let post_rollback_guard = activity
            .try_enter_directory_paths(root, &[old_dir, new_dir])
            .await
            .unwrap();
        assert!(post_rollback_guard.is_ok());
    }

    #[tokio::test]
    async fn test_userupdate_requires_login() {
        let mut test_ctx = create_test_context().await;

        // Non-existent id: the login check fires before user lookup.
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

        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

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
    async fn test_userupdate_can_rename_to_deleted_account_old_username() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;
        let deleted_user = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "retired",
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
        assert!(
            test_ctx
                .db
                .users
                .delete_user(deleted_user.id, true)
                .await
                .unwrap(),
            "setup should delete the old account"
        );

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
            username: Some("retired".to_string()),
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
        handle_user_update(request, &mut test_ctx.handler_context())
            .await
            .unwrap();

        match read_server_message(&mut test_ctx).await {
            ServerMessage::UserUpdateResponse {
                success,
                error,
                id,
                username,
            } => {
                assert!(success, "rename should succeed: {:?}", error);
                assert_eq!(id, Some(admin_user.id));
                assert_eq!(username, Some("retired".to_string()));
            }
            other => panic!("Expected UserUpdateResponse, got {other:?}"),
        }

        assert!(
            test_ctx
                .db
                .users
                .get_user_by_username("admin")
                .await
                .unwrap()
                .is_none()
        );
        let renamed_user = test_ctx
            .db
            .users
            .get_user_by_username("retired")
            .await
            .unwrap()
            .expect("renamed account should own the old deleted username");
        assert_eq!(renamed_user.id, admin_user.id);
        assert_eq!(renamed_user.username, "retired");
        assert!(
            test_ctx
                .db
                .users
                .get_user_by_id(deleted_user.id)
                .await
                .unwrap()
                .is_none(),
            "deleted account must not be resurrected"
        );

        let session = test_ctx
            .user_manager
            .get_user_by_session_id(session_id)
            .await
            .expect("renamed admin session should remain online");
        assert_eq!(session.username, "retired");
        assert_eq!(session.nickname, "retired");
    }

    #[tokio::test]
    async fn test_userupdate_case_only_self_rename_succeeds() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let admin_user = test_ctx
            .db
            .users
            .get_user_by_username("admin")
            .await
            .unwrap()
            .unwrap();
        // Case-only self-rename: the pre-check guard is fold-equal (skipped) and
        // the UPDATE writes the same `username_lower` to the same row (no
        // self-collision), so the new display case lands.
        let request = UserUpdateRequest {
            id: admin_user.id,
            current_password: None,
            username: Some("Admin".to_string()),
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
        match read_server_message(&mut test_ctx).await {
            ServerMessage::UserUpdateResponse {
                success,
                error,
                username,
                ..
            } => {
                assert!(success, "case-only self-rename should succeed: {:?}", error);
                assert_eq!(username, Some("Admin".to_string()));
            }
            _ => panic!("Expected UserUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_userupdate_case_only_rename_keeps_personal_file_area_access() {
        let mut test_ctx = create_test_context().await;
        let _file_area = setup_file_area_basic(&mut test_ctx);
        let file_root = test_ctx.file_root.expect("file root configured");

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;
        let alice = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "Alice",
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

        let old_dir = file_root.join("users").join("Alice");
        let new_dir = file_root.join("users").join("alice");
        fs::create_dir(&old_dir).unwrap();
        fs::write(old_dir.join("note.txt"), "personal").unwrap();

        let request = UserUpdateRequest {
            id: alice.id,
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
        handle_user_update(request, &mut test_ctx.handler_context())
            .await
            .unwrap();

        match read_server_message(&mut test_ctx).await {
            ServerMessage::UserUpdateResponse {
                success,
                error,
                username,
                ..
            } => {
                assert!(success, "case-only rename should succeed: {:?}", error);
                assert_eq!(username, Some("alice".to_string()));
            }
            other => panic!("Expected UserUpdateResponse, got {other:?}"),
        }

        let updated = test_ctx
            .db
            .users
            .get_user_by_username("alice")
            .await
            .unwrap()
            .expect("renamed user should exist");
        assert_eq!(updated.id, alice.id);
        assert_eq!(updated.username, "alice");
        let old_note_exists = old_dir.join("note.txt").exists();
        assert!(new_dir.join("note.txt").exists());
        if old_note_exists {
            assert!(!test_ctx.file_index.is_dirty());
        } else {
            assert!(test_ctx.file_index.is_dirty());
        }
    }

    #[tokio::test]
    async fn test_userupdate_rename_migrates_personal_file_area() {
        let mut test_ctx = create_test_context().await;
        let _file_area = setup_file_area_basic(&mut test_ctx);
        let file_root = test_ctx.file_root.expect("file root configured");

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;
        let alice = test_ctx
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

        let old_dir = file_root.join("users").join("alice");
        let new_dir = file_root.join("users").join("alicia");
        fs::create_dir(&old_dir).unwrap();
        fs::write(old_dir.join("note.txt"), "personal").unwrap();

        let request = UserUpdateRequest {
            id: alice.id,
            current_password: None,
            username: Some("alicia".to_string()),
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
        handle_user_update(request, &mut test_ctx.handler_context())
            .await
            .unwrap();

        match read_server_message(&mut test_ctx).await {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(success, "rename should succeed: {:?}", error);
            }
            other => panic!("Expected UserUpdateResponse, got {other:?}"),
        }
        assert!(!old_dir.exists());
        assert!(new_dir.join("note.txt").exists());
        assert!(test_ctx.file_index.is_dirty());
    }

    #[tokio::test]
    async fn test_userupdate_rename_migrates_shared_account_personal_file_area() {
        let mut test_ctx = create_test_context().await;
        let _file_area = setup_file_area_basic(&mut test_ctx);
        let file_root = test_ctx.file_root.expect("file root configured");

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;
        let shared = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "guests",
                hashed_password: "hash",
                is_admin: false,
                is_shared: true,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        let old_dir = file_root.join("users").join("guests");
        let new_dir = file_root.join("users").join("visitors");
        fs::create_dir(&old_dir).unwrap();
        fs::write(old_dir.join("welcome.txt"), "shared").unwrap();

        let request = UserUpdateRequest {
            id: shared.id,
            current_password: None,
            username: Some("visitors".to_string()),
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
        handle_user_update(request, &mut test_ctx.handler_context())
            .await
            .unwrap();

        match read_server_message(&mut test_ctx).await {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(success, "shared rename should succeed: {:?}", error);
            }
            other => panic!("Expected UserUpdateResponse, got {other:?}"),
        }
        assert!(!old_dir.exists());
        assert!(new_dir.join("welcome.txt").exists());
    }

    #[tokio::test]
    async fn test_userupdate_rename_fails_when_personal_file_area_target_exists() {
        let mut test_ctx = create_test_context().await;
        let _file_area = setup_file_area_basic(&mut test_ctx);
        let file_root = test_ctx.file_root.expect("file root configured");

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;
        let alice = test_ctx
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

        let old_dir = file_root.join("users").join("alice");
        let new_dir = file_root.join("users").join("alicia");
        fs::create_dir(&old_dir).unwrap();
        fs::create_dir(&new_dir).unwrap();

        let request = UserUpdateRequest {
            id: alice.id,
            current_password: None,
            username: Some("alicia".to_string()),
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
        handle_user_update(request, &mut test_ctx.handler_context())
            .await
            .unwrap();

        match read_server_message(&mut test_ctx).await {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(!success, "rename should fail on area collision");
                assert!(
                    error
                        .as_deref()
                        .is_some_and(|msg| msg.contains("Personal file area")),
                    "unexpected error: {:?}",
                    error
                );
            }
            other => panic!("Expected UserUpdateResponse, got {other:?}"),
        }
        assert!(old_dir.exists());
        assert!(new_dir.exists());
        assert!(
            test_ctx
                .db
                .users
                .get_user_by_username("alice")
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            test_ctx
                .db
                .users
                .get_user_by_username("alicia")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn test_userupdate_rename_fails_when_personal_file_area_busy() {
        let mut test_ctx = create_test_context().await;
        let _file_area = setup_file_area_basic(&mut test_ctx);
        let file_root = test_ctx.file_root.expect("file root configured");

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;
        let alice = test_ctx
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

        let old_dir = file_root.join("users").join("alice");
        let new_dir = file_root.join("users").join("alicia");
        fs::create_dir(&old_dir).unwrap();
        fs::write(old_dir.join("note.txt"), "personal").unwrap();

        let _active_file_operation = test_ctx
            .file_activity
            .try_enter_child_path(file_root, &old_dir.join("note.txt"))
            .await
            .unwrap()
            .unwrap();
        let request = UserUpdateRequest {
            id: alice.id,
            current_password: None,
            username: Some("alicia".to_string()),
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
        handle_user_update(request, &mut test_ctx.handler_context())
            .await
            .unwrap();

        match read_server_message(&mut test_ctx).await {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(!success, "rename should fail while area is busy");
                assert_eq!(
                    error,
                    Some(err_personal_file_area_busy(DEFAULT_TEST_LOCALE))
                );
            }
            other => panic!("Expected UserUpdateResponse, got {other:?}"),
        }
        assert!(old_dir.join("note.txt").exists());
        assert!(!new_dir.exists());
        assert!(
            test_ctx
                .db
                .users
                .get_user_by_username("alice")
                .await
                .unwrap()
                .is_some()
        );
    }

    #[tokio::test]
    async fn test_userupdate_duplicate_username_does_not_move_personal_file_area() {
        let mut test_ctx = create_test_context().await;
        let _file_area = setup_file_area_basic(&mut test_ctx);
        let file_root = test_ctx.file_root.expect("file root configured");

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;
        let alice = test_ctx
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

        let old_dir = file_root.join("users").join("alice");
        let duplicate_dir = file_root.join("users").join("bob");
        fs::create_dir(&old_dir).unwrap();
        fs::write(old_dir.join("note.txt"), "personal").unwrap();

        let request = UserUpdateRequest {
            id: alice.id,
            current_password: None,
            username: Some("bob".to_string()),
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
        handle_user_update(request, &mut test_ctx.handler_context())
            .await
            .unwrap();

        match read_server_message(&mut test_ctx).await {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(!success, "duplicate username should fail");
                assert_eq!(error, Some(err_username_exists(DEFAULT_TEST_LOCALE, "bob")));
            }
            other => panic!("Expected UserUpdateResponse, got {other:?}"),
        }
        assert!(old_dir.join("note.txt").exists());
        assert!(!duplicate_dir.exists());
        assert!(
            test_ctx
                .db
                .users
                .get_user_by_username("alice")
                .await
                .unwrap()
                .is_some()
        );
    }

    #[tokio::test]
    async fn test_userupdate_rename_allows_preprovisioned_area_when_old_missing() {
        let mut test_ctx = create_test_context().await;
        let _file_area = setup_file_area_basic(&mut test_ctx);
        let file_root = test_ctx.file_root.expect("file root configured");

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;
        let alice = test_ctx
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

        let new_dir = file_root.join("users").join("alicia");
        fs::create_dir(&new_dir).unwrap();
        fs::write(new_dir.join("prepared.txt"), "ready").unwrap();

        let request = UserUpdateRequest {
            id: alice.id,
            current_password: None,
            username: Some("alicia".to_string()),
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
        handle_user_update(request, &mut test_ctx.handler_context())
            .await
            .unwrap();

        match read_server_message(&mut test_ctx).await {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(
                    success,
                    "rename should use preprovisioned area: {:?}",
                    error
                );
            }
            other => panic!("Expected UserUpdateResponse, got {other:?}"),
        }
        assert!(new_dir.join("prepared.txt").exists());
        assert!(
            test_ctx
                .db
                .users
                .get_user_by_username("alicia")
                .await
                .unwrap()
                .is_some()
        );
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

        let session_id = login_user(&mut test_ctx, "alice", "oldpassword", &[], false).await;

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

        let session_id = login_user(&mut test_ctx, "alice", "correctpassword", &[], false).await;

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

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

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
    async fn test_userupdate_rename_and_userdelete_serialize_cleanly() {
        let mut test_ctx = create_test_context().await;

        let update_admin_session =
            login_user(&mut test_ctx, "update_admin", "password", &[], true).await;
        let delete_admin_session =
            login_user(&mut test_ctx, "delete_admin", "password", &[], true).await;
        let bob_session = login_user(&mut test_ctx, "bob", "password", &[], false).await;
        let bob = test_ctx
            .db
            .users
            .get_user_by_username("bob")
            .await
            .unwrap()
            .unwrap();

        let (update_server, update_client) = tokio::io::duplex(4096);
        let (delete_server, delete_client) = tokio::io::duplex(4096);
        let mut update_writer = nexus_common::framing::FrameWriter::new(update_server);
        let mut delete_writer = nexus_common::framing::FrameWriter::new(delete_server);
        let mut update_reader =
            nexus_common::framing::FrameReader::new(tokio::io::BufReader::new(update_client));
        let mut delete_reader =
            nexus_common::framing::FrameReader::new(tokio::io::BufReader::new(delete_client));

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
            session_id: Some(update_admin_session),
        };

        let (update_result, delete_result) = {
            let mut update_ctx = HandlerContext {
                writer: DirectWriter::new(&mut update_writer),
                peer_addr: test_ctx.peer_addr,
                user_manager: &test_ctx.user_manager,
                db: &test_ctx.db,
                tx: &test_ctx.tx,
                egress: &test_ctx.egress,
                egress_connection_id: test_ctx.egress_connection_id,
                egress_connection_registered: true,
                locale: DEFAULT_TEST_LOCALE,
                message_id: test_ctx.message_id,
                file_root: test_ctx.file_root,
                transfer_port: nexus_common::DEFAULT_TRANSFER_PORT,
                transfer_websocket_port: Some(nexus_common::DEFAULT_TRANSFER_WEBSOCKET_PORT),
                connection_tracker: test_ctx.connection_tracker.clone(),
                ip_rule_cache: test_ctx.ip_rule_cache.clone(),
                file_index: test_ctx.file_index.clone(),
                file_activity: test_ctx.file_activity.clone(),
                channel_manager: &test_ctx.channel_manager,
                transfer_registry: test_ctx.transfer_registry.clone(),
                voice_registry: &test_ctx.voice_registry,
                tracker_manager: &test_ctx.tracker_manager,
                fingerprint: TEST_FINGERPRINT,
                flood_config: test_ctx.flood_config.clone(),
            };
            let mut delete_ctx = HandlerContext {
                writer: DirectWriter::new(&mut delete_writer),
                peer_addr: test_ctx.peer_addr,
                user_manager: &test_ctx.user_manager,
                db: &test_ctx.db,
                tx: &test_ctx.tx,
                egress: &test_ctx.egress,
                egress_connection_id: test_ctx.egress_connection_id,
                egress_connection_registered: true,
                locale: DEFAULT_TEST_LOCALE,
                message_id: test_ctx.message_id,
                file_root: test_ctx.file_root,
                transfer_port: nexus_common::DEFAULT_TRANSFER_PORT,
                transfer_websocket_port: Some(nexus_common::DEFAULT_TRANSFER_WEBSOCKET_PORT),
                connection_tracker: test_ctx.connection_tracker.clone(),
                ip_rule_cache: test_ctx.ip_rule_cache.clone(),
                file_index: test_ctx.file_index.clone(),
                file_activity: test_ctx.file_activity.clone(),
                channel_manager: &test_ctx.channel_manager,
                transfer_registry: test_ctx.transfer_registry.clone(),
                voice_registry: &test_ctx.voice_registry,
                tracker_manager: &test_ctx.tracker_manager,
                fingerprint: TEST_FINGERPRINT,
                flood_config: test_ctx.flood_config.clone(),
            };

            tokio::join!(
                handle_user_update(request, &mut update_ctx),
                crate::handlers::user_delete::handle_user_delete(
                    bob.id,
                    Some(delete_admin_session),
                    &mut delete_ctx,
                )
            )
        };

        update_result.unwrap();
        delete_result.unwrap();

        let update_message = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            nexus_common::io::read_server_message(&mut update_reader),
        )
        .await
        .expect("timed out waiting for user update response")
        .unwrap()
        .expect("user update connection closed")
        .message;
        let delete_message = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            nexus_common::io::read_server_message(&mut delete_reader),
        )
        .await
        .expect("timed out waiting for user delete response")
        .unwrap()
        .expect("user delete connection closed")
        .message;

        let update_success = match update_message {
            ServerMessage::UserUpdateResponse {
                success,
                error,
                id,
                username,
            } => {
                if success {
                    assert!(error.is_none());
                    assert_eq!(id, Some(bob.id));
                    assert_eq!(username, Some("robert".to_string()));
                } else {
                    assert_eq!(
                        error,
                        Some(err_user_not_found(DEFAULT_TEST_LOCALE, &bob.id.to_string()))
                    );
                    assert!(id.is_none());
                    assert!(username.is_none());
                }
                success
            }
            other => panic!("Expected UserUpdateResponse, got {other:?}"),
        };

        match delete_message {
            ServerMessage::UserDeleteResponse {
                success,
                error,
                username,
            } => {
                assert!(success, "delete should win or clean up after the rename");
                assert!(error.is_none());
                assert_eq!(
                    username,
                    Some(if update_success { "robert" } else { "bob" }.to_string())
                );
            }
            other => panic!("Expected UserDeleteResponse, got {other:?}"),
        }

        assert!(
            test_ctx
                .db
                .users
                .get_user_by_id(bob.id)
                .await
                .unwrap()
                .is_none(),
            "target row must be gone after delete"
        );
        assert!(
            test_ctx
                .db
                .users
                .get_user_by_username("bob")
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            test_ctx
                .db
                .users
                .get_user_by_username("robert")
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            test_ctx
                .user_manager
                .get_user_by_session_id(bob_session)
                .await
                .is_none(),
            "deleted target session must be removed from the cache"
        );
        assert!(
            test_ctx
                .user_manager
                .get_sessions_by_username("bob")
                .await
                .is_empty()
        );
        assert!(
            test_ctx
                .user_manager
                .get_sessions_by_username("robert")
                .await
                .is_empty()
        );
    }

    #[tokio::test]
    async fn test_userupdate_cannot_demote_last_admin() {
        let mut test_ctx = create_test_context().await;

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

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[crate::db::Permission::UserEdit],
            false,
        )
        .await;

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
    async fn test_userupdate_casing_only_change_propagates() {
        let mut test_ctx = create_test_context().await;

        // Admin actor with UserEdit, plus an online target to observe propagation.
        let admin_session = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[crate::db::Permission::UserEdit],
            false,
        )
        .await;
        let carol_session = login_user(&mut test_ctx, "carol", "password", &[], false).await;
        let carol = test_ctx
            .db
            .users
            .get_user_by_username("carol")
            .await
            .unwrap()
            .unwrap();

        // Casing-only rename: "carol" -> "Carol".
        let request = UserUpdateRequest {
            id: carol.id,
            current_password: None,
            username: Some("Carol".to_string()),
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

        // The session cache must reflect the new case — proving `username_changed`
        // fired and `update_username` ran (the folded comparison treated it as a
        // no-op and propagated nothing).
        let session = test_ctx
            .user_manager
            .get_user_by_session_id(carol_session)
            .await
            .expect("carol session present");
        assert_eq!(
            session.username, "Carol",
            "casing-only rename must update the cached username"
        );
        assert_eq!(session.nickname, "Carol");
    }

    /// A session in `#general` with a readable rx, for asserting on channel-stream
    /// messages other members receive.
    async fn add_channel_member(
        test_ctx: &TestContext,
        user_id: i64,
        username: &str,
        permissions: std::collections::HashSet<Permission>,
    ) -> (u32, SessionRx) {
        let (tx, rx) = ConnectionWriter::channel();
        let session_id = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                user_id,
                username: username.to_string(),
                nickname: username.to_string(),
                is_admin: false,
                is_shared: false,
                permissions,
                address: test_ctx.peer_addr,
                created_at: 0,
                tx,
                features: vec![],
                locale: DEFAULT_TEST_LOCALE.to_string(),
                avatar: None,
                is_away: false,
                status: None,
                group_id: None,
                group_name: None,
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
                bandwidth_weight_override: None,
                last_activity: std::time::Instant::now(),
            })
            .await
            .expect("add session");
        let _ = test_ctx
            .channel_manager
            .join("#general", session_id, JoinPolicy::CreateIfMissing)
            .await;
        (session_id, rx)
    }

    #[tokio::test]
    async fn test_userupdate_rename_emits_chat_user_renamed_to_channel_observer() {
        use std::collections::HashSet;

        let mut test_ctx = create_test_context().await;
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let alice = test_ctx
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

        let (_alice_session, _alice_rx) =
            add_channel_member(&test_ctx, alice.id, "alice", HashSet::new()).await;
        let (_observer_session, mut observer_rx) =
            add_channel_member(&test_ctx, 999, "observer", HashSet::new()).await;

        let request = UserUpdateRequest {
            id: alice.id,
            current_password: None,
            username: Some("alicia".to_string()),
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
        handle_user_update(request, &mut test_ctx.handler_context())
            .await
            .unwrap();

        let mut found = false;
        while let Ok(event) = observer_rx.try_recv() {
            let (msg, _) = event.expect_message();
            if let ServerMessage::ChatUserRenamed {
                channel,
                old_nickname,
                new_nickname,
                is_admin,
            } = msg
            {
                assert_eq!(channel, "#general");
                assert_eq!(old_nickname, "alice");
                assert_eq!(new_nickname, "alicia");
                assert!(!is_admin);
                found = true;
            }
        }
        assert!(
            found,
            "channel observer must receive ChatUserRenamed for a regular account rename"
        );
    }

    #[tokio::test]
    async fn test_userupdate_rename_and_disable_uses_pre_rename_nickname() {
        use std::collections::HashSet;

        let mut test_ctx = create_test_context().await;
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
        let alice = test_ctx
            .db
            .users
            .get_user_by_username("alice")
            .await
            .unwrap()
            .unwrap();
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // alice and an observer both in #general; the observer's rx is readable.
        let (alice_session, _a) =
            add_channel_member(&test_ctx, alice.id, "alice", HashSet::new()).await;
        let (_obs, mut obs_rx) =
            add_channel_member(&test_ctx, 999, "observer", HashSet::new()).await;
        assert!(
            test_ctx
                .channel_manager
                .get_members("#general")
                .await
                .is_some_and(|m| m.contains(&alice_session)),
            "test setup: alice must be a #general member"
        );

        // Rename alice -> alicia AND disable in one update.
        let request = UserUpdateRequest {
            id: alice.id,
            current_password: None,
            username: Some("alicia".to_string()),
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
        handle_user_update(request, &mut test_ctx.handler_context())
            .await
            .unwrap();

        // The disable teardown fires before clients learn of the rename, so its
        // ChatUserLeft must carry the pre-rename nickname the observer still holds.
        let mut left = None;
        while let Ok(event) = obs_rx.try_recv() {
            let (msg, _) = event.expect_message();
            if let ServerMessage::ChatUserLeft { nickname, .. } = msg {
                left = Some(nickname);
            }
        }
        assert_eq!(
            left,
            Some("alice".to_string()),
            "rename+disable teardown must use the pre-rename nickname"
        );
    }

    #[tokio::test]
    async fn test_userupdate_rename_and_voice_revoke_uses_pre_rename_nickname() {
        use std::collections::HashSet;

        let mut test_ctx = create_test_context().await;
        let mut voice_perms = Permissions::new();
        voice_perms.permissions.insert(Permission::VoiceListen);
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "alice",
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
        let alice = test_ctx
            .db
            .users
            .get_user_by_username("alice")
            .await
            .unwrap()
            .unwrap();
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let voiced: HashSet<Permission> = [Permission::VoiceListen].into_iter().collect();
        let (alice_session, _a) =
            add_channel_member(&test_ctx, alice.id, "alice", voiced.clone()).await;
        // Observer holds voice_listen so it receives the channel's VoiceUserLeft.
        let (_obs, mut obs_rx) = add_channel_member(&test_ctx, 999, "observer", voiced).await;

        // Put alice in voice.
        test_ctx
            .voice_registry
            .add(crate::voice::VoiceSession::new(
                "alice".to_string(),
                vec!["#general".to_string()],
                alice_session,
            ))
            .await
            .expect("add voice session");

        // Rename alice -> alicia AND revoke voice_listen in one update.
        let request = UserUpdateRequest {
            id: alice.id,
            current_password: None,
            username: Some("alicia".to_string()),
            password: None,
            is_admin: None,
            enabled: None,
            permissions: Some(vec![]),
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

        // The voice teardown fires before the rename broadcast, so VoiceUserLeft must
        // carry the pre-rename nickname.
        let mut left = None;
        while let Ok(event) = obs_rx.try_recv() {
            let (msg, _) = event.expect_message();
            if let ServerMessage::VoiceUserLeft { nickname, .. } = msg {
                left = Some(nickname);
            }
        }
        assert_eq!(
            left,
            Some("alice".to_string()),
            "rename+voice-revoke teardown must use the pre-rename nickname"
        );
    }

    #[tokio::test]
    async fn test_userupdate_non_admin_cannot_change_admin_status() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[crate::db::Permission::UserEdit],
            false,
        )
        .await;

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
    async fn test_userupdate_duplicate_username_unicode() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // "Éclair" and "bob"; renaming bob to "éclair" differs only by Unicode
        // case, colliding solely through the folded username_lower index.
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "Éclair",
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

        let request = UserUpdateRequest {
            id: bob.id,
            current_password: None,
            username: Some("éclair".to_string()),
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
        match read_server_message(&mut test_ctx).await {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            _ => panic!("Expected UserUpdateResponse"),
        }
    }

    /// A regular account can't be renamed onto a nickname an active shared
    /// session already holds — usernames and active nicknames share one
    /// namespace, and the shared nickname is not a DB username so only the
    /// in-memory check catches it.
    #[tokio::test]
    async fn test_userupdate_rename_onto_active_shared_nickname_fails() {
        let mut test_ctx = create_test_context().await;
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let alice = test_ctx
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

        // Live shared session holding nickname "bob" (its username is shared_acct).
        let shared = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "shared_acct",
                hashed_password: "hash",
                is_admin: false,
                is_shared: true,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();
        test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                user_id: shared.id,
                username: "shared_acct".to_string(),
                is_admin: false,
                is_shared: true,
                permissions: HashSet::new(),
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
                bandwidth_weight_override: None,
                last_activity: std::time::Instant::now(),
            })
            .await
            .unwrap();

        let request = UserUpdateRequest {
            id: alice.id,
            current_password: None,
            username: Some("bob".to_string()),
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
        match read_server_message(&mut test_ctx).await {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(!success, "rename onto an active nickname must be rejected");
                assert!(error.is_some());
            }
            _ => panic!("Expected UserUpdateResponse"),
        }
        assert!(
            test_ctx
                .db
                .users
                .get_user_by_username("alice")
                .await
                .unwrap()
                .is_some(),
            "alice must keep her original name"
        );
    }

    /// The active-nickname collision is case-insensitive: renaming onto "BOB"
    /// while a session holds "bob" must still be rejected.
    #[tokio::test]
    async fn test_userupdate_rename_onto_active_shared_nickname_case_insensitive_fails() {
        let mut test_ctx = create_test_context().await;
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let alice = test_ctx
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

        let shared = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "shared_acct",
                hashed_password: "hash",
                is_admin: false,
                is_shared: true,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();
        test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                user_id: shared.id,
                username: "shared_acct".to_string(),
                is_admin: false,
                is_shared: true,
                permissions: HashSet::new(),
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
                bandwidth_weight_override: None,
                last_activity: std::time::Instant::now(),
            })
            .await
            .unwrap();

        let request = UserUpdateRequest {
            id: alice.id,
            current_password: None,
            username: Some("BOB".to_string()),
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
        match read_server_message(&mut test_ctx).await {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(
                    !success,
                    "case-insensitive nickname collision must be rejected"
                );
                assert!(error.is_some());
            }
            _ => panic!("Expected UserUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_userupdate_rename_onto_active_shared_nickname_unicode_fails() {
        let mut test_ctx = create_test_context().await;
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let alice = test_ctx
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

        let shared = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "shared_acct",
                hashed_password: "hash",
                is_admin: false,
                is_shared: true,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();
        // Active shared session holding the nickname "Café"; renaming a regular
        // account onto "CAFÉ" collides only under the Unicode fold (É↔é), which
        // ASCII NOCASE would miss — exercises the is_nickname_in_use guard.
        test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                user_id: shared.id,
                username: "shared_acct".to_string(),
                is_admin: false,
                is_shared: true,
                permissions: HashSet::new(),
                address: test_ctx.peer_addr,
                created_at: 0,
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: DEFAULT_TEST_LOCALE.to_string(),
                avatar: None,
                nickname: "Café".to_string(),
                is_away: false,
                status: None,
                group_id: None,
                group_name: None,
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
                bandwidth_weight_override: None,
                last_activity: std::time::Instant::now(),
            })
            .await
            .unwrap();

        let request = UserUpdateRequest {
            id: alice.id,
            current_password: None,
            username: Some("CAFÉ".to_string()),
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
        match read_server_message(&mut test_ctx).await {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(!success, "Unicode nickname collision must be rejected");
                assert!(error.is_some());
            }
            _ => panic!("Expected UserUpdateResponse"),
        }
    }

    /// The active-nickname collision guard must not fire when a username is
    /// resubmitted unchanged (a common form-submit shape, where every field is
    /// sent back). The guard is gated on a real (case-insensitive) change, so
    /// admin re-sending its own username while changing another field — with
    /// its own session holding that nickname — still succeeds. Without the
    /// gate, `is_nickname_in_use("admin")` would match admin's own session and
    /// falsely reject.
    #[tokio::test]
    async fn test_userupdate_unchanged_username_not_blocked_by_own_nickname() {
        let mut test_ctx = create_test_context().await;
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

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
            username: Some("admin".to_string()),
            password: None,
            is_admin: None,
            enabled: None,
            permissions: None,
            group_id: None,
            remove_group: None,
            revokes: None,
            bandwidth_weight: Some(42),
            inherit_bandwidth_weight: None,
            session_id: Some(admin_session),
        };
        let result = handle_user_update(request, &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(
                    success,
                    "unchanged-username resubmit must not be self-rejected: {error:?}"
                );
            }
            _ => panic!("Expected UserUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_userupdate_change_password() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

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

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

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

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

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

        let alice = test_ctx
            .db
            .users
            .get_user_by_username("alice")
            .await
            .unwrap()
            .unwrap();

        // Bob (no user_info/chat_send) sets Alice's perms to just user_list. The
        // perms he doesn't own must survive — only his owned set is touched.
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

        // user_list set by Bob; user_info + chat_send preserved (Bob can't touch them).
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

        // Send both grants and revokes. Pre-fix, set_permissions_in_tx deleted all
        // rows (incl. revokes just written) then re-inserted only grants, losing revokes.
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

        let _admin3_session = login_user(&mut test_ctx, "admin3", "password", &[], true).await;

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

        // Demote admin2 and admin3 so admin1 is the only admin.
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

        // admin1 is now the only admin; self-edit is blocked, so test the DB
        // last-admin protection directly.
        let admin1 = test_ctx
            .db
            .users
            .get_user_by_username("admin1")
            .await
            .unwrap()
            .unwrap();

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

        let _admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

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
                bandwidth_weight_override: None,
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

        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

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

        assert!(bob.enabled, "Bob should be enabled initially");

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

        let bob_after = test_ctx
            .db
            .users
            .get_user_by_username("bob")
            .await
            .unwrap()
            .unwrap();
        assert!(!bob_after.enabled, "Bob should be disabled");

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

        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let bob_session = login_user(&mut test_ctx, "bob", "password", &[], false).await;

        assert!(
            test_ctx
                .user_manager
                .get_user_by_session_id(bob_session)
                .await
                .is_some(),
            "Bob should be in user manager"
        );

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

        assert!(
            test_ctx
                .user_manager
                .get_user_by_session_id(bob_session)
                .await
                .is_none(),
            "Bob should be removed from user manager after being disabled"
        );
    }

    /// Naming the same permission as both grant and revoke is rejected upfront
    /// with `err_permission_grant_revoke_conflict` (the `(user_id, permission)`
    /// PK can't hold both). The target is grouped so revokes parse to `Some` —
    /// the handler drops revokes for ungrouped users.
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

        // No partial write: Bob still has zero override rows (group provides ChatSend).
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

    /// Regression: a `UserUpdate` that renames AND disables must still disconnect.
    /// Pre-fix the cache rename ran after the disable lookup, so the lookup missed
    /// the session under the new name and the disabled user stayed online.
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

    /// Regression: demoting an in-voice admin must kick them from voice even with
    /// no explicit `VoiceListen` grant. Admins hold it via the has_permission
    /// bypass; the old check read only stored grants, saw `had_voice_listen =
    /// false`, and left the demoted admin receiving relayed audio.
    #[tokio::test]
    async fn test_userupdate_demoted_admin_loses_voice() {
        use std::collections::HashSet;

        let mut test_ctx = create_test_context().await;

        // Requester stays admin so the demotion isn't blocked by last-admin protection.
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Demotion target: no explicit VoiceListen grant, relies on the admin bypass.
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

        // Seed as admins are at login: is_admin true, empty perm set (bypass lives
        // in has_permission, not the set).
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
                bandwidth_weight_override: None,
                last_activity: std::time::Instant::now(),
            })
            .await
            .expect("Failed to add voiceadmin session");

        // Put voiceadmin in voice.
        let voice_session = crate::voice::VoiceSession::new(
            "voiceadmin".to_string(),
            vec!["#general".to_string()],
            voiceadmin_session,
        );
        test_ctx
            .voice_registry
            .add(voice_session)
            .await
            .expect("test setup: session_id is unique");
        assert!(
            test_ctx
                .voice_registry
                .has_session(voiceadmin_session)
                .await,
            "voiceadmin must start out in the voice registry"
        );

        // Demote with no granted permissions, so `final_permissions` resolves to
        // empty — the exact scenario the admin-bypass-aware check must catch.
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

        // Voice cleanup must have fired (pre-fix `had_voice_listen` saw the empty
        // stored grants as false and skipped it).
        assert!(
            !test_ctx
                .voice_registry
                .has_session(voiceadmin_session)
                .await,
            "demoted admin must be kicked from voice (admin bypass meant they were effectively voiced)"
        );

        // Cache flip is visible too: the demoted session is no longer is_admin.
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

    /// Regression: renaming a SHARED account must not rewrite the voice-registry
    /// nickname of its in-voice sessions. Shared accounts keep per-session
    /// login-chosen nicknames; the handler's voice-registry update lacked the gate
    /// `update_username` has, so the voice nickname flipped to the account name.
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

        // Nickname differs from the account name: the chosen handle must survive rename.
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
                bandwidth_weight_override: None,
                last_activity: std::time::Instant::now(),
            })
            .await
            .expect("Failed to add shared session");

        // Put that session in voice under the chosen nickname.
        let voice_session = crate::voice::VoiceSession::new(
            "vibes".to_string(),
            vec!["#general".to_string()],
            lounge_session,
        );
        test_ctx
            .voice_registry
            .add(voice_session)
            .await
            .expect("test setup: session_id is unique");

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

        // Voice nickname unchanged: renaming the account shouldn't move "vibes".
        let voice_after = test_ctx
            .voice_registry
            .get_by_session_id(lounge_session)
            .await
            .expect("voice session must still exist after rename");
        assert_eq!(
            voice_after.nickname, "vibes",
            "shared-account rename must not rewrite the per-session voice nickname"
        );

        // Sanity: the UserManager session's nickname is also unchanged.
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
                bandwidth_weight_override: None,
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
                bandwidth_weight_override: None,
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

        // Give admin2 user_edit, then have them try to demote admin1.
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

        // Goes straight to the DB (bypassing the handler's admin-status check) to
        // exercise the atomic last-admin SQL protection in isolation.
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

        assert!(result.is_ok());
        assert!(
            matches!(result.unwrap(), db::UpdateUserResult::Blocked),
            "Database should block demoting last admin atomically"
        );

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

    #[tokio::test]
    async fn test_userupdate_shared_user_cannot_self_edit() {
        let mut test_ctx = create_test_context().await;

        let _admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

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

        let admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

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

        let admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

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
                bandwidth_weight_override: None,
                last_activity: std::time::Instant::now(),
            })
            .await
            .unwrap();

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
                bandwidth_weight_override: None,
                last_activity: std::time::Instant::now(),
            })
            .await
            .unwrap();

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
                bandwidth_weight_override: None,
                last_activity: std::time::Instant::now(),
            })
            .await
            .unwrap();

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
                bandwidth_weight_override: None,
                last_activity: std::time::Instant::now(),
            })
            .await
            .unwrap();

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

        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

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
                bandwidth_weight_override: None,
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

        let _ = bob_session;
    }

    #[tokio::test]
    async fn test_userupdate_permissions_updated_sent_when_changed() {
        let mut test_ctx = create_test_context().await;

        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

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
                bandwidth_weight_override: None,
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
            .expect("Should receive PermissionsUpdated")
            .expect_message();
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

        let _ = bob_session;
    }

    #[tokio::test]
    async fn test_userupdate_no_permissions_updated_for_password_only_change() {
        let mut test_ctx = create_test_context().await;

        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

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
                bandwidth_weight_override: None,
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

        let _ = bob_session;
    }

    #[tokio::test]
    async fn test_userupdate_permissions_updated_sent_when_admin_status_changes() {
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
                bandwidth_weight_override: None,
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
            .expect("Should receive PermissionsUpdated when admin status changes")
            .expect_message();
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

        let _ = bob_session;
    }

    #[tokio::test]
    async fn test_userupdate_permissions_updated_sent_when_enabled_status_changes() {
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
                bandwidth_weight_override: None,
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

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse { success, .. } => {
                assert!(success);
            }
            _ => panic!("Expected UserUpdateResponse, got {:?}", response),
        }

        // PermissionsUpdated is broadcast for the enabled-status change before the
        // disconnect Error, so we should receive it.
        let (msg, _) = test_ctx
            .rx
            .recv()
            .await
            .expect("Should receive PermissionsUpdated when enabled status changes")
            .expect_message();
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

        let _ = bob_session;
    }

    #[tokio::test]
    async fn test_userupdate_no_permissions_updated_when_admin_status_unchanged() {
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
                bandwidth_weight_override: None,
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

        let _ = bob_session;
    }

    #[tokio::test]
    async fn test_userupdate_voice_listen_revoked_kicks_from_voice() {
        use std::collections::HashSet;

        let mut test_ctx = create_test_context().await;

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

        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

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
                bandwidth_weight_override: None,
                last_activity: std::time::Instant::now(),
            })
            .await
            .expect("Failed to add voice user session");

        let _ = test_ctx
            .channel_manager
            .join("#general", voice_user_session, JoinPolicy::CreateIfMissing)
            .await;

        let voice_session = crate::voice::VoiceSession::new(
            "voiceuser".to_string(),
            vec!["#general".to_string()],
            voice_user_session,
        );
        test_ctx
            .voice_registry
            .add(voice_session)
            .await
            .expect("test setup: session_id is unique");

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

        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

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
                bandwidth_weight_override: None,
                last_activity: std::time::Instant::now(),
            })
            .await
            .expect("Failed to add voice user session");

        let _ = test_ctx
            .channel_manager
            .join("#general", voice_user_session, JoinPolicy::CreateIfMissing)
            .await;

        let voice_session = crate::voice::VoiceSession::new(
            "voiceuser".to_string(),
            vec!["#general".to_string()],
            voice_user_session,
        );
        test_ctx
            .voice_registry
            .add(voice_session)
            .await
            .expect("test setup: session_id is unique");

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

        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

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

        let bob = test_ctx
            .db
            .users
            .get_user_by_username("bob")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(bob.group_id, None);

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

        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

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

        let bob = test_ctx
            .db
            .users
            .get_user_by_username("bob")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(bob.group_id, Some(group.id));

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

        // High-weight group, same permission set, so only the weight rule should reject.
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

    /// Log a non-admin in and move them into a group at `requester_weight` so
    /// their cached session weight matches. Returns (admin_session,
    /// editor_session, editor_id, editor_group_id).
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

    /// Regression: inherit-delegation must resolve against the POST-update group.
    /// `{remove_group: true, inherit: true}` on a user in a high-weight group will
    /// inherit DEFAULT after removal, so judging by the old group falsely rejects a
    /// request that actually lowers the user.
    #[tokio::test]
    async fn test_userupdate_inherit_delegation_uses_post_update_group_on_remove() {
        let mut test_ctx = create_test_context().await;
        let (_admin_session, editor_session, _editor_id, _editor_group_id) =
            setup_editor_with_weight(&mut test_ctx, 25).await;

        // Bob in a weight-100 group with override 75; editor (25) sends
        // {remove_group, inherit}. Post-update effective = DEFAULT (1). Pre-fix
        // rejected because the resolver used the old group (100).
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

    /// Regression companion: same `inherit: true`, but the proposed NEW group also
    /// exceeds the requester's weight, so the check must reject on the new group's
    /// weight, not the old. Values old < requester < new exercise it cleanly: the
    /// new check rejects where the old check would have allowed.
    #[tokio::test]
    async fn test_userupdate_inherit_delegation_uses_post_update_group_on_assign() {
        let mut test_ctx = create_test_context().await;
        let (_admin_session, editor_session, _editor_id, _editor_group_id) =
            setup_editor_with_weight(&mut test_ctx, 25).await;

        // Bob in a weight-5 group (below editor's 25); editor sends
        // {group_id: high(100), inherit: true}. Pre-fix the resolver used the old
        // group (5) and passed; the separate new-group check would catch it, but the
        // inherit check should reject too — standalone defense-in-depth.
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
        // Promotion must refresh the cached bandwidth_weight (admins resolve to
        // DEFAULT_ADMIN_BANDWIDTH_WEIGHT); otherwise the scheduler reads the stale value.
        use std::sync::atomic::Ordering;

        let mut test_ctx = create_test_context().await;
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Non-admin login: cached weight starts at default 1.
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
        // Promotion auto-clears group_id in the DB; the session cache must follow,
        // or get_sessions_by_group_id keeps returning this admin until re-login.
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

        // Bob logs in non-admin, then is assigned to Staff (DB + session cache).
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

    /// Invariant: a `UserUpdate` touching multiple UserInfo-visible fields produces
    /// exactly one `UserUpdated` per receiver, not one per field (mirrors
    /// group_update.rs). Catches a future refactor that splits into per-field emits.
    #[tokio::test]
    async fn test_userupdate_one_broadcast_per_receiver_for_combined_field_change() {
        let mut test_ctx = create_test_context().await;

        let _admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Bob is an online session holding user_list (a valid broadcast target);
        // admin and bob share test_ctx.tx, so one broadcast lands twice. UserList
        // must be granted in the DB — user_update re-syncs cached perms from the DB
        // mid-handler, so a session-cache-only grant would be wiped first.
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
                bandwidth_weight_override: None,
                last_activity: std::time::Instant::now(),
            })
            .await
            .unwrap();

        // username + bandwidth_weight in one call: both trigger the broadcast, which
        // must still fire once. Avoiding is_admin/group/perms keeps PermissionsUpdated
        // (a separate message) out of the queue.
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

        // First message is the UserUpdateResponse to admin; drain the rest and
        // count UserUpdated for "robert".
        let _response = read_server_message(&mut test_ctx).await;

        let mut user_updated_count = 0;
        let mut other_msgs = Vec::new();
        while let Ok(event) = test_ctx.rx.try_recv() {
            let (msg, _) = event.expect_message();
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

    /// Regression: a renamed regular account that holds neither `user_list` nor any
    /// channel membership must still learn of its own rename. `UserUpdated` is
    /// `user_list`-gated and `ChatUserRenamed` only fans out through channels, so
    /// such a session would otherwise keep a stale cached identity (breaking
    /// self-message detection). The handler must direct-send the `UserUpdated` to the
    /// account's own non-`user_list` sessions.
    #[tokio::test]
    async fn test_userupdate_rename_reaches_own_non_userlist_session() {
        let mut test_ctx = create_test_context().await;

        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Alice: regular account, NO user_list (DB + session), in NO channels.
        let alice = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "alice",
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

        // Alice's session on its OWN channel so we can assert exactly what she gets.
        let (alice_tx, mut alice_rx) = ConnectionWriter::channel();
        test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                user_id: alice.id,
                username: "alice".to_string(),
                is_admin: false,
                is_shared: false,
                permissions: HashSet::new(),
                address: test_ctx.peer_addr,
                created_at: 0,
                tx: alice_tx,
                features: vec![],
                locale: DEFAULT_TEST_LOCALE.to_string(),
                avatar: None,
                nickname: "alice".to_string(),
                is_away: false,
                status: None,
                group_id: None,
                group_name: None,
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
                bandwidth_weight_override: None,
                last_activity: std::time::Instant::now(),
            })
            .await
            .unwrap();

        let request = UserUpdateRequest {
            id: alice.id,
            current_password: None,
            username: Some("alicia".to_string()),
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
        handle_user_update(request, &mut test_ctx.handler_context())
            .await
            .unwrap();

        // Alice's own session — lacking user_list and channels — must still receive a
        // UserUpdated carrying the rename. Without the direct-send it gets nothing.
        let mut found = false;
        while let Ok(event) = alice_rx.try_recv() {
            let (msg, _) = event.expect_message();
            if let ServerMessage::UserUpdated {
                previous_username,
                user,
            } = msg
            {
                assert_eq!(previous_username, "alice");
                assert_eq!(user.username, "alicia");
                assert_eq!(user.id, alice.id);
                found = true;
            }
        }
        assert!(
            found,
            "renamed regular account's own non-user_list session must receive a UserUpdated"
        );
    }

    /// Regression: renaming a *shared* account's username must still reach the
    /// account's own sessions that lack `user_list`. Their per-session nickname is
    /// unchanged, but the account username they carry changes; `UserUpdated` is
    /// `user_list`-gated and `ChatUserRenamed` is regular-only, so without a direct-send
    /// such a session keeps a stale cached identity. The handler must direct-send the
    /// per-session `UserUpdated`.
    #[tokio::test]
    async fn test_userupdate_shared_rename_reaches_own_non_userlist_session() {
        let mut test_ctx = create_test_context().await;

        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Shared account, NO user_list, with one session whose nickname differs from the
        // account username.
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

        // The shared session on its OWN channel so we can assert exactly what it gets.
        let (member_tx, mut member_rx) = ConnectionWriter::channel();
        test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                user_id: shared.id,
                username: "shared_acct".to_string(),
                is_admin: false,
                is_shared: true,
                permissions: HashSet::new(),
                address: test_ctx.peer_addr,
                created_at: 0,
                tx: member_tx,
                features: vec![],
                locale: DEFAULT_TEST_LOCALE.to_string(),
                avatar: None,
                nickname: "Member1".to_string(),
                is_away: false,
                status: None,
                group_id: None,
                group_name: None,
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
                bandwidth_weight_override: None,
                last_activity: std::time::Instant::now(),
            })
            .await
            .unwrap();

        let request = UserUpdateRequest {
            id: shared.id,
            current_password: None,
            username: Some("shared_acct2".to_string()),
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
        handle_user_update(request, &mut test_ctx.handler_context())
            .await
            .unwrap();

        // The shared session — lacking user_list — must receive a per-session UserUpdated
        // carrying the new account username while keeping its own nickname.
        let mut found = false;
        while let Ok(event) = member_rx.try_recv() {
            let (msg, _) = event.expect_message();
            if let ServerMessage::UserUpdated {
                previous_username,
                user,
            } = msg
            {
                assert_eq!(previous_username, "shared_acct");
                assert_eq!(user.username, "shared_acct2");
                assert_eq!(user.id, shared.id);
                assert!(user.is_shared);
                assert_eq!(
                    user.nickname, "Member1",
                    "shared session keeps its per-session nickname across a username rename"
                );
                assert_eq!(
                    user.session_ids.len(),
                    1,
                    "per-session UserUpdated carries only its own session id"
                );
                found = true;
            }
        }
        assert!(
            found,
            "renamed shared account's own non-user_list session must receive a per-session UserUpdated"
        );
    }

    /// Regression: a `UserUpdated` for a *shared* account must fan out one message
    /// per session (each carrying that session's nickname and its single session
    /// id), not one aggregated message with every session id — otherwise the client
    /// stamps all ids onto every shared entry and mis-decrements on disconnect.
    #[tokio::test]
    async fn test_userupdate_shared_account_fans_out_per_session() {
        let mut test_ctx = create_test_context().await;

        // Admin observer (holds user_list via admin bypass), on the shared test tx.
        let _admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;
        let admin_session = test_ctx
            .user_manager
            .get_sessions_by_username("admin")
            .await
            .first()
            .map(|s| s.session_id);

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

        // Two live sessions under the shared account, with distinct nicknames.
        for nick in ["alice", "bob"] {
            test_ctx
                .user_manager
                .add_user(NewSessionParams {
                    session_id: 0,
                    user_id: shared.id,
                    username: "shared_acct".to_string(),
                    is_admin: false,
                    is_shared: true,
                    permissions: HashSet::new(),
                    address: test_ctx.peer_addr,
                    created_at: 0,
                    tx: test_ctx.tx.clone(),
                    features: vec![],
                    locale: DEFAULT_TEST_LOCALE.to_string(),
                    avatar: None,
                    nickname: nick.to_string(),
                    is_away: false,
                    status: None,
                    group_id: None,
                    group_name: None,
                    bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
                    bandwidth_weight_override: None,
                    last_activity: std::time::Instant::now(),
                })
                .await
                .unwrap();
        }

        // A bandwidth-weight change (allowed for shared) fires the broadcast.
        let request = UserUpdateRequest {
            id: shared.id,
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
            session_id: admin_session,
        };
        handle_user_update(request, &mut test_ctx.handler_context())
            .await
            .unwrap();

        // First message is the UserUpdateResponse to admin; the broadcasts follow.
        let _response = read_server_message(&mut test_ctx).await;

        let mut nicknames = Vec::new();
        while let Ok(event) = test_ctx.rx.try_recv() {
            let (msg, _) = event.expect_message();
            if let ServerMessage::UserUpdated { user, .. } = msg
                && user.id == shared.id
            {
                assert!(user.is_shared, "shared update must keep is_shared");
                assert_eq!(
                    user.session_ids.len(),
                    1,
                    "each shared per-session UserUpdated carries only its own session id"
                );
                assert_eq!(user.bandwidth_weight, Some(42));
                nicknames.push(user.nickname);
            }
        }
        nicknames.sort();
        assert_eq!(
            nicknames,
            vec!["alice".to_string(), "bob".to_string()],
            "expected one UserUpdated per shared session (per nickname), not one aggregated"
        );
    }

    /// Regression: with both `bandwidth_weight: Some(N)` and `inherit: Some(true)`,
    /// inherit wins in the DB (override cleared to NULL regardless of N). The
    /// broadcast detector must mirror that, or an N equal to the stored value would
    /// suppress a broadcast even though the effective weight changed.
    #[tokio::test]
    async fn test_userupdate_broadcasts_when_inherit_wins_over_matching_explicit() {
        let mut test_ctx = create_test_context().await;

        let _admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Bob has a per-user override of 50; UserList in DB so the broadcast reaches him.
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
                bandwidth_weight_override: None,
                last_activity: std::time::Instant::now(),
            })
            .await
            .unwrap();

        // Both fields present, bandwidth_weight matching bob's stored value. Inherit
        // wins → override cleared → effective weight drops to baseline 1.
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

        // Expect a UserUpdated for bob at the new effective weight (DEFAULT = 1).
        let mut saw_broadcast = false;
        while let Ok(event) = test_ctx.rx.try_recv() {
            let (msg, _) = event.expect_message();
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

    /// Handler enforcement of admin XOR shared: promoting a shared account to
    /// admin is rejected before any DB write (the schema CHECK is the safety net;
    /// this pins the translated error path on top).
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

    // Self-edit gate matrix: the gate's five sequential checkpoints, one outcome
    // class per test. `assert_self_edit_outcome` sets up the user, runs the
    // handler with a per-case request builder, and asserts the result.

    enum SelfEditOutcome {
        Success,
        Error(String),
    }

    /// Per-case request builder for the gate-matrix tests; aliased to dodge
    /// `type_complexity`.
    type SelfEditRequestBuilder = fn(i64, Option<u32>) -> UserUpdateRequest;

    /// Run a self-edit and assert the outcome. `build_request` gets the user's id
    /// and session_id and returns a `UserUpdateRequest` targeting self.
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

    /// A UserUpdateRequest with all fields None, for tests to fill one per case.
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

    /// Gate checkpoint 1: shared accounts are blocked from self-edit before any
    /// field check, so one case suffices.
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

    /// Gate checkpoint 2: the "forbidden on self" field set is blocked even for admins.
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

    /// Gate checkpoint 2 mirror: forbidden fields are also rejected for non-admins
    /// (the check ignores `requesting_is_admin`).
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

    // No-op broadcast suppression: a bandwidth-only update skips `UserUpdated` when
    // the effective weight didn't move (pre vs post resolved value). Cross-trigger
    // updates bypass suppression since the other field is visible on its own.

    /// `UserUpdated` is presence-only: a bandwidth-only update to an OFFLINE
    /// account (no live sessions) broadcasts nothing — there's no live entry to
    /// refresh and no reconnect reconciliation. Observers learn the change at the
    /// target's next login (via the snapshot), not from a delta.
    #[tokio::test]
    async fn test_userupdate_offline_account_does_not_broadcast() {
        let mut test_ctx = create_test_context().await;

        let _admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Bob in DB with no active session — offline.
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
        while let Ok(event) = test_ctx.rx.try_recv() {
            let (msg, _) = event.expect_message();
            if let ServerMessage::UserUpdated { user, .. } = msg
                && user.username == "bob"
            {
                saw_broadcast = true;
            }
        }
        assert!(
            !saw_broadcast,
            "offline-account update must not broadcast UserUpdated (presence-only)"
        );
    }

    /// Bandwidth-only update where the resolved value doesn't move → no broadcast.
    /// Non-admin, no group, override written equal to DEFAULT: the DB row changes
    /// (so the dirty bit fires) but resolved stays at DEFAULT, matching the cached
    /// value → suppression fires.
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
                bandwidth_weight_override: None,
                last_activity: std::time::Instant::now(),
            })
            .await
            .unwrap();

        // Override equal to the resolved value: DB row changes (dirty bit fires) but
        // resolved stays at DEFAULT — bob's cached value. Suppression must fire.
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
        while let Ok(event) = test_ctx.rx.try_recv() {
            let (msg, _) = event.expect_message();
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
        assert!(
            test_ctx.egress_settings_rx.try_recv().is_err(),
            "same-resolved bandwidth update should not emit an egress weight update"
        );
    }

    #[tokio::test]
    async fn test_userupdate_bandwidth_change_updates_egress_weight() {
        let mut test_ctx = create_test_context().await;

        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;
        let bob_session = login_user(&mut test_ctx, "bob", "password", &[], false).await;
        let bob = test_ctx
            .user_manager
            .get_user_by_session_id(bob_session)
            .await
            .unwrap();

        let request = UserUpdateRequest {
            id: bob.user_id,
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
            session_id: Some(admin_session),
        };
        handle_user_update(request, &mut test_ctx.handler_context())
            .await
            .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(success, "user update should succeed: {:?}", error);
            }
            other => panic!("Expected UserUpdateResponse, got {:?}", other),
        }

        match test_ctx.egress_settings_rx.try_recv() {
            Ok(EgressSettingsCommand::UpdateUserWeight { user_id, weight }) => {
                assert_eq!(user_id, bob.user_id);
                assert_eq!(weight, 200);
            }
            _ => panic!("Expected egress UpdateUserWeight command"),
        }
    }

    #[tokio::test]
    async fn test_userupdate_transfer_only_bandwidth_change_updates_egress_weight() {
        let mut test_ctx = create_test_context().await;

        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;
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
                bandwidth_weight: Some(50),
            })
            .await
            .unwrap();
        register_active_transfer(&mut test_ctx, bob.id, "bob", false);

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
            session_id: Some(admin_session),
        };
        handle_user_update(request, &mut test_ctx.handler_context())
            .await
            .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(success, "user update should succeed: {:?}", error);
            }
            other => panic!("Expected UserUpdateResponse, got {:?}", other),
        }

        match test_ctx.egress_settings_rx.try_recv() {
            Ok(EgressSettingsCommand::UpdateUserWeight { user_id, weight }) => {
                assert_eq!(user_id, bob.id);
                assert_eq!(weight, 200);
            }
            _ => panic!("Expected egress UpdateUserWeight command"),
        }
    }

    #[tokio::test]
    async fn test_userupdate_transfer_only_same_resolved_skips_egress_weight_update() {
        let mut test_ctx = create_test_context().await;

        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;
        let group = test_ctx
            .db
            .groups
            .create_group("Staff", false, &db::Permissions::new(), 50)
            .await
            .unwrap();
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
                group_id: Some(group.id),
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();
        register_active_transfer(&mut test_ctx, bob.id, "bob", false);

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
            bandwidth_weight: Some(50),
            inherit_bandwidth_weight: None,
            session_id: Some(admin_session),
        };
        handle_user_update(request, &mut test_ctx.handler_context())
            .await
            .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(success, "user update should succeed: {:?}", error);
            }
            other => panic!("Expected UserUpdateResponse, got {:?}", other),
        }

        assert!(
            test_ctx.egress_settings_rx.try_recv().is_err(),
            "same-resolved transfer-only update should not emit an egress weight update"
        );
    }

    #[tokio::test]
    async fn test_userupdate_inherit_bandwidth_updates_egress_to_group_weight() {
        let mut test_ctx = create_test_context().await;

        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;
        let group = test_ctx
            .db
            .groups
            .create_group("Staff", false, &db::Permissions::new(), 7)
            .await
            .unwrap();
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
                group_id: Some(group.id),
                revokes: &[],
                bandwidth_weight: Some(100),
            })
            .await
            .unwrap();
        let bob_session = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                user_id: bob.id,
                username: "bob".to_string(),
                is_admin: false,
                is_shared: false,
                permissions: HashSet::new(),
                address: test_ctx.peer_addr,
                created_at: 0,
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: DEFAULT_TEST_LOCALE.to_string(),
                avatar: None,
                nickname: "bob".to_string(),
                is_away: false,
                status: None,
                group_id: Some(group.id),
                group_name: Some("Staff".to_string()),
                bandwidth_weight: 100,
                bandwidth_weight_override: Some(100),
                last_activity: std::time::Instant::now(),
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
            session_id: Some(admin_session),
        };
        handle_user_update(request, &mut test_ctx.handler_context())
            .await
            .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(success, "user update should succeed: {:?}", error);
            }
            other => panic!("Expected UserUpdateResponse, got {:?}", other),
        }

        match test_ctx.egress_settings_rx.try_recv() {
            Ok(EgressSettingsCommand::UpdateUserWeight { user_id, weight }) => {
                assert_eq!(user_id, bob.id);
                assert_eq!(weight, 7);
            }
            _ => panic!("Expected egress UpdateUserWeight command"),
        }

        let updated_bob = test_ctx
            .user_manager
            .get_user_by_session_id(bob_session)
            .await
            .unwrap();
        assert_eq!(
            updated_bob
                .bandwidth_weight
                .load(std::sync::atomic::Ordering::Relaxed),
            7
        );
        assert_eq!(updated_bob.bandwidth_weight_override, None);
    }

    #[tokio::test]
    async fn test_userupdate_bandwidth_change_succeeds_when_egress_channel_closed() {
        let mut test_ctx = create_test_context().await;

        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;
        let bob_session = login_user(&mut test_ctx, "bob", "password", &[], false).await;
        let bob = test_ctx
            .user_manager
            .get_user_by_session_id(bob_session)
            .await
            .unwrap();
        test_ctx.egress_settings_rx.close();

        let request = UserUpdateRequest {
            id: bob.user_id,
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
            session_id: Some(admin_session),
        };
        handle_user_update(request, &mut test_ctx.handler_context())
            .await
            .unwrap();

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserUpdateResponse { success, error, .. } => {
                assert!(
                    success,
                    "egress failure must not fail user update: {:?}",
                    error
                );
            }
            other => panic!("Expected UserUpdateResponse, got {:?}", other),
        }
    }

    /// Suppression must NOT fire when a non-bandwidth field also changed: same
    /// no-op bandwidth setup as above, but combined with a username rename — a
    /// visible change, so the broadcast fires regardless.
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
                bandwidth_weight_override: None,
                last_activity: std::time::Instant::now(),
            })
            .await
            .unwrap();

        // rename + bandwidth override resolving to the same value: the rename keeps
        // the broadcast firing despite the no-op bandwidth side.
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
        while let Ok(event) = test_ctx.rx.try_recv() {
            let (msg, _) = event.expect_message();
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
