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
    HandlerContext, Outcome, ServerInfoOptions, ServerInfoValues,
    broadcast_user_updated_for_members, build_server_info, dispatch_outcome,
    err_bandwidth_weight_delegation, err_bandwidth_weight_zero, err_database,
    err_group_already_exists, err_group_name_empty, err_group_name_invalid,
    err_group_name_too_long, err_group_no_fields, err_group_not_empty_modify, err_group_not_found,
    err_group_shared_permission, err_not_logged_in, err_permission_denied,
    err_permissions_contains_newlines, err_permissions_empty_permission,
    err_permissions_invalid_characters, err_permissions_permission_too_long,
    err_permissions_too_many, err_unknown_permission, update_egress_user_weight,
};
use crate::constants::*;
use crate::db::{GroupPermissionWriteScope, Permission, Permissions};
use crate::voice::send_voice_leave_notifications;

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
    let Some(session_id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_GROUP_UPDATE_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_GROUP_UPDATE))
            .await;
    };

    // Guard lives only inside this block; all socket sends happen after it.
    let outcome = 'locked: {
        let _state_guard = ctx.user_manager.lock_user_state().await;
        let requesting_user = match ctx.user_manager.get_user_by_session_id(session_id).await {
            Some(u) => u,
            None => break 'locked Outcome::Disconnect,
        };

        if !requesting_user.has_permission(Permission::GroupEdit) {
            warn!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_GROUP_UPDATE_PERMISSION_DENIED);
            break 'locked Outcome::Send(Box::new(ServerMessage::GroupUpdateResponse {
                success: false,
                id: None,
                name: None,
                error: Some(err_permission_denied(ctx.locale)),
            }));
        }

        if name.is_none()
            && is_shared.is_none()
            && permissions.is_none()
            && bandwidth_weight.is_none()
        {
            break 'locked Outcome::Send(Box::new(ServerMessage::GroupUpdateResponse {
                success: false,
                id: None,
                name: None,
                error: Some(err_group_no_fields(ctx.locale)),
            }));
        }

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
            break 'locked Outcome::Send(Box::new(ServerMessage::GroupUpdateResponse {
                success: false,
                id: None,
                name: None,
                error: Some(error_msg),
            }));
        }

        if let Some(ref perms) = permissions
            && let Err(e) = validators::validate_permissions(perms)
        {
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
            break 'locked Outcome::Send(Box::new(ServerMessage::GroupUpdateResponse {
                success: false,
                id: None,
                name: None,
                error: Some(error_msg),
            }));
        }

        let group = match ctx.db.groups.get_group_by_id(id).await {
            Ok(Some(g)) => g,
            Ok(None) => {
                break 'locked Outcome::Send(Box::new(ServerMessage::GroupUpdateResponse {
                    success: false,
                    id: None,
                    name: None,
                    error: Some(err_group_not_found(ctx.locale)),
                }));
            }
            Err(e) => {
                error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_GROUP_UPDATE_DB_ERROR);
                break 'locked Outcome::Send(Box::new(ServerMessage::GroupUpdateResponse {
                    success: false,
                    id: None,
                    name: None,
                    error: Some(err_database(ctx.locale)),
                }));
            }
        };

        let current_permissions: Vec<Permission> = match ctx
            .db
            .groups
            .get_group_permissions(id)
            .await
        {
            Ok(p) => p,
            Err(e) => {
                error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_GROUP_UPDATE_DB_ERROR);
                break 'locked Outcome::Send(Box::new(ServerMessage::GroupUpdateResponse {
                    success: false,
                    id: None,
                    name: None,
                    error: Some(err_database(ctx.locale)),
                }));
            }
        };

        // Shared status toggle: if flipping `is_shared`, group must be empty.
        if let Some(new_shared) = is_shared
            && new_shared != group.is_shared
        {
            let member_count = match ctx.db.groups.get_member_count(id).await {
                Ok(c) => c,
                Err(e) => {
                    error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_GROUP_UPDATE_DB_ERROR);
                    break 'locked Outcome::Send(Box::new(ServerMessage::GroupUpdateResponse {
                        success: false,
                        id: None,
                        name: None,
                        error: Some(err_database(ctx.locale)),
                    }));
                }
            };

            if member_count > 0 {
                break 'locked Outcome::Send(Box::new(ServerMessage::GroupUpdateResponse {
                    success: false,
                    id: None,
                    name: None,
                    error: Some(err_group_not_empty_modify(ctx.locale)),
                }));
            }
        }

        let final_name = name.unwrap_or_else(|| group.name.clone());
        let final_is_shared = is_shared.unwrap_or(group.is_shared);

        // Parse requested permissions. Non-admins' `OwnedSubset` write only
        // touches rows they own, so a concurrent admin grant survives.
        let parsed_requested: Option<Vec<Permission>> = if let Some(ref requested_perms) =
            permissions
        {
            let mut parsed: Vec<Permission> = Vec::new();
            for perm_str in requested_perms {
                match Permission::parse(perm_str) {
                    Some(perm) => parsed.push(perm),
                    None => {
                        break 'locked Outcome::Send(Box::new(
                            ServerMessage::GroupUpdateResponse {
                                success: false,
                                id: None,
                                name: None,
                                error: Some(err_unknown_permission(ctx.locale, perm_str)),
                            },
                        ));
                    }
                }
            }

            // Non-admins can't grant a permission they don't hold.
            if !requesting_user.is_admin {
                for perm in &parsed {
                    if !requesting_user.has_permission(*perm) {
                        warn!(user = %requesting_user.username, ip = %ctx.peer_addr, perm = %perm.as_str(), "{}", LOG_GROUP_UPDATE_UNOWNED_PERMISSION);
                        break 'locked Outcome::Send(Box::new(
                            ServerMessage::GroupUpdateResponse {
                                success: false,
                                id: None,
                                name: None,
                                error: Some(err_permission_denied(ctx.locale)),
                            },
                        ));
                    }
                }
            }
            Some(parsed)
        } else {
            None
        };

        // Project the final set for shared-compat validation. Admin: the
        // requested set. Non-admin: (current ∖ owned) ∪ (owned ∩ requested),
        // i.e. preserved unowned perms plus requested owned perms. Snapshot-
        // based, so a racing admin grant could slip past validation (closing
        // it needs in-tx validation; deferred).
        let requester_owned: Vec<Permission> = if requesting_user.is_admin {
            Vec::new()
        } else {
            requesting_user.permissions.iter().copied().collect()
        };
        let projected_final: Vec<Permission> = match (&parsed_requested, requesting_user.is_admin) {
            (Some(req), true) => req.clone(),
            (Some(req), false) => {
                let mut out: Vec<Permission> = current_permissions
                    .iter()
                    .filter(|p| !requester_owned.contains(p))
                    .copied()
                    .collect();
                for p in req {
                    if !out.contains(p) {
                        out.push(*p);
                    }
                }
                out
            }
            (None, _) => current_permissions.clone(),
        };

        // Shared groups may only hold shared-account permissions.
        if final_is_shared {
            for perm in &projected_final {
                if !is_shared_account_permission(perm.as_str()) {
                    break 'locked Outcome::Send(Box::new(ServerMessage::GroupUpdateResponse {
                        success: false,
                        id: None,
                        name: None,
                        error: Some(err_group_shared_permission(ctx.locale)),
                    }));
                }
            }
        }

        // Old name/weight for diff detection. (Permission diffing uses the
        // in-tx `previous_permissions` from `update_group`, not the pre-tx
        // snapshot — see below — to avoid missing a cascade after a
        // concurrent update.)
        let old_name = group.name.clone();
        let old_bandwidth_weight = group.bandwidth_weight;

        // Requested set as-is; the DB layer applies ReplaceAll / OwnedSubset /
        // NoChange per `permission_write_scope`.
        let requested_permissions: Permissions = match &parsed_requested {
            Some(p) => Permissions::from(p.as_slice()),
            None => Permissions::new(),
        };
        let permission_write_scope = match (&parsed_requested, requesting_user.is_admin) {
            (Some(_), true) => GroupPermissionWriteScope::ReplaceAll,
            (Some(_), false) => GroupPermissionWriteScope::OwnedSubset(&requester_owned),
            (None, _) => GroupPermissionWriteScope::NoChange,
        };

        if !requesting_user.is_admin
            && let Some(w) = bandwidth_weight
            && w > requesting_user.bandwidth_weight.load(Ordering::Relaxed)
        {
            break 'locked Outcome::Send(Box::new(ServerMessage::GroupUpdateResponse {
                success: false,
                id: None,
                name: None,
                error: Some(err_bandwidth_weight_delegation(ctx.locale)),
            }));
        }
        if let Some(w) = bandwidth_weight
            && let Err(BandwidthWeightError::Zero) = validate_bandwidth_weight(w)
        {
            break 'locked Outcome::Send(Box::new(ServerMessage::GroupUpdateResponse {
                success: false,
                id: None,
                name: None,
                error: Some(err_bandwidth_weight_zero(ctx.locale, MIN_BANDWIDTH_WEIGHT)),
            }));
        }
        let final_bandwidth_weight = bandwidth_weight.unwrap_or(group.bandwidth_weight);

        match ctx
            .db
            .groups
            .update_group(
                id,
                &final_name,
                final_is_shared,
                &requested_permissions,
                final_bandwidth_weight,
                permission_write_scope,
            )
            .await
        {
            Ok(Some(crate::db::UpdateGroupResult {
                group: updated_group,
                previous_permissions,
                final_permissions,
            })) => {
                // Diffs below use the in-tx `updated_group` /
                // `previous_permissions` / `final_permissions`, not the
                // requested values or pre-tx snapshot.
                let response = ServerMessage::GroupUpdateResponse {
                    success: true,
                    id: Some(updated_group.id),
                    name: Some(updated_group.name.clone()),
                    error: None,
                };

                let name_changed = old_name != updated_group.name;
                let old_permissions: HashSet<Permission> =
                    previous_permissions.iter().copied().collect();
                let new_permissions: HashSet<Permission> =
                    final_permissions.iter().copied().collect();
                let permissions_changed = old_permissions != new_permissions;
                let bandwidth_weight_changed =
                    old_bandwidth_weight != updated_group.bandwidth_weight;

                let inheriting_set: HashSet<i64> = if bandwidth_weight_changed {
                    ctx.user_manager
                        .update_bandwidth_weight_for_group_inheritors(
                            id,
                            updated_group.bandwidth_weight,
                        )
                        .await
                } else {
                    HashSet::new()
                };
                let mut egress_update_user_ids = inheriting_set.clone();
                if bandwidth_weight_changed {
                    let active_transfer_user_ids = ctx.transfer_registry.active_user_ids();
                    if !active_transfer_user_ids.is_empty() {
                        match ctx
                            .db
                            .users
                            .get_group_bandwidth_inheritor_user_ids(id)
                            .await
                        {
                            Ok(group_inheritors) => {
                                for user_id in group_inheritors {
                                    if active_transfer_user_ids.contains(&user_id) {
                                        egress_update_user_ids.insert(user_id);
                                    }
                                }
                            }
                            Err(e) => {
                                error!(user = %requesting_user.username, ip = %ctx.peer_addr, group_id = id, err = %e, "{}", LOG_GROUP_UPDATE_DB_ERROR);
                            }
                        }
                    }
                }
                for user_id in &egress_update_user_ids {
                    update_egress_user_weight(ctx, *user_id, updated_group.bandwidth_weight);
                }

                if name_changed {
                    ctx.user_manager
                        .update_group_name(id, &updated_group.name)
                        .await;
                }

                let member_sessions =
                    if name_changed || !inheriting_set.is_empty() || permissions_changed {
                        ctx.user_manager.get_sessions_by_group_id(id).await
                    } else {
                        Vec::new()
                    };

                // (user_id, payload, voice_cleanup_needed) — keyed on the
                // immutable user_id so a concurrent rename can't drop sessions.
                let mut perm_updates: Vec<(i64, ServerMessage, bool)> = Vec::new();
                if permissions_changed {
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

                    let mut seen_user_ids: HashSet<i64> = HashSet::new();
                    for session in &member_sessions {
                        if !seen_user_ids.insert(session.user_id) {
                            continue;
                        }
                        let old_session_perms = session.permissions.clone();
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
                        ctx.user_manager
                            .update_permissions(session.user_id, new_effective.permissions.clone())
                            .await;
                        if old_session_perms == new_effective.permissions {
                            continue;
                        }

                        let permission_strings: Vec<String> = new_effective
                            .permissions
                            .iter()
                            .map(|p| p.as_str().to_string())
                            .collect();
                        let has_file_reindex =
                            new_effective.permissions.contains(&Permission::FileReindex);
                        let has_chat_join =
                            new_effective.permissions.contains(&Permission::ChatJoin);
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

                        let had_voice_listen = old_session_perms.contains(&Permission::VoiceListen);
                        let has_voice_listen =
                            new_effective.permissions.contains(&Permission::VoiceListen);
                        let voice_cleanup_needed = had_voice_listen && !has_voice_listen;

                        perm_updates.push((
                            session.user_id,
                            permissions_update,
                            voice_cleanup_needed,
                        ));
                    }
                }

                // Broadcasts + voice cleanup run under the lock (in-memory
                // sends) so a concurrent UserUpdate can't re-grant VoiceListen
                // between our cache write and cleanup. Keyed on user_id.
                if name_changed || !inheriting_set.is_empty() {
                    broadcast_user_updated_for_members(ctx, &member_sessions, None, |session| {
                        name_changed || inheriting_set.contains(&session.user_id)
                    })
                    .await;
                }

                for (user_id, permissions_update, voice_cleanup_needed) in perm_updates {
                    ctx.user_manager
                        .broadcast_to_user_id(user_id, &permissions_update)
                        .await;
                    if voice_cleanup_needed {
                        for session in ctx.user_manager.get_sessions_by_user_id(user_id).await {
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

                info!(user = %requesting_user.username, ip = %ctx.peer_addr, group = %updated_group.name, "{}", LOG_GROUP_UPDATE_SUCCESS);
                Outcome::Send(Box::new(response))
            }
            Ok(None) => {
                // 0 rows: group deleted concurrently, or the atomic shared-
                // toggle protection blocked it (members exist). The explicit
                // Err arm keeps a DB read failure from looking like "not found".
                let error = match ctx.db.groups.get_member_count(id).await {
                    Ok(count) if count > 0 => err_group_not_empty_modify(ctx.locale),
                    Ok(_) => err_group_not_found(ctx.locale),
                    Err(e) => {
                        error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_GROUP_UPDATE_DB_ERROR);
                        err_database(ctx.locale)
                    }
                };
                Outcome::Send(Box::new(ServerMessage::GroupUpdateResponse {
                    success: false,
                    id: None,
                    name: None,
                    error: Some(error),
                }))
            }
            Err(e) => {
                if ctx
                    .db
                    .groups
                    .get_group_by_name(&final_name)
                    .await
                    .ok()
                    .flatten()
                    .is_some_and(|existing| existing.id != id)
                {
                    Outcome::Send(Box::new(ServerMessage::GroupUpdateResponse {
                        success: false,
                        id: None,
                        name: None,
                        error: Some(err_group_already_exists(ctx.locale)),
                    }))
                } else {
                    error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_GROUP_UPDATE_DB_ERROR);
                    Outcome::Send(Box::new(ServerMessage::GroupUpdateResponse {
                        success: false,
                        id: None,
                        name: None,
                        error: Some(err_database(ctx.locale)),
                    }))
                }
            }
        }
    };

    dispatch_outcome(outcome, ctx, HANDLER_GROUP_UPDATE).await
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::time::Instant;

    use super::*;
    use crate::channels::JoinPolicy;
    use crate::db;
    use crate::egress::task::EgressSettingsCommand;
    use crate::handlers::testing::{create_test_context, login_user, read_server_message};
    use crate::transfers::registry::{TransferDirection, TransferRegistration};

    async fn add_online_group_member(
        test_ctx: &mut crate::handlers::testing::TestContext,
        username: &str,
        group_id: i64,
        group_name: &str,
        bandwidth_weight: u16,
    ) -> i64 {
        add_online_group_member_with_override(
            test_ctx,
            username,
            group_id,
            group_name,
            bandwidth_weight,
            None,
        )
        .await
    }

    async fn add_online_group_member_with_override(
        test_ctx: &mut crate::handlers::testing::TestContext,
        username: &str,
        group_id: i64,
        group_name: &str,
        bandwidth_weight: u16,
        bandwidth_weight_override: Option<u16>,
    ) -> i64 {
        let user = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username,
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &db::Permissions::new(),
                group_id: Some(group_id),
                revokes: &[],
                bandwidth_weight: bandwidth_weight_override,
            })
            .await
            .unwrap();

        test_ctx
            .user_manager
            .add_user(crate::users::user::NewSessionParams {
                session_id: 0,
                user_id: user.id,
                username: username.to_string(),
                is_admin: false,
                is_shared: false,
                permissions: HashSet::new(),
                address: test_ctx.peer_addr,
                created_at: 0,
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: DEFAULT_TEST_LOCALE.to_string(),
                avatar: None,
                nickname: username.to_string(),
                is_away: false,
                status: None,
                group_id: Some(group_id),
                group_name: Some(group_name.to_string()),
                bandwidth_weight,
                bandwidth_weight_override,
                last_activity: Instant::now(),
            })
            .await
            .unwrap();

        user.id
    }

    async fn add_transfer_only_group_member(
        test_ctx: &mut crate::handlers::testing::TestContext,
        username: &str,
        group_id: i64,
        bandwidth_weight_override: Option<u16>,
    ) -> i64 {
        let user = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username,
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &db::Permissions::new(),
                group_id: Some(group_id),
                revokes: &[],
                bandwidth_weight: bandwidth_weight_override,
            })
            .await
            .unwrap();

        let (_info, _rx) = test_ctx.transfer_registry.register(TransferRegistration {
            user_id: user.id,
            peer_addr: test_ctx.peer_addr,
            nickname: username.to_string(),
            username: username.to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Download,
            path: "/files/test.bin".to_string(),
            total_size: 1024,
        });

        user.id
    }

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

        // Rename GroupB to the name GroupA already holds.
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
    async fn test_group_update_duplicate_name_unicode() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::GroupEdit],
            false,
        )
        .await;

        test_ctx
            .db
            .groups
            .create_group("Équipe", false, &Permissions::new(), 1)
            .await
            .expect("create Équipe");
        let other = test_ctx
            .db
            .groups
            .create_group("Other", false, &Permissions::new(), 1)
            .await
            .expect("create Other");

        // Rename Other to a Unicode-case variant of Équipe — collides via the
        // folded name_lower, which ASCII NOCASE would miss.
        let result = handle_group_update(
            other.id,
            Some("équipe".to_string()),
            None,
            None,
            None,
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
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

        let group = test_ctx
            .db
            .groups
            .create_group("Staff", false, &Permissions::new(), 1)
            .await
            .expect("Failed to create group");

        // A member blocks the is_shared toggle below.
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

        let group = test_ctx
            .db
            .groups
            .create_group("Staff", false, &Permissions::new(), 1)
            .await
            .expect("Failed to create group");

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

        // Make it shared while keeping user_kick (forbidden for shared groups).
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

        let group = test_ctx
            .db
            .groups
            .create_group("Staff", false, &Permissions::new(), 1)
            .await
            .expect("Failed to create group");

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

        // Editor has GroupEdit + ChatSend but NOT UserKick (delegation boundary).
        let editor_session = login_user(
            &mut test_ctx,
            "editor",
            "password",
            &[Permission::GroupEdit, Permission::ChatSend],
            false,
        )
        .await;

        // Created via DB: the editor lacks group-create permission.
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

        // Request includes user_kick, which the editor doesn't own.
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

        // Editor owns GroupEdit + ChatSend + UserList.
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

        // Both requested perms are owned by the editor.
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

        // Editor has GroupEdit + ChatSend but NOT UserKick (delegation boundary).
        let editor_session = login_user(
            &mut test_ctx,
            "editor",
            "password",
            &[Permission::GroupEdit, Permission::ChatSend],
            false,
        )
        .await;

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

        // Editor sends only ChatSend; the unowned UserKick is preserved.
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

        // Editor owns GroupEdit + ChatSend + UserList but NOT UserKick.
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

        // Editor drops the owned UserList; the unowned UserKick stays.
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

    #[tokio::test]
    async fn test_group_update_permission_cascade_sends_permissions_updated() {
        let mut test_ctx = create_test_context().await;

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
                bandwidth_weight_override: None,
                last_activity: Instant::now(),
            })
            .await
            .unwrap();

        // Add user_kick to the group.
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

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse { success, .. } => assert!(success),
            _ => panic!("Expected GroupUpdateResponse"),
        }

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
                server_info,
                group_id,
                group_name,
            } => {
                assert!(!is_admin);
                assert!(permissions.contains(&"chat_send".to_string()));
                assert!(permissions.contains(&"user_kick".to_string()));
                assert_eq!(group_id, Some(group.id));
                assert_eq!(group_name, Some("Staff".to_string()));

                let info = server_info.expect("server_info should be included");
                // All-user fields populated; admin-only fields and image omitted for non-admin bob.
                assert!(info.name.is_some());
                assert!(info.version.is_some());
                assert!(info.chat_burst_limit.is_some());
                assert!(info.chat_rate_limit.is_some());
                assert!(info.min_password_strength.is_some());
                assert!(info.log_level.is_some());
                assert!(info.persistent_channels.is_none());
                assert!(info.image.is_none());
            }
            _ => panic!("Expected PermissionsUpdated, got {:?}", msg),
        }

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

        // Grant UserList on top of group perms so bob receives UserUpdated.
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
                bandwidth_weight_override: None,
                last_activity: Instant::now(),
            })
            .await
            .unwrap();

        // Rename only (no permission change).
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

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse { success, name, .. } => {
                assert!(success);
                assert_eq!(name, Some("Moderators".to_string()));
            }
            _ => panic!("Expected GroupUpdateResponse"),
        }

        let (msg, _) = test_ctx
            .rx
            .recv()
            .await
            .expect("Should receive UserUpdated")
            .expect_message();
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

        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Member exists in DB only (offline — not in UserManager).
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

        // bob is offline, so no PermissionsUpdated should fire.
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

        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

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

        let bob_effective = test_ctx
            .db
            .users
            .get_user_permissions(bob.id)
            .await
            .unwrap();

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
                bandwidth_weight_override: None,
                last_activity: Instant::now(),
            })
            .await
            .unwrap();

        // bob joins a channel and voice.
        let _ = test_ctx
            .channel_manager
            .join("#general", bob_session, JoinPolicy::CreateIfMissing)
            .await;

        let voice_session = crate::voice::VoiceSession::new(
            "voicebob".to_string(),
            vec!["#general".to_string()],
            bob_session,
        );
        test_ctx
            .voice_registry
            .add(voice_session)
            .await
            .expect("test setup: session_id is unique");

        assert!(test_ctx.voice_registry.has_session(bob_session).await);

        // Drop voice_listen from the group (keep chat_send).
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

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse { success, .. } => assert!(success),
            _ => panic!("Expected GroupUpdateResponse"),
        }

        assert!(
            !test_ctx.voice_registry.has_session(bob_session).await,
            "User should be kicked from voice when voice_listen is revoked via group edit"
        );
    }

    #[tokio::test]
    async fn test_group_update_permission_cascade_with_member_overrides() {
        let mut test_ctx = create_test_context().await;

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

        // bob: grant override user_list, revoke override user_kick.
        // Effective before: (chat_send, user_kick) ∪ (user_list) − (user_kick) = chat_send, user_list
        let mut bob_perms = db::Permissions::new();
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
                bandwidth_weight_override: None,
                last_activity: Instant::now(),
            })
            .await
            .unwrap();

        // New group perms chat_send + ban_create; bob's effective becomes
        // chat_send, ban_create, user_list (the user_kick revoke is now a no-op).
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

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse { success, .. } => assert!(success),
            _ => panic!("Expected GroupUpdateResponse"),
        }

        let (msg, _) = test_ctx
            .rx
            .recv()
            .await
            .expect("Should receive PermissionsUpdated")
            .expect_message();
        match msg {
            ServerMessage::PermissionsUpdated { permissions, .. } => {
                assert!(permissions.contains(&"chat_send".to_string()));
                assert!(permissions.contains(&"ban_create".to_string()));
                assert!(permissions.contains(&"user_list".to_string()));
                // Absent: revoke override + dropped from group.
                assert!(!permissions.contains(&"user_kick".to_string()));
            }
            _ => panic!("Expected PermissionsUpdated, got {:?}", msg),
        }

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

        // bob revokes user_kick → effective (chat_send, user_kick) − (user_kick) = chat_send.
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
                bandwidth_weight_override: None,
                last_activity: Instant::now(),
            })
            .await
            .unwrap();

        // Drop user_kick from the group — bob already revoked it, so his
        // effective set stays chat_send (no change → no cascade).
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

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse { success, .. } => assert!(success),
            _ => panic!("Expected GroupUpdateResponse"),
        }

        assert!(
            test_ctx.rx.try_recv().is_err(),
            "No PermissionsUpdated when effective permissions unchanged for member"
        );

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

        // Grant UserList on top of group perms so bob receives broadcasts.
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
                bandwidth_weight_override: None,
                last_activity: Instant::now(),
            })
            .await
            .unwrap();

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

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse { success, .. } => assert!(success),
            _ => panic!("Expected GroupUpdateResponse"),
        }

        // Expect both UserUpdated (rename) and PermissionsUpdated (perm change).
        // broadcast_to_permission fans out to all UserList holders on the
        // shared tx, so drain everything and check both types appear.
        let mut got_user_updated = false;
        let mut got_permissions_updated = false;

        while let Ok(event) = test_ctx.rx.try_recv() {
            let (msg, _) = event.expect_message();
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
                _ => {}
            }
        }

        assert!(got_user_updated, "Should have received UserUpdated");
        assert!(
            got_permissions_updated,
            "Should have received PermissionsUpdated"
        );

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
                bandwidth_weight_override: None,
                last_activity: Instant::now(),
            })
            .await
            .unwrap();

        // Same name and permissions → no cascade.
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

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse { success, .. } => assert!(success),
            _ => panic!("Expected GroupUpdateResponse"),
        }

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
                bandwidth_weight_override: None,
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
        while let Ok(event) = test_ctx.rx.try_recv() {
            let (msg, _) = event.expect_message();
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
                bandwidth_weight_override: None,
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
            .expect("Should receive UserUpdated broadcast")
            .expect_message();
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

        match test_ctx.egress_settings_rx.try_recv() {
            Ok(EgressSettingsCommand::UpdateUserWeight { user_id, weight }) => {
                assert_eq!(user_id, bob.id);
                assert_eq!(weight, 12);
            }
            _ => panic!("Expected egress UpdateUserWeight command"),
        }
    }

    #[tokio::test]
    async fn test_group_update_bandwidth_cascade_updates_egress_for_each_online_member() {
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

        let bob_id = add_online_group_member(&mut test_ctx, "bob", group.id, "Staff", 5).await;
        let carol_id = add_online_group_member(&mut test_ctx, "carol", group.id, "Staff", 5).await;

        let result = handle_group_update(
            group.id,
            None,
            None,
            None,
            Some(13),
            Some(admin_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse { success, error, .. } => {
                assert!(success, "group update should succeed: {:?}", error);
            }
            other => panic!("Expected GroupUpdateResponse, got {:?}", other),
        }

        let mut updates = Vec::new();
        for _ in 0..2 {
            match test_ctx.egress_settings_rx.try_recv() {
                Ok(EgressSettingsCommand::UpdateUserWeight { user_id, weight }) => {
                    updates.push((user_id, weight));
                }
                _ => panic!("Expected egress UpdateUserWeight command"),
            }
        }
        updates.sort_unstable();

        let mut expected = vec![(bob_id, 13), (carol_id, 13)];
        expected.sort_unstable();
        assert_eq!(updates, expected);
        assert!(
            test_ctx.egress_settings_rx.try_recv().is_err(),
            "group cascade should emit exactly one egress update per affected online user"
        );
    }

    #[tokio::test]
    async fn test_group_update_bandwidth_cascade_skips_override_members_for_egress() {
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

        let inheritor_id =
            add_online_group_member(&mut test_ctx, "bob", group.id, "Staff", 5).await;
        let override_id = add_online_group_member_with_override(
            &mut test_ctx,
            "carol",
            group.id,
            "Staff",
            100,
            Some(100),
        )
        .await;

        let result = handle_group_update(
            group.id,
            None,
            None,
            None,
            Some(13),
            Some(admin_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse { success, error, .. } => {
                assert!(success, "group update should succeed: {:?}", error);
            }
            other => panic!("Expected GroupUpdateResponse, got {:?}", other),
        }

        match test_ctx.egress_settings_rx.try_recv() {
            Ok(EgressSettingsCommand::UpdateUserWeight { user_id, weight }) => {
                assert_eq!(user_id, inheritor_id);
                assert_eq!(weight, 13);
                assert_ne!(user_id, override_id);
            }
            _ => panic!("Expected egress UpdateUserWeight command"),
        }
        assert!(
            test_ctx.egress_settings_rx.try_recv().is_err(),
            "override members should not receive egress updates for inherited group weight changes"
        );
    }

    #[tokio::test]
    async fn test_group_update_bandwidth_cascade_updates_transfer_only_inheritors() {
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

        let inheritor_id =
            add_transfer_only_group_member(&mut test_ctx, "bob", group.id, None).await;
        let override_id =
            add_transfer_only_group_member(&mut test_ctx, "carol", group.id, Some(100)).await;

        let result = handle_group_update(
            group.id,
            None,
            None,
            None,
            Some(13),
            Some(admin_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::GroupUpdateResponse { success, error, .. } => {
                assert!(success, "group update should succeed: {:?}", error);
            }
            other => panic!("Expected GroupUpdateResponse, got {:?}", other),
        }

        match test_ctx.egress_settings_rx.try_recv() {
            Ok(EgressSettingsCommand::UpdateUserWeight { user_id, weight }) => {
                assert_eq!(user_id, inheritor_id);
                assert_eq!(weight, 13);
                assert_ne!(user_id, override_id);
            }
            _ => panic!("Expected egress UpdateUserWeight command"),
        }
        assert!(
            test_ctx.egress_settings_rx.try_recv().is_err(),
            "transfer override members should not receive inherited group weight updates"
        );
    }

    #[tokio::test]
    async fn test_group_update_transfer_inheritors_use_current_db_membership() {
        let mut test_ctx = create_test_context().await;

        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;
        let old_group = test_ctx
            .db
            .groups
            .create_group("OldStaff", false, &Permissions::new(), 5)
            .await
            .unwrap();
        let new_group = test_ctx
            .db
            .groups
            .create_group("NewStaff", false, &Permissions::new(), 7)
            .await
            .unwrap();
        let bob_id = add_transfer_only_group_member(&mut test_ctx, "bob", old_group.id, None).await;

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
                group_id: Some(new_group.id),
                bandwidth_weight: None,
                inherit_bandwidth_weight: false,
                requester_is_admin: true,
                permission_write_scope: db::PermissionWriteScope::ReplaceAll,
                requester_bandwidth_max: None,
            })
            .await
            .unwrap();

        let result = handle_group_update(
            old_group.id,
            None,
            None,
            None,
            Some(11),
            Some(admin_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::GroupUpdateResponse { success, error, .. } => {
                assert!(success, "old group update should succeed: {:?}", error);
            }
            other => panic!("Expected GroupUpdateResponse, got {:?}", other),
        }
        assert!(
            test_ctx.egress_settings_rx.try_recv().is_err(),
            "old group update must not target a transfer whose user moved away"
        );

        let result = handle_group_update(
            new_group.id,
            None,
            None,
            None,
            Some(17),
            Some(admin_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::GroupUpdateResponse { success, error, .. } => {
                assert!(success, "new group update should succeed: {:?}", error);
            }
            other => panic!("Expected GroupUpdateResponse, got {:?}", other),
        }

        match test_ctx.egress_settings_rx.try_recv() {
            Ok(EgressSettingsCommand::UpdateUserWeight { user_id, weight }) => {
                assert_eq!(user_id, bob_id);
                assert_eq!(weight, 17);
            }
            _ => panic!("Expected egress UpdateUserWeight command for new group"),
        }
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
                bandwidth_weight_override: None,
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
        while let Ok(event) = test_ctx.rx.try_recv() {
            let (msg, _) = event.expect_message();
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
                bandwidth_weight_override: Some(100),
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
        assert!(
            test_ctx.egress_settings_rx.try_recv().is_err(),
            "No egress weight update when every member's resolved weight is unchanged"
        );
    }
}
