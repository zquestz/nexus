//! GroupUpdate message handler

use std::collections::HashSet;
use std::io;
use std::sync::atomic::Ordering;

use tokio::io::AsyncWrite;
use tracing::{error, info, warn};

use nexus_common::is_shared_account_permission;
use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{
    self, BandwidthWeightError, GroupNameError, MIN_BANDWIDTH_WEIGHT, PermissionsError,
    validate_bandwidth_weight,
};

#[cfg(test)]
use super::testing::DEFAULT_TEST_LOCALE;
use super::{
    HandlerContext, ServerInfoOptions, ServerInfoValues, build_server_info, err_authentication,
    err_bandwidth_weight_delegation, err_bandwidth_weight_zero, err_database,
    err_group_already_exists, err_group_name_empty, err_group_name_invalid,
    err_group_name_too_long, err_group_no_fields, err_group_not_empty_modify, err_group_not_found,
    err_group_shared_permission, err_not_logged_in, err_permission_denied,
    err_permissions_contains_newlines, err_permissions_empty_permission,
    err_permissions_invalid_characters, err_permissions_permission_too_long,
    err_permissions_too_many, err_unknown_permission,
};
use crate::constants::*;
use crate::db::{Permission, Permissions};
use crate::users::manager::UserManager;
use crate::users::user::UserSession;
use crate::voice::send_voice_leave_notifications;

/// Handle a group update request
pub async fn handle_group_update<W>(
    id: i64,
    name: Option<String>,
    is_shared: Option<bool>,
    permissions: Option<Vec<String>>,
    bandwidth_weight: Option<u16>,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    // Verify authentication
    let Some(session_id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_GROUP_UPDATE_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_GROUP_UPDATE))
            .await;
    };

    // Get requesting user from session
    let requesting_user = match ctx.user_manager.get_user_by_session_id(session_id).await {
        Some(u) => u,
        None => {
            return ctx
                .send_error_and_disconnect(
                    &err_authentication(ctx.locale),
                    Some(HANDLER_GROUP_UPDATE),
                )
                .await;
        }
    };

    // Check GroupEdit permission
    if !requesting_user.has_permission(Permission::GroupEdit) {
        warn!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_GROUP_UPDATE_PERMISSION_DENIED);
        let response = ServerMessage::GroupUpdateResponse {
            success: false,
            id: None,
            name: None,
            error: Some(err_permission_denied(ctx.locale)),
        };
        return ctx.send_message(&response).await;
    }

    // If all optional fields are None, there's nothing to update
    if name.is_none() && is_shared.is_none() && permissions.is_none() && bandwidth_weight.is_none()
    {
        let response = ServerMessage::GroupUpdateResponse {
            success: false,
            id: None,
            name: None,
            error: Some(err_group_no_fields(ctx.locale)),
        };
        return ctx.send_message(&response).await;
    }

    // Validate name format (if provided)
    if let Some(ref n) = name
        && let Err(e) = validators::validate_group_name(n)
    {
        let error_msg = match e {
            GroupNameError::Empty => err_group_name_empty(ctx.locale),
            GroupNameError::TooLong => {
                err_group_name_too_long(ctx.locale, validators::MAX_GROUP_NAME_LENGTH)
            }
            GroupNameError::InvalidCharacters => err_group_name_invalid(ctx.locale),
        };
        let response = ServerMessage::GroupUpdateResponse {
            success: false,
            id: None,
            name: None,
            error: Some(error_msg),
        };
        return ctx.send_message(&response).await;
    }

    // Validate permissions format (if provided)
    if let Some(ref perms) = permissions
        && let Err(e) = validators::validate_permissions(perms)
    {
        let error_msg = match e {
            PermissionsError::TooMany => {
                err_permissions_too_many(ctx.locale, nexus_common::PERMISSIONS_COUNT)
            }
            PermissionsError::EmptyPermission => err_permissions_empty_permission(ctx.locale),
            PermissionsError::PermissionTooLong => {
                err_permissions_permission_too_long(ctx.locale, validators::MAX_PERMISSION_LENGTH)
            }
            PermissionsError::ContainsNewlines => err_permissions_contains_newlines(ctx.locale),
            PermissionsError::InvalidCharacters => err_permissions_invalid_characters(ctx.locale),
        };
        let response = ServerMessage::GroupUpdateResponse {
            success: false,
            id: None,
            name: None,
            error: Some(error_msg),
        };
        return ctx.send_message(&response).await;
    }

    // Fetch existing group
    let group = match ctx.db.groups.get_group_by_id(id).await {
        Ok(Some(g)) => g,
        Ok(None) => {
            let response = ServerMessage::GroupUpdateResponse {
                success: false,
                id: None,
                name: None,
                error: Some(err_group_not_found(ctx.locale)),
            };
            return ctx.send_message(&response).await;
        }
        Err(e) => {
            error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_GROUP_UPDATE_DB_ERROR);
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some(HANDLER_GROUP_UPDATE))
                .await;
        }
    };

    // Fetch current permissions once and reuse everywhere
    let current_permissions: Vec<Permission> = match ctx.db.groups.get_group_permissions(id).await {
        Ok(p) => p,
        Err(e) => {
            error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_GROUP_UPDATE_DB_ERROR);
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some(HANDLER_GROUP_UPDATE))
                .await;
        }
    };

    // Shared status toggle check: if changing is_shared, group must have no members
    if let Some(new_shared) = is_shared
        && new_shared != group.is_shared
    {
        let member_count = match ctx.db.groups.get_member_count(id).await {
            Ok(c) => c,
            Err(e) => {
                error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_GROUP_UPDATE_DB_ERROR);
                return ctx
                    .send_error_and_disconnect(
                        &err_database(ctx.locale),
                        Some(HANDLER_GROUP_UPDATE),
                    )
                    .await;
            }
        };

        if member_count > 0 {
            let response = ServerMessage::GroupUpdateResponse {
                success: false,
                id: None,
                name: None,
                error: Some(err_group_not_empty_modify(ctx.locale)),
            };
            return ctx.send_message(&response).await;
        }
    }

    // Build final values
    let final_name = name.unwrap_or_else(|| group.name.clone());
    let final_is_shared = is_shared.unwrap_or(group.is_shared);

    // Resolve final permissions
    let final_permissions_vec: Vec<Permission> = if let Some(ref requested_perms) = permissions {
        // Parse and validate each permission string
        let mut parsed_requested: Vec<Permission> = Vec::new();
        for perm_str in requested_perms {
            match Permission::parse(perm_str) {
                Some(perm) => parsed_requested.push(perm),
                None => {
                    let response = ServerMessage::GroupUpdateResponse {
                        success: false,
                        id: None,
                        name: None,
                        error: Some(err_unknown_permission(ctx.locale, perm_str)),
                    };
                    return ctx.send_message(&response).await;
                }
            }
        }

        // Non-admins: validate requested permissions, then merge with
        // current group permissions the requester can't control
        if !requesting_user.is_admin {
            // Reject if requesting a permission the editor doesn't hold
            for perm in &parsed_requested {
                if !requesting_user.has_permission(*perm) {
                    warn!(user = %requesting_user.username, ip = %ctx.peer_addr, perm = %perm.as_str(), "{}", LOG_GROUP_UPDATE_UNOWNED_PERMISSION);
                    let response = ServerMessage::GroupUpdateResponse {
                        success: false,
                        id: None,
                        name: None,
                        error: Some(err_permission_denied(ctx.locale)),
                    };
                    return ctx.send_message(&response).await;
                }
            }

            // Merge: preserve current group permissions the requester can't
            // control, then layer in the requested changes
            let mut merged = Vec::new();
            for perm in &current_permissions {
                if !requesting_user.has_permission(*perm) {
                    merged.push(*perm);
                }
            }
            for perm in parsed_requested {
                if !merged.contains(&perm) {
                    merged.push(perm);
                }
            }
            merged
        } else {
            parsed_requested
        }
    } else {
        // No permission changes requested — pass through current permissions unchanged
        current_permissions.clone()
    };

    // Shared group permission validation: if final group is shared, all permissions must be allowed
    if final_is_shared {
        for perm in &final_permissions_vec {
            if !is_shared_account_permission(perm.as_str()) {
                let response = ServerMessage::GroupUpdateResponse {
                    success: false,
                    id: None,
                    name: None,
                    error: Some(err_group_shared_permission(ctx.locale)),
                };
                return ctx.send_message(&response).await;
            }
        }
    }

    // Capture old state for diff detection
    let old_name = group.name.clone();
    let old_bandwidth_weight = group.bandwidth_weight;
    let old_permissions: HashSet<Permission> = current_permissions.into_iter().collect();

    let final_permissions: Permissions = Permissions::from(final_permissions_vec.as_slice());

    // Bandwidth weight delegation: a non-admin updating a group can set
    // its weight only at or below their own resolved bandwidth weight.
    // Admins bypass.
    if !requesting_user.is_admin
        && let Some(w) = bandwidth_weight
        && w > requesting_user.bandwidth_weight.load(Ordering::Relaxed)
    {
        let response = ServerMessage::GroupUpdateResponse {
            success: false,
            id: None,
            name: None,
            error: Some(err_bandwidth_weight_delegation(ctx.locale)),
        };
        return ctx.send_message(&response).await;
    }
    if let Some(w) = bandwidth_weight
        && let Err(BandwidthWeightError::Zero) = validate_bandwidth_weight(w)
    {
        let response = ServerMessage::GroupUpdateResponse {
            success: false,
            id: None,
            name: None,
            error: Some(err_bandwidth_weight_zero(ctx.locale, MIN_BANDWIDTH_WEIGHT)),
        };
        return ctx.send_message(&response).await;
    }
    let final_bandwidth_weight = bandwidth_weight.unwrap_or(group.bandwidth_weight);

    // Update group in database
    match ctx
        .db
        .groups
        .update_group(
            id,
            &final_name,
            final_is_shared,
            &final_permissions,
            final_bandwidth_weight,
        )
        .await
    {
        Ok(Some(crate::db::UpdateGroupResult {
            group: updated_group,
            inheriting_member_ids,
        })) => {
            // Source of truth for everything below is `updated_group` —
            // the row state from the same tx as the write — not the
            // `final_*` values we asked it to write.
            info!(user = %requesting_user.username, ip = %ctx.peer_addr, group = %updated_group.name, "{}", LOG_GROUP_UPDATE_SUCCESS);
            let response = ServerMessage::GroupUpdateResponse {
                success: true,
                id: Some(updated_group.id),
                name: Some(updated_group.name.clone()),
                error: None,
            };
            ctx.send_message(&response).await?;

            // === Cascade to member sessions ===

            let name_changed = old_name != updated_group.name;
            let new_permissions: HashSet<Permission> = final_permissions.iter().copied().collect();
            let permissions_changed = old_permissions != new_permissions;
            let bandwidth_weight_changed = old_bandwidth_weight != updated_group.bandwidth_weight;

            // Cascade scope: when the group's bandwidth weight changes,
            // only members whose effective weight is sourced from the group
            // (NULL override) actually move. Override-holders stay pinned
            // to their own value — broadcasting "your bandwidth changed"
            // for them would be both wasteful and a lie. The set comes
            // directly from `update_group` (computed inside the same tx
            // as the write), so the membership matches the row state we
            // just committed.
            let inheriting_set: HashSet<i64> = if bandwidth_weight_changed {
                inheriting_member_ids.into_iter().collect()
            } else {
                HashSet::new()
            };

            // Bandwidth cache refresh: a single batched call walks the
            // session map once and updates every session whose user_id
            // is in `inheriting_set`. Offline members produce no
            // matching sessions; we don't need a session snapshot to
            // scope this fan-out.
            //
            // Race window with concurrent `UserUpdate`: `inheriting_set`
            // is bound to the group tx, but this fan-out runs after
            // commit. If a `UserUpdate` sets a per-user override on
            // member X between our commit and this fan-out, X is no
            // longer truly inheriting but we still write
            // `updated_group.bandwidth_weight` to X's cache — briefly
            // poisoning it with the old (now-incorrect) value. The
            // window is bounded by the gap between the two `await`
            // points (microseconds in practice); the `UserUpdate`
            // handler's own cache-refresh path then overwrites X with
            // the correct resolved value. Bandwidth is advisory for
            // fairness, not a correctness invariant, so the brief stale
            // window is acceptable. Eliminating it would require
            // re-resolving every inheriting member inside the tx (N
            // extra reads); accept the eventual consistency instead.
            if bandwidth_weight_changed {
                ctx.user_manager
                    .update_bandwidth_weight_for_user_ids(
                        &inheriting_set,
                        updated_group.bandwidth_weight,
                    )
                    .await;
            }

            if name_changed {
                // Update cached group_name on all member sessions; the broadcast
                // helper reads group_name straight from the session cache.
                ctx.user_manager
                    .update_group_name(id, &updated_group.name)
                    .await;
            }

            // Single fresh snapshot of online member sessions, reused by
            // both the UserUpdated broadcast and the permissions cascade
            // below. Taken AFTER the bandwidth/name cache writes so the
            // shared-account broadcast branch (which reads `group_name`
            // / `bandwidth_weight` straight off the session) sees the
            // post-update values. The permissions cascade re-resolves
            // permissions from the DB per member, so it only needs the
            // snapshot for `user_id` / `username` and the pre-cascade
            // `permissions` it diffs against — none of which the
            // bandwidth/name writes touch.
            let member_sessions =
                if name_changed || !inheriting_set.is_empty() || permissions_changed {
                    ctx.user_manager.get_sessions_by_group_id(id).await
                } else {
                    Vec::new()
                };

            // Single UserUpdated broadcast per affected user:
            // - name_changed → every member (group_name shifted for all)
            // - bandwidth_weight_changed → only inheriting members
            //   (override-holders' effective value didn't move).
            if name_changed || !inheriting_set.is_empty() {
                broadcast_user_updated_for_members(ctx, &member_sessions, |session| {
                    name_changed || inheriting_set.contains(&session.user_id)
                })
                .await;
            }

            // Permission change cascade: re-resolve effective permissions for each
            // online member, update session caches, send PermissionsUpdated, do voice cleanup.
            // Offline users get fresh permissions at next login — no cascade needed.
            if permissions_changed {
                // Fetch config once for ServerInfo construction (shared across all members)
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

                // Deduplicate by user_id (regular accounts may have multiple sessions)
                let mut seen_user_ids: HashSet<i64> = HashSet::new();
                for session in &member_sessions {
                    if !seen_user_ids.insert(session.user_id) {
                        continue;
                    }

                    // Capture old cached permissions from session (before update)
                    let old_session_perms = session.permissions.clone();

                    // Re-resolve effective permissions from DB
                    let new_effective = match ctx
                        .db
                        .users
                        .get_user_permissions(session.user_id)
                        .await
                    {
                        Ok(p) => p,
                        Err(e) => {
                            error!(user = %requesting_user.username, ip = %ctx.peer_addr, target = %session.username, err = %e, "{}", LOG_GROUP_UPDATE_DB_ERROR_PERMISSIONS);
                            continue;
                        }
                    };

                    // Update session cache
                    ctx.user_manager
                        .update_permissions(session.user_id, new_effective.permissions.clone())
                        .await;

                    // Check if permissions actually changed for this member
                    if old_session_perms == new_effective.permissions {
                        continue;
                    }

                    // Build and send PermissionsUpdated
                    let permission_strings: Vec<String> = new_effective
                        .permissions
                        .iter()
                        .map(|p| p.as_str().to_string())
                        .collect();

                    // Build per-user ServerInfo with permission-based field visibility
                    let has_file_reindex =
                        new_effective.permissions.contains(&Permission::FileReindex);
                    let has_chat_join = new_effective.permissions.contains(&Permission::ChatJoin);
                    let info_options = ServerInfoOptions {
                        is_admin: false,
                        has_file_reindex,
                        has_chat_join,
                        include_image: false,
                    };
                    let server_info = Some(build_server_info(&info_values, &info_options));

                    let permissions_update = ServerMessage::PermissionsUpdated {
                        is_admin: false,
                        permissions: permission_strings,
                        server_info,
                        group_id: Some(id),
                        group_name: Some(updated_group.name.clone()),
                    };

                    ctx.user_manager
                        .broadcast_to_username(&session.username, &permissions_update)
                        .await;

                    // Voice cleanup: if voice_listen was revoked, kick from voice
                    let had_voice_listen = old_session_perms.contains(&Permission::VoiceListen);
                    let has_voice_listen =
                        new_effective.permissions.contains(&Permission::VoiceListen);

                    if had_voice_listen && !has_voice_listen {
                        let session_ids = ctx
                            .user_manager
                            .get_session_ids_for_user(&session.username)
                            .await;

                        for sid in session_ids {
                            if let Some(info) = ctx.voice_registry.remove_by_session_id(sid).await {
                                let leaving_user_tx = ctx
                                    .user_manager
                                    .get_user_by_session_id(sid)
                                    .await
                                    .map(|u| u.tx.clone());

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

            Ok(())
        }
        Ok(None) => {
            // 0 rows affected — either group was deleted by another admin
            // between our fetch and the update, or the atomic shared-toggle
            // protection blocked the update (members exist).
            // Query member count to distinguish the two cases.
            let error = match ctx.db.groups.get_member_count(id).await {
                Ok(count) if count > 0 => err_group_not_empty_modify(ctx.locale),
                _ => err_group_not_found(ctx.locale),
            };
            let response = ServerMessage::GroupUpdateResponse {
                success: false,
                id: None,
                name: None,
                error: Some(error),
            };
            ctx.send_message(&response).await
        }
        Err(e) => {
            // Check if failure was due to duplicate name
            if ctx
                .db
                .groups
                .get_group_by_name(&final_name)
                .await
                .ok()
                .flatten()
                .is_some_and(|existing| existing.id != id)
            {
                let response = ServerMessage::GroupUpdateResponse {
                    success: false,
                    id: None,
                    name: None,
                    error: Some(err_group_already_exists(ctx.locale)),
                };
                ctx.send_message(&response).await
            } else {
                error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_GROUP_UPDATE_DB_ERROR);
                ctx.send_error_and_disconnect(&err_database(ctx.locale), Some(HANDLER_GROUP_UPDATE))
                    .await
            }
        }
    }
}

/// Fan out one `UserUpdated` per affected member to everyone with
/// `Permission::UserList`. Shared accounts broadcast per-session (each
/// session is a distinct identity with its own nickname); regular accounts
/// dedup by username and broadcast an aggregated `UserInfo`. `should_emit`
/// decides which members participate — typically a predicate built from the
/// trigger flags (name change, bandwidth change set).
async fn broadcast_user_updated_for_members<W, F>(
    ctx: &HandlerContext<'_, W>,
    member_sessions: &[UserSession],
    should_emit: F,
) where
    W: AsyncWrite + Unpin,
    F: Fn(&UserSession) -> bool,
{
    let mut seen_usernames: HashSet<String> = HashSet::new();
    for session in member_sessions {
        if !should_emit(session) {
            continue;
        }
        if session.is_shared {
            let user_info = UserManager::build_user_info_from_session(session);
            let user_updated = ServerMessage::UserUpdated {
                previous_username: session.username.clone(),
                user: user_info,
            };
            ctx.user_manager
                .broadcast_to_permission(user_updated, Permission::UserList)
                .await;
        } else {
            let username_lower = session.username.to_lowercase();
            if !seen_usernames.insert(username_lower) {
                continue;
            }
            let all_sessions = ctx
                .user_manager
                .get_sessions_by_username(&session.username)
                .await;
            if let Some(user_info) = UserManager::build_aggregated_user_info(&all_sessions) {
                let user_updated = ServerMessage::UserUpdated {
                    previous_username: session.username.clone(),
                    user: user_info,
                };
                ctx.user_manager
                    .broadcast_to_permission(user_updated, Permission::UserList)
                    .await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::db;
    use crate::handlers::testing::{create_test_context, login_user, read_server_message};

    #[tokio::test]
    async fn test_group_update_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_group_update(
            1,
            Some("NewName".to_string()),
            None,
            None,
            None,
            None,
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_err(), "GroupUpdate should require login");
    }

    #[tokio::test]
    async fn test_group_update_requires_permission() {
        let mut test_ctx = create_test_context().await;

        // Login as user without GroupEdit permission
        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let result = handle_group_update(
            1,
            Some("NewName".to_string()),
            None,
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_permission_denied(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected GroupUpdateResponse with permission denied"),
        }
    }

    #[tokio::test]
    async fn test_group_update_not_found() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::GroupEdit],
            false,
        )
        .await;

        let result = handle_group_update(
            9999,
            Some("NewName".to_string()),
            None,
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_group_not_found(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected GroupUpdateResponse with not found"),
        }
    }

    #[tokio::test]
    async fn test_group_update_no_fields() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::GroupEdit],
            false,
        )
        .await;

        let result = handle_group_update(
            1,
            None,
            None,
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_group_no_fields(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected GroupUpdateResponse with no fields error"),
        }
    }

    #[tokio::test]
    async fn test_group_update_rename() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::GroupEdit],
            false,
        )
        .await;

        // Create a group
        let group = test_ctx
            .db
            .groups
            .create_group("OldName", false, &Permissions::new(), 1)
            .await
            .expect("Failed to create group");

        let result = handle_group_update(
            group.id,
            Some("NewName".to_string()),
            None,
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse {
                success,
                id,
                name,
                error,
            } => {
                assert!(success);
                assert_eq!(id, Some(group.id));
                assert_eq!(name, Some("NewName".to_string()));
                assert!(error.is_none());
            }
            _ => panic!("Expected GroupUpdateResponse success"),
        }

        // Verify in database
        let updated = test_ctx
            .db
            .groups
            .get_group_by_id(group.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated.name, "NewName");
    }

    #[tokio::test]
    async fn test_group_update_duplicate_name() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::GroupEdit],
            false,
        )
        .await;

        // Create two groups
        let _group_a = test_ctx
            .db
            .groups
            .create_group("GroupA", false, &Permissions::new(), 1)
            .await
            .expect("Failed to create GroupA");

        let group_b = test_ctx
            .db
            .groups
            .create_group("GroupB", false, &Permissions::new(), 1)
            .await
            .expect("Failed to create GroupB");

        // Try to rename GroupB to GroupA
        let result = handle_group_update(
            group_b.id,
            Some("GroupA".to_string()),
            None,
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_group_already_exists(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected GroupUpdateResponse with already exists"),
        }
    }

    #[tokio::test]
    async fn test_group_update_permissions() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[
                db::Permission::GroupEdit,
                db::Permission::ChatSend,
                db::Permission::UserKick,
            ],
            false,
        )
        .await;

        // Create a group with initial permissions
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
            .expect("Failed to create group");

        // Update permissions
        let result = handle_group_update(
            group.id,
            None,
            None,
            Some(vec!["chat_send".to_string(), "user_kick".to_string()]),
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse {
                success, id, error, ..
            } => {
                assert!(success);
                assert_eq!(id, Some(group.id));
                assert!(error.is_none());
            }
            _ => panic!("Expected GroupUpdateResponse success"),
        }

        // Verify permissions in database
        let perms = test_ctx
            .db
            .groups
            .get_group_permissions(group.id)
            .await
            .unwrap();
        assert_eq!(perms.len(), 2);
        assert!(perms.contains(&Permission::ChatSend));
        assert!(perms.contains(&Permission::UserKick));
    }

    #[tokio::test]
    async fn test_group_update_shared_toggle_with_members_rejected() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::GroupEdit],
            false,
        )
        .await;

        // Create a non-shared group
        let group = test_ctx
            .db
            .groups
            .create_group("Staff", false, &Permissions::new(), 1)
            .await
            .expect("Failed to create group");

        // Assign a user to this group via create_user with group_id
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "member",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &db::Permissions::new(),
                group_id: Some(group.id),
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .expect("Failed to create member");

        // Try to toggle is_shared — should fail because group has members
        let result = handle_group_update(
            group.id,
            None,
            Some(true),
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_group_not_empty_modify(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected GroupUpdateResponse with not empty error"),
        }
    }

    #[tokio::test]
    async fn test_group_update_shared_toggle_no_members() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::GroupEdit],
            false,
        )
        .await;

        // Create a non-shared group with no members
        let group = test_ctx
            .db
            .groups
            .create_group("Staff", false, &Permissions::new(), 1)
            .await
            .expect("Failed to create group");

        // Toggle is_shared — should succeed (no members)
        let result = handle_group_update(
            group.id,
            None,
            Some(true),
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse {
                success, id, error, ..
            } => {
                assert!(success);
                assert_eq!(id, Some(group.id));
                assert!(error.is_none());
            }
            _ => panic!("Expected GroupUpdateResponse success"),
        }

        // Verify in database
        let updated = test_ctx
            .db
            .groups
            .get_group_by_id(group.id)
            .await
            .unwrap()
            .unwrap();
        assert!(updated.is_shared);
    }

    #[tokio::test]
    async fn test_group_update_shared_with_forbidden_permission() {
        let mut test_ctx = create_test_context().await;

        // Login as admin so permission delegation doesn't interfere
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a non-shared group with no members
        let group = test_ctx
            .db
            .groups
            .create_group(
                "Staff",
                false,
                &Permissions::from(&[Permission::UserKick]),
                1,
            )
            .await
            .expect("Failed to create group");

        // Try to make it shared while keeping user_kick (forbidden for shared)
        let result = handle_group_update(
            group.id,
            None,
            Some(true),
            Some(vec!["user_kick".to_string()]),
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(
                    error,
                    Some(err_group_shared_permission(DEFAULT_TEST_LOCALE))
                );
            }
            _ => panic!("Expected GroupUpdateResponse with shared permission error"),
        }
    }

    #[tokio::test]
    async fn test_group_update_admin_has_permission() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a group
        let group = test_ctx
            .db
            .groups
            .create_group("Staff", false, &Permissions::new(), 1)
            .await
            .expect("Failed to create group");

        // Update name and permissions as admin
        let result = handle_group_update(
            group.id,
            Some("Moderators".to_string()),
            None,
            Some(vec!["user_kick".to_string(), "ban_create".to_string()]),
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse {
                success,
                id,
                name,
                error,
            } => {
                assert!(success);
                assert_eq!(id, Some(group.id));
                assert_eq!(name, Some("Moderators".to_string()));
                assert!(error.is_none());
            }
            _ => panic!("Expected GroupUpdateResponse success"),
        }

        // Verify in database
        let updated = test_ctx
            .db
            .groups
            .get_group_by_id(group.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated.name, "Moderators");

        let perms = test_ctx
            .db
            .groups
            .get_group_permissions(group.id)
            .await
            .unwrap();
        assert_eq!(perms.len(), 2);
        assert!(perms.contains(&Permission::BanCreate));
        assert!(perms.contains(&Permission::UserKick));
    }

    #[tokio::test]
    async fn test_group_update_non_admin_cannot_set_unowned_permissions() {
        let mut test_ctx = create_test_context().await;

        // Login as non-admin with GroupEdit + ChatSend (but NOT UserKick)
        let editor_session = login_user(
            &mut test_ctx,
            "editor",
            "password",
            &[Permission::GroupEdit, Permission::ChatSend],
            false,
        )
        .await;

        // Create a group (must be done via DB since editor can't create groups)
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
            .expect("Failed to create group");

        // Try to update permissions including one the editor doesn't have
        let result = handle_group_update(
            group.id,
            None,
            None,
            Some(vec!["chat_send".to_string(), "user_kick".to_string()]),
            None,
            Some(editor_session),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse { success, error, .. } => {
                assert!(!success, "Should reject unowned permission");
                assert!(error.is_some());
            }
            _ => panic!("Expected GroupUpdateResponse"),
        }

        // Verify permissions unchanged in DB
        let perms = test_ctx
            .db
            .groups
            .get_group_permissions(group.id)
            .await
            .unwrap();
        assert_eq!(perms.len(), 1);
        assert!(perms.contains(&Permission::ChatSend));
    }

    #[tokio::test]
    async fn test_group_update_non_admin_can_set_owned_permissions() {
        let mut test_ctx = create_test_context().await;

        // Login as non-admin with GroupEdit + ChatSend + UserList
        let editor_session = login_user(
            &mut test_ctx,
            "editor",
            "password",
            &[
                Permission::GroupEdit,
                Permission::ChatSend,
                Permission::UserList,
            ],
            false,
        )
        .await;

        // Create a group with ChatSend
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
            .expect("Failed to create group");

        // Update to ChatSend + UserList — both owned by editor
        let result = handle_group_update(
            group.id,
            None,
            None,
            Some(vec!["chat_send".to_string(), "user_list".to_string()]),
            None,
            Some(editor_session),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse { success, error, .. } => {
                assert!(success, "Should accept owned permissions");
                assert!(error.is_none());
            }
            _ => panic!("Expected GroupUpdateResponse"),
        }

        // Verify permissions updated in DB
        let perms = test_ctx
            .db
            .groups
            .get_group_permissions(group.id)
            .await
            .unwrap();
        assert_eq!(perms.len(), 2);
        assert!(perms.contains(&Permission::ChatSend));
        assert!(perms.contains(&Permission::UserList));
    }

    #[tokio::test]
    async fn test_group_update_non_admin_merge_preserves_unowned_permissions() {
        let mut test_ctx = create_test_context().await;

        // Login as non-admin with GroupEdit + ChatSend (but NOT UserKick)
        let editor_session = login_user(
            &mut test_ctx,
            "editor",
            "password",
            &[Permission::GroupEdit, Permission::ChatSend],
            false,
        )
        .await;

        // Create a group with ChatSend + UserKick
        let group = test_ctx
            .db
            .groups
            .create_group(
                "Staff",
                false,
                &Permissions::from(&[Permission::ChatSend, Permission::UserKick]),
                1,
            )
            .await
            .expect("Failed to create group");

        // Editor sends only ChatSend (the one they control)
        // UserKick should be preserved automatically
        let result = handle_group_update(
            group.id,
            None,
            None,
            Some(vec!["chat_send".to_string()]),
            None,
            Some(editor_session),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse { success, error, .. } => {
                assert!(success, "Should accept owned permissions with merge");
                assert!(error.is_none());
            }
            _ => panic!("Expected GroupUpdateResponse"),
        }

        // Verify: ChatSend kept (requested), UserKick preserved (unowned by editor)
        let perms = test_ctx
            .db
            .groups
            .get_group_permissions(group.id)
            .await
            .unwrap();
        assert_eq!(perms.len(), 2);
        assert!(perms.contains(&Permission::ChatSend));
        assert!(perms.contains(&Permission::UserKick));
    }

    #[tokio::test]
    async fn test_group_update_non_admin_merge_can_remove_owned_permission() {
        let mut test_ctx = create_test_context().await;

        // Login as non-admin with GroupEdit + ChatSend + UserList (but NOT UserKick)
        let editor_session = login_user(
            &mut test_ctx,
            "editor",
            "password",
            &[
                Permission::GroupEdit,
                Permission::ChatSend,
                Permission::UserList,
            ],
            false,
        )
        .await;

        // Create a group with ChatSend + UserList + UserKick
        let group = test_ctx
            .db
            .groups
            .create_group(
                "Staff",
                false,
                &Permissions::from(&[
                    Permission::ChatSend,
                    Permission::UserList,
                    Permission::UserKick,
                ]),
                1,
            )
            .await
            .expect("Failed to create group");

        // Editor sends only ChatSend — removing UserList (which they control)
        // UserKick should be preserved (editor can't control it)
        let result = handle_group_update(
            group.id,
            None,
            None,
            Some(vec!["chat_send".to_string()]),
            None,
            Some(editor_session),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse { success, error, .. } => {
                assert!(success, "Should succeed with merge");
                assert!(error.is_none());
            }
            _ => panic!("Expected GroupUpdateResponse"),
        }

        // Verify: ChatSend kept, UserList removed (editor's choice), UserKick preserved
        let perms = test_ctx
            .db
            .groups
            .get_group_permissions(group.id)
            .await
            .unwrap();
        assert_eq!(perms.len(), 2);
        assert!(perms.contains(&Permission::ChatSend));
        assert!(perms.contains(&Permission::UserKick));
        assert!(!perms.contains(&Permission::UserList));
    }

    // ========================================================================
    // Cascade tests (Step 7)
    // ========================================================================

    #[tokio::test]
    async fn test_group_update_permission_cascade_sends_permissions_updated() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a group with chat_send
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
            .expect("Failed to create group");

        // Create bob assigned to the group (no individual grants — relies on group)
        let bob = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: "hash",
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

        // Resolve effective permissions from DB (group provides them)
        let bob_effective = test_ctx
            .db
            .users
            .get_user_permissions(bob.id)
            .await
            .unwrap();

        // Add bob to UserManager so he's "online"
        let bob_session = test_ctx
            .user_manager
            .add_user(crate::users::user::NewSessionParams {
                session_id: 0,
                user_id: bob.id,
                username: "bob".to_string(),
                is_admin: false,
                is_shared: false,
                permissions: bob_effective.permissions.clone(),
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
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
                last_activity: Instant::now(),
            })
            .await
            .unwrap();

        // Update group permissions: add user_kick
        let result = handle_group_update(
            group.id,
            None,
            None,
            Some(vec!["chat_send".to_string(), "user_kick".to_string()]),
            None,
            Some(admin_session),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        // Read GroupUpdateResponse
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse { success, .. } => assert!(success),
            _ => panic!("Expected GroupUpdateResponse"),
        }

        // Read PermissionsUpdated sent to bob via the broadcast channel
        let (msg, _) = test_ctx
            .rx
            .recv()
            .await
            .expect("Should receive PermissionsUpdated");
        match msg {
            ServerMessage::PermissionsUpdated {
                is_admin,
                permissions,
                server_info,
                group_id,
                group_name,
            } => {
                assert!(!is_admin);
                assert!(permissions.contains(&"chat_send".to_string()));
                assert!(permissions.contains(&"user_kick".to_string()));
                assert_eq!(group_id, Some(group.id));
                assert_eq!(group_name, Some("Staff".to_string()));

                // Verify server_info is included in group cascade
                let info = server_info.expect("server_info should be included");
                // All-user fields should be populated
                assert!(info.name.is_some());
                assert!(info.version.is_some());
                assert!(info.chat_burst_limit.is_some());
                assert!(info.chat_rate_limit.is_some());
                assert!(info.min_password_strength.is_some());
                assert!(info.log_level.is_some());
                // Admin-only fields should be None for non-admin bob
                assert!(info.persistent_channels.is_none());
                // Image not included in PermissionsUpdated
                assert!(info.image.is_none());
            }
            _ => panic!("Expected PermissionsUpdated, got {:?}", msg),
        }

        // Verify session cache was updated
        let updated_bob = test_ctx
            .user_manager
            .get_user_by_session_id(bob_session)
            .await
            .unwrap();
        assert!(updated_bob.permissions.contains(&db::Permission::ChatSend));
        assert!(updated_bob.permissions.contains(&db::Permission::UserKick));
    }

    #[tokio::test]
    async fn test_group_update_name_cascade_sends_user_updated() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a group
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
            .expect("Failed to create group");

        // Create bob assigned to the group (no individual grants — relies on group)
        let bob = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: "hash",
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

        // Resolve effective permissions from DB, then add user_list for receiving UserUpdated
        let bob_effective = test_ctx
            .db
            .users
            .get_user_permissions(bob.id)
            .await
            .unwrap();
        let mut bob_session_perms = bob_effective.permissions.clone();
        bob_session_perms.insert(db::Permission::UserList);
        let bob_session = test_ctx
            .user_manager
            .add_user(crate::users::user::NewSessionParams {
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
                group_id: Some(group.id),
                group_name: Some("Staff".to_string()),
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
                last_activity: Instant::now(),
            })
            .await
            .unwrap();

        // Rename group (no permission changes)
        let result = handle_group_update(
            group.id,
            Some("Moderators".to_string()),
            None,
            None,
            None,
            Some(admin_session),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        // Read GroupUpdateResponse
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse { success, name, .. } => {
                assert!(success);
                assert_eq!(name, Some("Moderators".to_string()));
            }
            _ => panic!("Expected GroupUpdateResponse"),
        }

        // Read UserUpdated broadcast (sent to users with user_list permission)
        let (msg, _) = test_ctx
            .rx
            .recv()
            .await
            .expect("Should receive UserUpdated");
        match msg {
            ServerMessage::UserUpdated {
                previous_username,
                user,
            } => {
                assert_eq!(previous_username, "bob");
                assert_eq!(user.username, "bob");
                assert_eq!(user.group_id, Some(group.id));
                assert_eq!(user.group_name, Some("Moderators".to_string()));
            }
            _ => panic!("Expected UserUpdated, got {:?}", msg),
        }

        // Verify session cache was updated
        let updated_bob = test_ctx
            .user_manager
            .get_user_by_session_id(bob_session)
            .await
            .unwrap();
        assert_eq!(updated_bob.group_name, Some("Moderators".to_string()));
    }

    #[tokio::test]
    async fn test_group_update_no_cascade_when_no_online_members() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a group with a member in DB (but NOT in UserManager — offline)
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
            .expect("Failed to create group");

        let mut bob_perms = db::Permissions::new();
        bob_perms.permissions.insert(db::Permission::ChatSend);
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &bob_perms,
                group_id: Some(group.id),
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        // Update group permissions — bob is offline so no PermissionsUpdated should be sent
        let result = handle_group_update(
            group.id,
            None,
            None,
            Some(vec!["chat_send".to_string(), "user_kick".to_string()]),
            None,
            Some(admin_session),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        // Read GroupUpdateResponse
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse { success, .. } => assert!(success),
            _ => panic!("Expected GroupUpdateResponse"),
        }

        // Channel should be empty — no PermissionsUpdated was sent
        assert!(
            test_ctx.rx.try_recv().is_err(),
            "No PermissionsUpdated should be sent when member is offline"
        );
    }

    #[tokio::test]
    async fn test_group_update_voice_listen_revoked_kicks_from_voice() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a group with voice_listen
        let group = test_ctx
            .db
            .groups
            .create_group(
                "Listeners",
                false,
                &Permissions::from(&[Permission::VoiceListen, Permission::ChatSend]),
                1,
            )
            .await
            .expect("Failed to create group");

        // Create bob assigned to the group (no individual grants — relies on group)
        let bob = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "voicebob",
                hashed_password: "hash",
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

        // Resolve effective permissions from DB (group provides them)
        let bob_effective = test_ctx
            .db
            .users
            .get_user_permissions(bob.id)
            .await
            .unwrap();

        // Add bob to UserManager
        let bob_session = test_ctx
            .user_manager
            .add_user(crate::users::user::NewSessionParams {
                session_id: 0,
                user_id: bob.id,
                username: "voicebob".to_string(),
                is_admin: false,
                is_shared: false,
                permissions: bob_effective.permissions.clone(),
                address: test_ctx.peer_addr,
                created_at: 0,
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: DEFAULT_TEST_LOCALE.to_string(),
                avatar: None,
                nickname: "voicebob".to_string(),
                is_away: false,
                status: None,
                group_id: Some(group.id),
                group_name: Some("Listeners".to_string()),
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
                last_activity: Instant::now(),
            })
            .await
            .unwrap();

        // Have bob join a channel and voice
        let _ = test_ctx.channel_manager.join("#general", bob_session).await;

        let voice_session = crate::voice::VoiceSession::new(
            "voicebob".to_string(),
            vec!["#general".to_string()],
            bob_session,
            test_ctx.peer_addr.ip(),
        );
        test_ctx.voice_registry.add(voice_session).await;

        // Verify bob is in voice
        assert!(test_ctx.voice_registry.has_session(bob_session).await);

        // Remove voice_listen from group (keep only chat_send)
        let result = handle_group_update(
            group.id,
            None,
            None,
            Some(vec!["chat_send".to_string()]),
            None,
            Some(admin_session),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        // Read GroupUpdateResponse
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse { success, .. } => assert!(success),
            _ => panic!("Expected GroupUpdateResponse"),
        }

        // Verify bob was kicked from voice
        assert!(
            !test_ctx.voice_registry.has_session(bob_session).await,
            "User should be kicked from voice when voice_listen is revoked via group edit"
        );
    }

    #[tokio::test]
    async fn test_group_update_permission_cascade_with_member_overrides() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a group with chat_send and user_kick
        let group = test_ctx
            .db
            .groups
            .create_group(
                "Staff",
                false,
                &Permissions::from(&[Permission::ChatSend, Permission::UserKick]),
                1,
            )
            .await
            .expect("Failed to create group");

        // Create bob assigned to the group with a grant override (user_list)
        // and a revoke override (user_kick)
        let mut bob_perms = db::Permissions::new();
        // Effective before: (chat_send, user_kick) ∪ (user_list) - (user_kick) = chat_send, user_list
        bob_perms.permissions.insert(db::Permission::ChatSend);
        bob_perms.permissions.insert(db::Permission::UserList);
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
                group_id: Some(group.id),
                revokes: &[db::Permission::UserKick],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        // Add bob to UserManager with effective permissions
        let mut effective = std::collections::HashSet::new();
        effective.insert(db::Permission::ChatSend);
        effective.insert(db::Permission::UserList);
        let bob_session = test_ctx
            .user_manager
            .add_user(crate::users::user::NewSessionParams {
                session_id: 0,
                user_id: bob.id,
                username: "bob".to_string(),
                is_admin: false,
                is_shared: false,
                permissions: effective,
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
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
                last_activity: Instant::now(),
            })
            .await
            .unwrap();

        // Update group: add ban_create, remove user_kick
        // New group perms: chat_send, ban_create
        // Bob's effective: (chat_send, ban_create) ∪ (user_list) - (user_kick) = chat_send, ban_create, user_list
        // (user_kick revoke is now a no-op since group no longer has it)
        let result = handle_group_update(
            group.id,
            None,
            None,
            Some(vec!["chat_send".to_string(), "ban_create".to_string()]),
            None,
            Some(admin_session),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        // Read GroupUpdateResponse
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse { success, .. } => assert!(success),
            _ => panic!("Expected GroupUpdateResponse"),
        }

        // Read PermissionsUpdated
        let (msg, _) = test_ctx
            .rx
            .recv()
            .await
            .expect("Should receive PermissionsUpdated");
        match msg {
            ServerMessage::PermissionsUpdated { permissions, .. } => {
                assert!(permissions.contains(&"chat_send".to_string()));
                assert!(permissions.contains(&"ban_create".to_string()));
                assert!(permissions.contains(&"user_list".to_string()));
                // user_kick should NOT be present (revoke override + removed from group)
                assert!(!permissions.contains(&"user_kick".to_string()));
            }
            _ => panic!("Expected PermissionsUpdated, got {:?}", msg),
        }

        // Verify session cache
        let updated_bob = test_ctx
            .user_manager
            .get_user_by_session_id(bob_session)
            .await
            .unwrap();
        assert!(updated_bob.permissions.contains(&db::Permission::ChatSend));
        assert!(updated_bob.permissions.contains(&db::Permission::BanCreate));
        assert!(updated_bob.permissions.contains(&db::Permission::UserList));
        assert!(!updated_bob.permissions.contains(&db::Permission::UserKick));
    }

    #[tokio::test]
    async fn test_group_update_no_permissions_updated_when_effective_unchanged() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a group with chat_send and user_kick
        let group = test_ctx
            .db
            .groups
            .create_group(
                "Staff",
                false,
                &Permissions::from(&[Permission::ChatSend, Permission::UserKick]),
                1,
            )
            .await
            .expect("Failed to create group");

        // Create bob with a revoke on user_kick
        // Effective: (chat_send, user_kick) - (user_kick) = chat_send
        let mut bob_perms = db::Permissions::new();
        bob_perms.permissions.insert(db::Permission::ChatSend);
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
                group_id: Some(group.id),
                revokes: &[db::Permission::UserKick],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        // Add bob to UserManager with effective permissions (just chat_send)
        let mut effective = std::collections::HashSet::new();
        effective.insert(db::Permission::ChatSend);
        let bob_session = test_ctx
            .user_manager
            .add_user(crate::users::user::NewSessionParams {
                session_id: 0,
                user_id: bob.id,
                username: "bob".to_string(),
                is_admin: false,
                is_shared: false,
                permissions: effective,
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
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
                last_activity: Instant::now(),
            })
            .await
            .unwrap();

        // Remove user_kick from group (bob already had it revoked)
        // New group: chat_send only
        // Bob's effective: (chat_send) - (user_kick revoke is now no-op) = chat_send
        // No effective change for bob!
        let result = handle_group_update(
            group.id,
            None,
            None,
            Some(vec!["chat_send".to_string()]),
            None,
            Some(admin_session),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        // Read GroupUpdateResponse
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse { success, .. } => assert!(success),
            _ => panic!("Expected GroupUpdateResponse"),
        }

        // No PermissionsUpdated should be sent — bob's effective permissions didn't change
        assert!(
            test_ctx.rx.try_recv().is_err(),
            "No PermissionsUpdated when effective permissions unchanged for member"
        );

        // Verify bob still has only chat_send
        let updated_bob = test_ctx
            .user_manager
            .get_user_by_session_id(bob_session)
            .await
            .unwrap();
        assert_eq!(updated_bob.permissions.len(), 1);
        assert!(updated_bob.permissions.contains(&db::Permission::ChatSend));
    }

    #[tokio::test]
    async fn test_group_update_name_and_permissions_cascade_together() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a group
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
            .expect("Failed to create group");

        // Create bob assigned to the group (no individual grants — relies on group)
        let bob = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: "hash",
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

        // Resolve effective permissions from DB, then add user_list for receiving broadcasts
        let bob_effective = test_ctx
            .db
            .users
            .get_user_permissions(bob.id)
            .await
            .unwrap();
        let mut bob_session_perms = bob_effective.permissions.clone();
        bob_session_perms.insert(db::Permission::UserList);
        let bob_session = test_ctx
            .user_manager
            .add_user(crate::users::user::NewSessionParams {
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
                group_id: Some(group.id),
                group_name: Some("Staff".to_string()),
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
                last_activity: Instant::now(),
            })
            .await
            .unwrap();

        // Update both name and permissions
        let result = handle_group_update(
            group.id,
            Some("Moderators".to_string()),
            None,
            Some(vec!["chat_send".to_string(), "user_kick".to_string()]),
            None,
            Some(admin_session),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        // Read GroupUpdateResponse
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse { success, .. } => assert!(success),
            _ => panic!("Expected GroupUpdateResponse"),
        }

        // Should receive UserUpdated (name change) and PermissionsUpdated (perm change)
        // Note: broadcast_to_permission sends UserUpdated to ALL users with user_list
        // (both admin and bob share the same tx in tests), so we may get multiple messages.
        // Drain all messages and check we got both types.
        let mut got_user_updated = false;
        let mut got_permissions_updated = false;

        // Drain available messages (broadcasts are synchronous in test context)
        while let Ok((msg, _)) = test_ctx.rx.try_recv() {
            match msg {
                ServerMessage::UserUpdated {
                    ref previous_username,
                    ref user,
                } if previous_username == "bob" => {
                    assert_eq!(user.group_name, Some("Moderators".to_string()));
                    got_user_updated = true;
                }
                ServerMessage::PermissionsUpdated {
                    ref permissions,
                    ref group_name,
                    ..
                } => {
                    assert!(permissions.contains(&"chat_send".to_string()));
                    assert!(permissions.contains(&"user_kick".to_string()));
                    assert_eq!(*group_name, Some("Moderators".to_string()));
                    got_permissions_updated = true;
                }
                _ => {} // Ignore other broadcast messages
            }
        }

        assert!(got_user_updated, "Should have received UserUpdated");
        assert!(
            got_permissions_updated,
            "Should have received PermissionsUpdated"
        );

        // Verify session cache
        let updated_bob = test_ctx
            .user_manager
            .get_user_by_session_id(bob_session)
            .await
            .unwrap();
        assert_eq!(updated_bob.group_name, Some("Moderators".to_string()));
        assert!(updated_bob.permissions.contains(&db::Permission::ChatSend));
        assert!(updated_bob.permissions.contains(&db::Permission::UserKick));
    }

    #[tokio::test]
    async fn test_group_update_no_cascade_for_name_only_no_change() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a group
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
            .expect("Failed to create group");

        // Create bob assigned to the group (no individual grants — relies on group)
        let bob = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: "hash",
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

        // Resolve effective permissions from DB (group provides them)
        let bob_effective = test_ctx
            .db
            .users
            .get_user_permissions(bob.id)
            .await
            .unwrap();

        // Add bob to UserManager
        let _bob_session = test_ctx
            .user_manager
            .add_user(crate::users::user::NewSessionParams {
                session_id: 0,
                user_id: bob.id,
                username: "bob".to_string(),
                is_admin: false,
                is_shared: false,
                permissions: bob_effective.permissions.clone(),
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
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
                last_activity: Instant::now(),
            })
            .await
            .unwrap();

        // Update with same name and same permissions — no cascade should happen
        let result = handle_group_update(
            group.id,
            Some("Staff".to_string()),
            None,
            Some(vec!["chat_send".to_string()]),
            None,
            Some(admin_session),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        // Read GroupUpdateResponse
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse { success, .. } => assert!(success),
            _ => panic!("Expected GroupUpdateResponse"),
        }

        // No cascade messages — nothing changed
        assert!(
            test_ctx.rx.try_recv().is_err(),
            "No cascade when name and permissions are unchanged"
        );
    }

    /// Regression: shared-account members in a renamed/reweighted
    /// group must broadcast `UserUpdated` with the *new* `group_name`
    /// and `bandwidth_weight`. The helper's shared branch builds
    /// `UserInfo` straight from the session snapshot, so an unrefreshed
    /// snapshot taken before the cache writes silently leaks stale
    /// values into the broadcast.
    #[tokio::test]
    async fn test_group_update_shared_member_broadcast_has_fresh_group_name_and_weight() {
        let mut test_ctx = create_test_context().await;

        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let group = test_ctx
            .db
            .groups
            .create_group(
                "Staff",
                true,
                &Permissions::from(&[Permission::ChatSend]),
                5,
            )
            .await
            .expect("Failed to create shared group");

        let kiosk = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "kiosk",
                hashed_password: "hash",
                is_admin: false,
                is_shared: true,
                enabled: true,
                permissions: &db::Permissions::new(),
                group_id: Some(group.id),
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        let kiosk_effective = test_ctx
            .db
            .users
            .get_user_permissions(kiosk.id)
            .await
            .unwrap();
        test_ctx
            .user_manager
            .add_user(crate::users::user::NewSessionParams {
                session_id: 0,
                user_id: kiosk.id,
                username: "kiosk".to_string(),
                is_admin: false,
                is_shared: true,
                permissions: kiosk_effective.permissions.clone(),
                address: test_ctx.peer_addr,
                created_at: 0,
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: DEFAULT_TEST_LOCALE.to_string(),
                avatar: None,
                nickname: "lobby".to_string(),
                is_away: false,
                status: None,
                group_id: Some(group.id),
                group_name: Some("Staff".to_string()),
                bandwidth_weight: 5,
                last_activity: Instant::now(),
            })
            .await
            .unwrap();

        let result = handle_group_update(
            group.id,
            Some("Moderators".to_string()),
            None,
            None,
            Some(12),
            Some(admin_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse { success, .. } => assert!(success),
            other => panic!("Expected GroupUpdateResponse, got {:?}", other),
        }

        let mut saw_kiosk_broadcast = false;
        while let Ok((msg, _)) = test_ctx.rx.try_recv() {
            if let ServerMessage::UserUpdated {
                previous_username,
                user,
            } = msg
                && previous_username == "kiosk"
            {
                assert_eq!(user.nickname, "lobby");
                assert_eq!(
                    user.group_name,
                    Some("Moderators".to_string()),
                    "shared-account broadcast must carry the post-update group_name"
                );
                assert_eq!(
                    user.bandwidth_weight,
                    Some(12),
                    "shared-account broadcast must carry the post-update bandwidth_weight"
                );
                saw_kiosk_broadcast = true;
            }
        }
        assert!(
            saw_kiosk_broadcast,
            "expected a UserUpdated broadcast for the shared session"
        );
    }

    /// Set up a non-admin "editor" with resolved bandwidth weight = `weight`.
    async fn setup_editor_with_weight(
        test_ctx: &mut crate::handlers::testing::TestContext,
        weight: u16,
    ) -> u32 {
        let editor_group = test_ctx
            .db
            .groups
            .create_group(
                "Editors",
                false,
                &crate::db::Permissions::from(&[crate::db::Permission::GroupEdit]),
                weight,
            )
            .await
            .unwrap();
        let editor_session = login_user(
            test_ctx,
            "editor",
            "password",
            &[crate::db::Permission::GroupEdit],
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
        crate::handlers::user_update::handle_user_update(
            crate::handlers::user_update::UserUpdateRequest {
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
        editor_session
    }

    #[tokio::test]
    async fn test_groupupdate_non_admin_can_set_lower_weight() {
        let mut test_ctx = create_test_context().await;
        let editor_session = setup_editor_with_weight(&mut test_ctx, 25).await;

        // Create a target group at weight 5 (initial).
        let target = test_ctx
            .db
            .groups
            .create_group("Helpers", false, &crate::db::Permissions::new(), 5)
            .await
            .unwrap();

        // Editor bumps target's weight to 20 (≤ editor's 25): delegation OK.
        let result = handle_group_update(
            target.id,
            None,
            None,
            None,
            Some(20),
            Some(editor_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse { success, error, .. } => {
                assert!(success, "delegation-OK weight should succeed: {:?}", error);
            }
            _ => panic!("Expected GroupUpdateResponse"),
        }
        // Drain any cascade messages.
        while test_ctx.rx.try_recv().is_ok() {}
    }

    #[tokio::test]
    async fn test_groupupdate_non_admin_cannot_set_higher_weight() {
        let mut test_ctx = create_test_context().await;
        let editor_session = setup_editor_with_weight(&mut test_ctx, 25).await;

        let target = test_ctx
            .db
            .groups
            .create_group("Helpers", false, &crate::db::Permissions::new(), 5)
            .await
            .unwrap();

        // Editor tries to bump target to 100 (> editor's 25): delegation rejects.
        let result = handle_group_update(
            target.id,
            None,
            None,
            None,
            Some(100),
            Some(editor_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(
                    error.unwrap(),
                    crate::handlers::err_bandwidth_weight_delegation(
                        crate::handlers::testing::DEFAULT_TEST_LOCALE
                    )
                );
            }
            _ => panic!("Expected GroupUpdateResponse"),
        }
    }

    #[tokio::test]
    async fn test_groupupdate_admin_bypasses_delegation() {
        let mut test_ctx = create_test_context().await;
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let target = test_ctx
            .db
            .groups
            .create_group("Helpers", false, &crate::db::Permissions::new(), 5)
            .await
            .unwrap();

        // Admin bumps target's weight to a value far above any non-admin's reach.
        let result = handle_group_update(
            target.id,
            None,
            None,
            None,
            Some(10_000),
            Some(admin_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse { success, error, .. } => {
                assert!(success, "admin bypass should succeed: {:?}", error);
            }
            _ => panic!("Expected GroupUpdateResponse"),
        }
        // Drain any cascade messages.
        while test_ctx.rx.try_recv().is_ok() {}
    }

    /// Regression: changing a group's bandwidth_weight must broadcast
    /// `UserUpdated` for each inheriting online member so other clients'
    /// user lists reflect the new effective weight. Without the broadcast,
    /// the cache refresh would update the server side but leave peers stale.
    #[tokio::test]
    async fn test_group_update_bandwidth_cascade_sends_user_updated() {
        let mut test_ctx = create_test_context().await;

        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Group starts at weight 5; bob inherits it.
        let group = test_ctx
            .db
            .groups
            .create_group(
                "Staff",
                false,
                &Permissions::from(&[Permission::ChatSend]),
                5,
            )
            .await
            .expect("Failed to create group");

        let bob = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: "hash",
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

        let bob_effective = test_ctx
            .db
            .users
            .get_user_permissions(bob.id)
            .await
            .unwrap();
        // Grant UserList so bob can receive the broadcast on his own tx.
        let mut bob_session_perms = bob_effective.permissions.clone();
        bob_session_perms.insert(db::Permission::UserList);
        let bob_session = test_ctx
            .user_manager
            .add_user(crate::users::user::NewSessionParams {
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
                group_id: Some(group.id),
                group_name: Some("Staff".to_string()),
                bandwidth_weight: 5,
                last_activity: Instant::now(),
            })
            .await
            .unwrap();

        // Bump group weight to 12.
        let result = handle_group_update(
            group.id,
            None,
            None,
            None,
            Some(12),
            Some(admin_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());

        // GroupUpdateResponse first.
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse { success, .. } => assert!(success),
            other => panic!("Expected GroupUpdateResponse, got {:?}", other),
        }

        // Then the cascaded UserUpdated for bob.
        let (msg, _) = test_ctx
            .rx
            .recv()
            .await
            .expect("Should receive UserUpdated broadcast");
        match msg {
            ServerMessage::UserUpdated {
                previous_username,
                user,
            } => {
                assert_eq!(previous_username, "bob");
                assert_eq!(user.username, "bob");
                assert_eq!(user.bandwidth_weight, Some(12));
            }
            other => panic!("Expected UserUpdated, got {:?}", other),
        }

        // Session cache reflects the new weight.
        let updated_bob = test_ctx
            .user_manager
            .get_user_by_session_id(bob_session)
            .await
            .unwrap();
        assert_eq!(
            updated_bob
                .bandwidth_weight
                .load(std::sync::atomic::Ordering::Relaxed),
            12
        );
    }

    /// Regression: when a single GroupUpdate changes BOTH name and
    /// bandwidth_weight, the cascade must emit exactly one `UserUpdated`
    /// call per affected user — not one per changed field. Two broadcasts
    /// would fan out to every UserList-holder for the same delta.
    #[tokio::test]
    async fn test_group_update_name_and_bandwidth_one_broadcast_per_member() {
        let mut test_ctx = create_test_context().await;

        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let group = test_ctx
            .db
            .groups
            .create_group(
                "Staff",
                false,
                &Permissions::from(&[Permission::ChatSend]),
                5,
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
                permissions: &db::Permissions::new(),
                group_id: Some(group.id),
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        let bob_effective = test_ctx
            .db
            .users
            .get_user_permissions(bob.id)
            .await
            .unwrap();
        let mut bob_session_perms = bob_effective.permissions.clone();
        bob_session_perms.insert(db::Permission::UserList);
        let _ = test_ctx
            .user_manager
            .add_user(crate::users::user::NewSessionParams {
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
                group_id: Some(group.id),
                group_name: Some("Staff".to_string()),
                bandwidth_weight: 5,
                last_activity: Instant::now(),
            })
            .await
            .unwrap();

        // Change name AND bandwidth in the same call.
        let result = handle_group_update(
            group.id,
            Some("Moderators".to_string()),
            None,
            None,
            Some(12),
            Some(admin_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse { success, .. } => assert!(success),
            other => panic!("Expected GroupUpdateResponse, got {:?}", other),
        }

        // Drain UserUpdated messages and count. Bob's tx is shared with admin's
        // (same `test_ctx.tx`), and `broadcast_to_permission` sends per-session.
        // A single consolidated broadcast → 2 messages (admin via admin bypass,
        // bob via explicit UserList). The pre-fix two-loop version sent 4.
        let mut user_updated_count = 0;
        while let Ok((msg, _)) = test_ctx.rx.try_recv() {
            match msg {
                ServerMessage::UserUpdated { user, .. } if user.username == "bob" => {
                    assert_eq!(user.group_name, Some("Moderators".to_string()));
                    assert_eq!(user.bandwidth_weight, Some(12));
                    user_updated_count += 1;
                }
                other => panic!("Unexpected message in queue: {:?}", other),
            }
        }
        assert_eq!(
            user_updated_count, 2,
            "Combined name+bandwidth cascade must emit one broadcast per affected user (one fan-out × 2 listeners on the shared tx = 2), not one per changed field (would be 4)"
        );
    }

    /// Regression companion to the broadcast test: when the group's weight
    /// change resolves to the same effective value for every member (because
    /// they all have higher own overrides), no `UserUpdated` should fire.
    #[tokio::test]
    async fn test_group_update_bandwidth_no_broadcast_when_override_wins() {
        let mut test_ctx = create_test_context().await;

        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let group = test_ctx
            .db
            .groups
            .create_group(
                "Staff",
                false,
                &Permissions::from(&[Permission::ChatSend]),
                5,
            )
            .await
            .unwrap();

        // bob has his own override at 100, so changing group weight from 5
        // to 12 leaves his effective weight at 100.
        let bob = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: "hash",
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

        let bob_effective = test_ctx
            .db
            .users
            .get_user_permissions(bob.id)
            .await
            .unwrap();
        let mut bob_session_perms = bob_effective.permissions.clone();
        bob_session_perms.insert(db::Permission::UserList);
        let _ = test_ctx
            .user_manager
            .add_user(crate::users::user::NewSessionParams {
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
                group_id: Some(group.id),
                group_name: Some("Staff".to_string()),
                bandwidth_weight: 100,
                last_activity: Instant::now(),
            })
            .await
            .unwrap();

        let result = handle_group_update(
            group.id,
            None,
            None,
            None,
            Some(12),
            Some(admin_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse { success, .. } => assert!(success),
            other => panic!("Expected GroupUpdateResponse, got {:?}", other),
        }

        assert!(
            test_ctx.rx.try_recv().is_err(),
            "No UserUpdated broadcast when every member's resolved weight is unchanged"
        );
    }
}
