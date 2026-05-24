//! Login message handler

use std::io;

use tokio::io::AsyncWrite;
use tracing::{debug, error, info, warn};

use nexus_common::names::fold_name;
use nexus_common::protocol::{ChannelJoinInfo, ServerMessage, UserInfo};
use nexus_common::validators::{
    self, AvatarError, FeaturesError, LocaleError, NicknameError, PasswordError, UsernameError,
};

use super::{
    HandlerContext, ServerInfoOptions, ServerInfoValues, build_server_info, current_timestamp,
    err_account_disabled, err_account_type_changed, err_already_logged_in, err_authentication,
    err_avatar_invalid_format, err_avatar_too_large, err_avatar_unsupported_type, err_database,
    err_failed_to_create_user, err_features_empty_feature, err_features_feature_too_long,
    err_features_invalid_characters, err_features_too_many, err_guest_disabled,
    err_handshake_required, err_invalid_credentials, err_locale_invalid_characters,
    err_locale_too_long, err_login_bandwidth_failed, err_login_group_failed,
    err_login_permissions_failed, err_nickname_empty, err_nickname_in_use, err_nickname_invalid,
    err_nickname_is_username, err_nickname_required, err_nickname_too_long, err_password_too_long,
    err_username_empty, err_username_invalid, err_username_too_long,
};
use crate::constants::{
    FEATURE_CHAT, HANDLER_LOGIN, LOG_BANDWIDTH_WEIGHT_RESOLVE_FAILED, LOG_LOGIN_ACCOUNT_DISABLED,
    LOG_LOGIN_ACCOUNT_STATE_CHANGED, LOG_LOGIN_ACCOUNT_TYPE_CHANGED, LOG_LOGIN_ALREADY_LOGGED_IN,
    LOG_LOGIN_CREATE_USER_ERROR, LOG_LOGIN_DB_ERROR, LOG_LOGIN_DB_NICKNAME, LOG_LOGIN_FIRST_ADMIN,
    LOG_LOGIN_GROUP_ERROR, LOG_LOGIN_HANDSHAKE_REQUIRED, LOG_LOGIN_HASH_ERROR,
    LOG_LOGIN_INVALID_CREDENTIALS, LOG_LOGIN_PASSWORD_CHANGED, LOG_LOGIN_PASSWORD_VERIFY_ERROR,
    LOG_LOGIN_PERMISSIONS_ERROR, LOG_LOGIN_RENAMED_MID_LOGIN, LOG_LOGIN_SUCCESS,
};
use crate::db::sql::GUEST_USERNAME;
use crate::db::{self, LoginSnapshotError, Permission};
use crate::users::manager::{AddUserError, UserManager};
use crate::users::user::NewSessionParams;

/// Login request parameters
pub struct LoginRequest {
    pub username: String,
    pub password: String,
    pub features: Vec<String>,
    pub locale: String,
    pub avatar: Option<String>,
    pub nickname: Option<String>,
    pub handshake_complete: bool,
}

fn handle_login_snapshot_error(
    err: &LoginSnapshotError,
    username: &str,
    peer: std::net::SocketAddr,
    locale: &str,
) -> String {
    let (msg, e, client_err) = match err {
        LoginSnapshotError::User(e) => (LOG_LOGIN_DB_ERROR, e, err_database(locale)),
        LoginSnapshotError::Permissions(e) => (
            LOG_LOGIN_PERMISSIONS_ERROR,
            e,
            err_login_permissions_failed(locale),
        ),
        LoginSnapshotError::Group(e) => (LOG_LOGIN_GROUP_ERROR, e, err_login_group_failed(locale)),
        LoginSnapshotError::BandwidthWeight(e) => (
            LOG_BANDWIDTH_WEIGHT_RESOLVE_FAILED,
            e,
            err_login_bandwidth_failed(locale),
        ),
    };
    error!(user = %username, ip = %peer, err = %e, "{}", msg);
    client_err
}

pub async fn handle_login<W>(
    request: LoginRequest,
    session_id: &mut Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let LoginRequest {
        username: raw_username,
        password,
        features,
        locale,
        avatar,
        nickname,
        handshake_complete,
    } = request;

    // Empty username means guest login.
    let username = if raw_username.is_empty() {
        GUEST_USERNAME.to_string()
    } else {
        raw_username
    };

    if !handshake_complete {
        warn!(ip = %ctx.peer_addr, "{}", LOG_LOGIN_HANDSHAKE_REQUIRED);
        return ctx
            .send_error_and_disconnect(&err_handshake_required(&locale), Some(HANDLER_LOGIN))
            .await;
    }

    if session_id.is_some() {
        warn!(ip = %ctx.peer_addr, "{}", LOG_LOGIN_ALREADY_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_already_logged_in(&locale), Some(HANDLER_LOGIN))
            .await;
    }

    if let Err(e) = validators::validate_username(&username) {
        let error_msg = match e {
            UsernameError::Empty => err_username_empty(&locale),
            UsernameError::TooLong => {
                err_username_too_long(&locale, validators::MAX_USERNAME_LENGTH)
            }
            UsernameError::InvalidCharacters => err_username_invalid(&locale),
        };
        return ctx
            .send_error_and_disconnect(&error_msg, Some(HANDLER_LOGIN))
            .await;
    }

    // Empty password is allowed (guest login); only length is rejected here.
    if let Err(PasswordError::TooLong) = validators::validate_password_input(&password) {
        return ctx
            .send_error_and_disconnect(
                &err_password_too_long(&locale, validators::MAX_PASSWORD_LENGTH),
                Some(HANDLER_LOGIN),
            )
            .await;
    }

    if let Err(e) = validators::validate_locale(&locale) {
        let error_msg = match e {
            LocaleError::TooLong => err_locale_too_long(&locale, validators::MAX_LOCALE_LENGTH),
            LocaleError::InvalidCharacters => err_locale_invalid_characters(&locale),
        };
        return ctx
            .send_error_and_disconnect(&error_msg, Some(HANDLER_LOGIN))
            .await;
    }

    if let Err(e) = validators::validate_features(&features) {
        let error_msg = match e {
            FeaturesError::TooMany => {
                err_features_too_many(&locale, validators::MAX_FEATURES_COUNT)
            }
            FeaturesError::EmptyFeature => err_features_empty_feature(&locale),
            FeaturesError::FeatureTooLong => {
                err_features_feature_too_long(&locale, validators::MAX_FEATURE_LENGTH)
            }
            FeaturesError::InvalidCharacters => err_features_invalid_characters(&locale),
        };
        return ctx
            .send_error_and_disconnect(&error_msg, Some(HANDLER_LOGIN))
            .await;
    }

    if let Some(ref avatar_data) = avatar
        && let Err(e) = validators::validate_avatar(avatar_data)
    {
        let error_msg = match e {
            AvatarError::TooLarge => {
                err_avatar_too_large(&locale, validators::MAX_AVATAR_DATA_URI_LENGTH)
            }
            AvatarError::InvalidFormat => err_avatar_invalid_format(&locale),
            AvatarError::UnsupportedType => err_avatar_unsupported_type(&locale),
        };
        return ctx
            .send_error_and_disconnect(&error_msg, Some(HANDLER_LOGIN))
            .await;
    }

    let account = match ctx.db.users.get_user_by_username(&username).await {
        Ok(acc) => acc,
        Err(e) => {
            error!(ip = %ctx.peer_addr, target = %username, err = %e, "{}", LOG_LOGIN_DB_ERROR);
            return ctx
                .send_error_and_disconnect(&err_database(&locale), Some(HANDLER_LOGIN))
                .await;
        }
    };

    let authenticated_account = if let Some(account) = account {
        // Guest accounts have an empty hash, which requires an empty password.
        let password_valid = if account.hashed_password.is_empty() {
            password.is_empty()
        } else {
            match db::verify_password_async(password.clone(), account.hashed_password.clone()).await
            {
                Ok(valid) => valid,
                Err(e) => {
                    error!(ip = %ctx.peer_addr, target = %username, err = %e, "{}", LOG_LOGIN_PASSWORD_VERIFY_ERROR);
                    return ctx
                        .send_error_and_disconnect(
                            &err_authentication(&locale),
                            Some(HANDLER_LOGIN),
                        )
                        .await;
                }
            }
        };

        if password_valid {
            if !account.enabled {
                warn!(ip = %ctx.peer_addr, target = %username, "{}", LOG_LOGIN_ACCOUNT_DISABLED);
                let error_msg = if fold_name(&username) == GUEST_USERNAME {
                    err_guest_disabled(&locale)
                } else {
                    err_account_disabled(&locale, &username)
                };
                return ctx
                    .send_error_and_disconnect(&error_msg, Some(HANDLER_LOGIN))
                    .await;
            }
            account
        } else {
            warn!(ip = %ctx.peer_addr, target = %username, "{}", LOG_LOGIN_INVALID_CREDENTIALS);
            return ctx
                .send_error_and_disconnect(&err_invalid_credentials(&locale), Some(HANDLER_LOGIN))
                .await;
        }
    } else {
        let min_strength = ctx.db.config.get_min_password_strength().await;
        let hashed_password = match db::hash_password_async(password.clone(), min_strength, false)
            .await
        {
            Ok(hash) => hash,
            Err(e) => {
                error!(ip = %ctx.peer_addr, target = %username, err = %e, "{}", LOG_LOGIN_HASH_ERROR);
                return ctx
                    .send_error_and_disconnect(
                        &err_failed_to_create_user(&locale, &username),
                        Some(HANDLER_LOGIN),
                    )
                    .await;
            }
        };

        // First user becomes admin; the DB method enforces atomicity.
        match ctx
            .db
            .users
            .create_first_user_if_none_exist(&username, &hashed_password)
            .await
        {
            Ok(Some(account)) => {
                info!(user = %username, ip = %ctx.peer_addr, "{}", LOG_LOGIN_FIRST_ADMIN);
                account
            }
            Ok(None) => {
                // Reuse the invalid-credentials error so we don't reveal whether
                // the username exists.
                return ctx
                    .send_error_and_disconnect(
                        &err_invalid_credentials(&locale),
                        Some(HANDLER_LOGIN),
                    )
                    .await;
            }
            Err(e) => {
                error!(ip = %ctx.peer_addr, target = %username, err = %e, "{}", LOG_LOGIN_CREATE_USER_ERROR);
                return ctx
                    .send_error_and_disconnect(
                        &err_failed_to_create_user(&locale, &username),
                        Some(HANDLER_LOGIN),
                    )
                    .await;
            }
        }
    };

    // Shared accounts require a unique nickname; regular accounts ignore it.
    let validated_nickname = if authenticated_account.is_shared {
        let Some(nickname) = nickname else {
            return ctx
                .send_error_and_disconnect(&err_nickname_required(&locale), Some(HANDLER_LOGIN))
                .await;
        };

        if let Err(e) = validators::validate_nickname(&nickname) {
            let error_msg = match e {
                NicknameError::Empty => err_nickname_empty(&locale),
                NicknameError::TooLong => {
                    err_nickname_too_long(&locale, validators::MAX_NICKNAME_LENGTH)
                }
                NicknameError::InvalidCharacters => err_nickname_invalid(&locale),
            };
            return ctx
                .send_error_and_disconnect(&error_msg, Some(HANDLER_LOGIN))
                .await;
        }

        // A nickname must not collide with any existing username (case-insensitive).
        match ctx.db.users.username_exists(&nickname).await {
            Ok(true) => {
                return ctx
                    .send_error_and_disconnect(
                        &err_nickname_is_username(&locale),
                        Some(HANDLER_LOGIN),
                    )
                    .await;
            }
            Ok(false) => {}
            Err(e) => {
                error!(ip = %ctx.peer_addr, target = %username, err = %e, "{}", LOG_LOGIN_DB_NICKNAME);
                return ctx
                    .send_error_and_disconnect(&err_database(&locale), Some(HANDLER_LOGIN))
                    .await;
            }
        }

        // …nor with an active session's nickname (case-insensitive).
        if ctx.user_manager.is_nickname_in_use(&nickname).await {
            return ctx
                .send_error_and_disconnect(&err_nickname_in_use(&locale), Some(HANDLER_LOGIN))
                .await;
        }

        Some(nickname)
    } else {
        None
    };

    let pre_snapshot = match ctx
        .db
        .get_login_session_snapshot(authenticated_account.id)
        .await
    {
        Ok(Some(s)) if s.account.enabled => s,
        Ok(Some(_)) => {
            return ctx
                .send_error_and_disconnect(
                    &err_account_disabled(&locale, &authenticated_account.username),
                    Some(HANDLER_LOGIN),
                )
                .await;
        }
        Ok(None) => {
            return ctx
                .send_error_and_disconnect(&err_authentication(&locale), Some(HANDLER_LOGIN))
                .await;
        }
        Err(e) => {
            let client_err = handle_login_snapshot_error(
                &e,
                &authenticated_account.username,
                ctx.peer_addr,
                &locale,
            );
            return ctx
                .send_error_and_disconnect(&client_err, Some(HANDLER_LOGIN))
                .await;
        }
    };

    // A rename or password reset during password verify means the sent
    // credentials no longer describe this row — reject as stale.
    if fold_name(&username) != fold_name(&pre_snapshot.account.username) {
        warn!(
            user = %username,
            new_username = %pre_snapshot.account.username,
            ip = %ctx.peer_addr,
            "{}", LOG_LOGIN_RENAMED_MID_LOGIN
        );
        return ctx
            .send_error_and_disconnect(&err_invalid_credentials(&locale), Some(HANDLER_LOGIN))
            .await;
    }
    if authenticated_account.hashed_password != pre_snapshot.account.hashed_password {
        warn!(
            user = %authenticated_account.username,
            ip = %ctx.peer_addr,
            "{}", LOG_LOGIN_PASSWORD_CHANGED
        );
        return ctx
            .send_error_and_disconnect(&err_invalid_credentials(&locale), Some(HANDLER_LOGIN))
            .await;
    }

    let authenticated_account = pre_snapshot.account;
    let cached_permissions = pre_snapshot.permissions.permissions;
    let group_id = authenticated_account.group_id;
    let group_name = pre_snapshot.group_name;
    let bandwidth_weight = pre_snapshot.resolved_bandwidth_weight;

    let has_chat_feature = features.iter().any(|f| f == FEATURE_CHAT);

    // Regular accounts inherit is_away/status from the latest existing
    // session so a multi-device login doesn't clear away state.
    let (inherited_is_away, inherited_status) = if !authenticated_account.is_shared {
        let existing_sessions = ctx
            .user_manager
            .get_sessions_by_username(&authenticated_account.username)
            .await;
        if let Some(latest) = existing_sessions.iter().max_by_key(|s| s.login_time) {
            (latest.is_away, latest.status.clone())
        } else {
            (false, None)
        }
    } else {
        (false, None)
    };

    // Shared nicknames share one namespace with usernames, so admission must
    // serialize against username renames (which hold user_state_lock).
    // Re-check username_exists under the lock: the early pre-check is stale if
    // a rename committed since — including an offline account renamed into the
    // nickname, which leaves no active session for add_user to catch. The guard
    // drops at the block's end, before any socket I/O. Regular accounts
    // (nickname == username, DB-unique) skip the lock.
    let admission: Result<u32, String> = {
        let _guard = if validated_nickname.is_some() {
            Some(ctx.user_manager.lock_user_state().await)
        } else {
            None
        };

        let username_conflict = if let Some(ref nickname) = validated_nickname {
            match ctx.db.users.username_exists(nickname).await {
                Ok(exists) => exists.then(|| err_nickname_is_username(&locale)),
                Err(e) => {
                    error!(ip = %ctx.peer_addr, target = %username, err = %e, "{}", LOG_LOGIN_DB_NICKNAME);
                    Some(err_database(&locale))
                }
            }
        } else {
            None
        };

        if let Some(error_msg) = username_conflict {
            Err(error_msg)
        } else {
            // add_user rechecks active-nickname uniqueness and inserts
            // atomically under users.write.
            match ctx
                .user_manager
                .add_user(NewSessionParams {
                    session_id: 0, // Will be assigned by add_user
                    user_id: authenticated_account.id,
                    username: authenticated_account.username.clone(),
                    is_admin: authenticated_account.is_admin,
                    is_shared: authenticated_account.is_shared,
                    permissions: cached_permissions.clone(),
                    address: ctx.peer_addr,
                    created_at: authenticated_account.created_at,
                    tx: ctx.tx.clone(),
                    features,
                    locale: locale.clone(),
                    avatar: avatar.clone(),
                    nickname: validated_nickname
                        .clone()
                        .unwrap_or_else(|| authenticated_account.username.clone()),
                    is_away: inherited_is_away,
                    status: inherited_status,
                    group_id,
                    group_name: group_name.clone(),
                    bandwidth_weight,
                    bandwidth_weight_override: authenticated_account.bandwidth_weight,
                    last_activity: std::time::Instant::now(),
                })
                .await
            {
                Ok(id) => Ok(id),
                Err(AddUserError::NicknameInUse) => Err(err_nickname_in_use(&locale)),
            }
        }
    };

    let id = match admission {
        Ok(id) => id,
        Err(error_msg) => {
            return ctx
                .send_error_and_disconnect(&error_msg, Some(HANDLER_LOGIN))
                .await;
        }
    };
    *session_id = Some(id);

    // Re-snapshot to catch mutations that committed during add_user,
    // including delete/disable races whose cleanup ran before our session
    // was registered.
    let post_snapshot = match ctx
        .db
        .get_login_session_snapshot(authenticated_account.id)
        .await
    {
        Ok(Some(s)) if s.account.enabled => s,
        Ok(Some(disabled)) => {
            ctx.user_manager.remove_user(id).await;
            *session_id = None;
            return ctx
                .send_error_and_disconnect(
                    &err_account_disabled(&locale, &disabled.account.username),
                    Some(HANDLER_LOGIN),
                )
                .await;
        }
        Ok(None) => {
            ctx.user_manager.remove_user(id).await;
            *session_id = None;
            return ctx
                .send_error_and_disconnect(&err_authentication(&locale), Some(HANDLER_LOGIN))
                .await;
        }
        Err(e) => {
            let client_err = handle_login_snapshot_error(
                &e,
                &authenticated_account.username,
                ctx.peer_addr,
                &locale,
            );
            ctx.user_manager.remove_user(id).await;
            *session_id = None;
            return ctx
                .send_error_and_disconnect(&client_err, Some(HANDLER_LOGIN))
                .await;
        }
    };

    // Auth-field buckets come before the state bucket so a password
    // reset coinciding with state drift logs the auth-specific event.
    if fold_name(&authenticated_account.username) != fold_name(&post_snapshot.account.username) {
        warn!(
            user = %authenticated_account.username,
            new_username = %post_snapshot.account.username,
            ip = %ctx.peer_addr,
            "{}", LOG_LOGIN_RENAMED_MID_LOGIN
        );
        ctx.user_manager.remove_user(id).await;
        *session_id = None;
        return ctx
            .send_error_and_disconnect(&err_invalid_credentials(&locale), Some(HANDLER_LOGIN))
            .await;
    }
    if authenticated_account.hashed_password != post_snapshot.account.hashed_password {
        warn!(
            user = %authenticated_account.username,
            ip = %ctx.peer_addr,
            "{}", LOG_LOGIN_PASSWORD_CHANGED
        );
        ctx.user_manager.remove_user(id).await;
        *session_id = None;
        return ctx
            .send_error_and_disconnect(&err_invalid_credentials(&locale), Some(HANDLER_LOGIN))
            .await;
    }
    if authenticated_account.is_shared != post_snapshot.account.is_shared {
        warn!(
            user = %authenticated_account.username,
            was_shared = authenticated_account.is_shared,
            now_shared = post_snapshot.account.is_shared,
            ip = %ctx.peer_addr,
            "{}", LOG_LOGIN_ACCOUNT_TYPE_CHANGED
        );
        ctx.user_manager.remove_user(id).await;
        *session_id = None;
        return ctx
            .send_error_and_disconnect(&err_account_type_changed(&locale), Some(HANDLER_LOGIN))
            .await;
    }
    if authenticated_account.is_admin != post_snapshot.account.is_admin
        || cached_permissions != post_snapshot.permissions.permissions
        || authenticated_account.group_id != post_snapshot.account.group_id
        || group_name != post_snapshot.group_name
        || bandwidth_weight != post_snapshot.resolved_bandwidth_weight
    {
        warn!(
            user = %authenticated_account.username,
            ip = %ctx.peer_addr,
            "{}", LOG_LOGIN_ACCOUNT_STATE_CHANGED
        );
        ctx.user_manager.remove_user(id).await;
        *session_id = None;
        return ctx
            .send_error_and_disconnect(&err_authentication(&locale), Some(HANDLER_LOGIN))
            .await;
    }

    let has_chat_join_permission =
        authenticated_account.is_admin || cached_permissions.contains(&Permission::ChatJoin);
    let has_chat_create_permission =
        authenticated_account.is_admin || cached_permissions.contains(&Permission::ChatCreate);
    let has_voice_listen_permission =
        authenticated_account.is_admin || cached_permissions.contains(&Permission::VoiceListen);
    let can_auto_join = has_chat_feature && has_chat_join_permission;

    let config = ctx.db.config.get_all().await;

    // Admin-configured auto-join channels (distinct from persistent channels:
    // these are joined on login, not restart-surviving). can_auto_join was
    // computed before add_user(), since `features` is moved into it.
    let auto_join_channel_names = if can_auto_join {
        crate::db::ConfigDb::parse_channel_list(&config.auto_join_channels)
    } else {
        Vec::new()
    };

    // Captured before the loop (user not yet in UserManager). Use the
    // DB-canonical username for consistent casing.
    let joining_user_nickname = validated_nickname
        .clone()
        .unwrap_or_else(|| authenticated_account.username.clone());
    let joining_user_is_admin = authenticated_account.is_admin;
    let joining_user_is_shared = authenticated_account.is_shared;

    let auto_join_policy = if has_chat_create_permission {
        crate::channels::JoinPolicy::CreateIfMissing
    } else {
        crate::channels::JoinPolicy::ExistingOnly
    };

    let mut joined_channels = Vec::new();
    for channel_name in auto_join_channel_names {
        // Skip on any error (missing channel + no ChatCreate, at limit, …).
        let Ok(result) = ctx
            .channel_manager
            .join(&channel_name, id, auto_join_policy)
            .await
        else {
            continue;
        };

        // Build member list as unique nicknames (member counts are nicknames, not sessions).
        let member_nicknames = ctx
            .user_manager
            .get_unique_nicknames_for_sessions(&result.member_session_ids)
            .await;

        // Membership is nickname-based: only broadcast when this nickname
        // first becomes present (multiple sessions may share a nickname).
        let nickname_present_elsewhere = ctx
            .user_manager
            .sessions_contain_nickname(&result.member_session_ids, &joining_user_nickname, Some(id))
            .await;

        if !nickname_present_elsewhere {
            let join_broadcast = ServerMessage::ChatUserJoined {
                channel: channel_name.clone(),
                nickname: joining_user_nickname.clone(),
                is_admin: joining_user_is_admin,
                is_shared: joining_user_is_shared,
            };
            for &member_session_id in &result.member_session_ids {
                if member_session_id != id {
                    ctx.user_manager
                        .send_to_session(member_session_id, join_broadcast.clone())
                        .await;
                }
            }
        }

        // Voiced nicknames are gated on voice_listen permission.
        let voiced = if has_voice_listen_permission {
            let participants = ctx.voice_registry.get_participants(&channel_name).await;
            if participants.is_empty() {
                None
            } else {
                Some(participants)
            }
        } else {
            None
        };

        joined_channels.push(ChannelJoinInfo {
            channel: channel_name,
            topic: result.topic,
            topic_set_by: result.topic_set_by,
            secret: result.secret,
            members: member_nicknames,
            voiced,
        });
    }

    // Resolved effective permissions sent as a flat string set. Admins get an
    // empty list; the client infers "all" from the is_admin flag.
    let user_permissions: Vec<String> = if authenticated_account.is_admin {
        vec![]
    } else {
        cached_permissions
            .iter()
            .map(|p| p.as_str().to_string())
            .collect()
    };

    let server_info_values = ServerInfoValues {
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

    let server_info_options = ServerInfoOptions {
        is_admin: authenticated_account.is_admin,
        has_file_reindex: cached_permissions.contains(&Permission::FileReindex),
        has_chat_join: can_auto_join,
        include_image: true,
    };

    let server_info = Some(build_server_info(&server_info_values, &server_info_options));

    let channels = if joined_channels.is_empty() {
        None
    } else {
        Some(joined_channels)
    };

    // Server-confirmed nickname: validated nickname for shared accounts, the
    // DB-canonical username (consistent casing) for regular accounts.
    let nickname = validated_nickname.unwrap_or_else(|| authenticated_account.username.clone());

    let response = ServerMessage::LoginResponse {
        success: true,
        session_id: Some(id),
        user_id: Some(authenticated_account.id),
        is_admin: Some(authenticated_account.is_admin),
        permissions: Some(user_permissions),
        server_info,
        locale: Some(locale.clone()),
        channels,
        nickname: Some(nickname.clone()),
        error: None,
        group_id,
        group_name: group_name.clone(),
    };
    ctx.send_message(&response).await?;

    debug!(user = %authenticated_account.username, ip = %ctx.peer_addr, "{}", LOG_LOGIN_SUCCESS);

    // Announce the new connection to other users.
    // Regular accounts: avatar = latest-login-with-an-avatar across all sessions
    // (incl. the one just registered), so a no-avatar login can't blank an
    // existing avatar. Shared accounts are per-session — use this session's own.
    let connected_avatar = if authenticated_account.is_shared {
        avatar
    } else {
        let sessions = ctx
            .user_manager
            .get_sessions_by_username(&authenticated_account.username)
            .await;
        UserManager::aggregate_avatar(sessions.iter())
    };
    let user_info = UserInfo {
        id: authenticated_account.id,
        username: authenticated_account.username.clone(),
        nickname,
        login_time: current_timestamp(),
        is_admin: authenticated_account.is_admin,
        is_shared: authenticated_account.is_shared,
        session_ids: vec![id],
        locale: locale.clone(),
        avatar: connected_avatar,
        is_away: false,
        status: None,
        group_id,
        group_name,
        bandwidth_weight: Some(bandwidth_weight),
    };
    ctx.user_manager
        .broadcast_user_event(
            ServerMessage::UserConnected { user: user_info },
            Some(id), // Don't send to the connecting user
        )
        .await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::testing::{
        DEFAULT_TEST_LOCALE, create_test_context, get_cached_password_hash, read_login_response,
        read_server_message,
    };

    #[tokio::test]
    async fn test_login_requires_handshake() {
        let mut test_ctx = create_test_context().await;
        let mut session_id = None;
        let handshake_complete = false;

        let request = LoginRequest {
            username: "alice".to_string(),
            password: "password".to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: None,
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(result.is_err(), "Login should fail without handshake");
        assert!(session_id.is_none(), "Session ID should remain None");
    }

    #[tokio::test]
    async fn test_first_login_creates_admin() {
        let mut test_ctx = create_test_context().await;
        let mut session_id = None;
        let handshake_complete = true;

        let request = LoginRequest {
            username: "alice".to_string(),
            password: "password123".to_string(),
            features: vec![FEATURE_CHAT.to_string()],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: None,
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "First login should succeed");
        assert!(session_id.is_some(), "Session ID should be set");

        let response_msg = read_login_response(&mut test_ctx).await;
        match response_msg {
            ServerMessage::LoginResponse {
                success,
                session_id,
                user_id,
                is_admin,
                permissions,
                error,
                ..
            } => {
                assert!(success, "Login should indicate success");
                assert!(session_id.is_some(), "Should return session ID");
                assert!(user_id.is_some(), "Should return user ID");
                assert_eq!(is_admin, Some(true), "First user should be marked as admin");
                assert_eq!(
                    permissions,
                    Some(vec![]),
                    "Admin should have empty permissions list"
                );
                assert!(error.is_none(), "Should have no error");
            }
            _ => panic!("Expected LoginResponse"),
        }

        let user = test_ctx
            .db
            .users
            .get_user_by_username("alice")
            .await
            .unwrap()
            .unwrap();
        assert!(user.is_admin, "First user should be admin");
    }

    #[tokio::test]
    async fn test_login_existing_user_correct_password() {
        let mut test_ctx = create_test_context().await;

        // Pre-create a user with permissions.
        let password = "mypassword";
        let hashed = get_cached_password_hash(password);
        let mut perms = db::Permissions::new();
        use std::collections::HashSet;
        perms.permissions = {
            let mut set = HashSet::new();
            set.insert(db::Permission::UserList);
            set.insert(db::Permission::ChatSend);
            set
        };
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: &hashed,
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

        let mut session_id = None;
        let handshake_complete = true;

        let request = LoginRequest {
            username: "bob".to_string(),
            password: password.to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: None,
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Login with correct password should succeed");
        assert!(session_id.is_some(), "Session ID should be set");

        let response_msg = read_login_response(&mut test_ctx).await;
        match response_msg {
            ServerMessage::LoginResponse {
                success,
                session_id,
                user_id,
                is_admin,
                permissions,
                error,
                ..
            } => {
                assert!(success, "Login should succeed");
                assert!(session_id.is_some(), "Should return session ID");
                assert!(user_id.is_some(), "Should return user ID");
                assert_eq!(
                    is_admin,
                    Some(false),
                    "Non-admin user should be marked as non-admin"
                );
                assert!(permissions.is_some(), "Should return permissions list");
                let perms = permissions.unwrap();
                assert!(
                    perms.contains(&"user_list".to_string()),
                    "Should have user_list permission"
                );
                assert!(
                    perms.contains(&"chat_send".to_string()),
                    "Should have chat_send permission"
                );
                assert!(error.is_none(), "Should have no error");
            }
            _ => panic!("Expected LoginResponse"),
        }
    }

    #[tokio::test]
    async fn test_login_wrong_password() {
        let mut test_ctx = create_test_context().await;

        let password = "correctpassword";
        let hashed = get_cached_password_hash(password);
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: &hashed,
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

        let mut session_id = None;
        let handshake_complete = true;

        let request = LoginRequest {
            username: "bob".to_string(),
            password: "wrongpassword".to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: None,
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(result.is_err(), "Login with wrong password should fail");
        assert!(session_id.is_none(), "Session ID should remain None");
    }

    #[tokio::test]
    async fn test_login_nonexistent_user() {
        let mut test_ctx = create_test_context().await;

        // Pre-create a user so we aren't the first user (who would auto-register as admin).
        let password = "password";
        let hashed = get_cached_password_hash(password);
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "existing",
                hashed_password: &hashed,
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

        let mut session_id = None;
        let handshake_complete = true;

        let request = LoginRequest {
            username: "nonexistent".to_string(),
            password: "password".to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: None,
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(
            result.is_err(),
            "Login as non-existent user should fail after first user"
        );
        assert!(session_id.is_none(), "Session ID should remain None");
    }

    #[tokio::test]
    async fn test_login_non_admin_returns_permissions() {
        let mut test_ctx = create_test_context().await;

        // Create an admin user first
        let admin_password = "adminpass";
        let admin_hashed = get_cached_password_hash(admin_password);
        let _admin = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "admin",
                hashed_password: &admin_hashed,
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

        // Create a non-admin user with specific permissions
        let password = "password";
        let hashed = get_cached_password_hash(password);
        let mut perms = db::Permissions::new();
        use std::collections::HashSet;
        perms.permissions = {
            let mut set = HashSet::new();
            set.insert(db::Permission::UserList);
            set.insert(db::Permission::ChatSend);
            set.insert(db::Permission::ChatReceive);
            set
        };
        let _user = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "alice",
                hashed_password: &hashed,
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

        let handshake_complete = true;
        let mut session_id = None;

        let request = LoginRequest {
            username: "alice".to_string(),
            password: password.to_string(),
            features: vec![FEATURE_CHAT.to_string()],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: None,
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Login should succeed");

        let response_msg = read_login_response(&mut test_ctx).await;
        match response_msg {
            ServerMessage::LoginResponse {
                success,
                session_id,
                user_id,
                is_admin,
                permissions,
                error,
                ..
            } => {
                assert!(success, "Login should succeed");
                assert!(session_id.is_some(), "Should return session ID");
                assert!(user_id.is_some(), "Should return user ID");
                assert_eq!(is_admin, Some(false), "Should not be admin");
                assert!(permissions.is_some(), "Should return permissions");

                let perms = permissions.unwrap();
                assert_eq!(perms.len(), 3, "Should have exactly 3 permissions");
                assert!(
                    perms.contains(&"user_list".to_string()),
                    "Should have user_list"
                );
                assert!(
                    perms.contains(&"chat_send".to_string()),
                    "Should have chat_send"
                );
                assert!(
                    perms.contains(&"chat_receive".to_string()),
                    "Should have chat_receive"
                );
                assert!(
                    !perms.contains(&"user_create".to_string()),
                    "Should NOT have user_create"
                );
                assert!(
                    !perms.contains(&"user_delete".to_string()),
                    "Should NOT have user_delete"
                );
                assert!(error.is_none(), "Should have no error");
            }
            _ => panic!("Expected LoginResponse"),
        }
    }

    #[tokio::test]
    async fn test_duplicate_login_same_connection() {
        let mut test_ctx = create_test_context().await;

        let password = "password";
        let hashed = get_cached_password_hash(password);
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "alice",
                hashed_password: &hashed,
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

        let mut session_id = None;
        let handshake_complete = true;

        let request1 = LoginRequest {
            username: "alice".to_string(),
            password: "password".to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: None,
            handshake_complete,
        };
        let result1 =
            handle_login(request1, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(result1.is_ok(), "First login should succeed");
        assert!(session_id.is_some(), "Session ID should be set");

        // Second login on the same connection must fail.
        let request2 = LoginRequest {
            username: "alice".to_string(),
            password: "password".to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: None,
            handshake_complete,
        };
        let result2 =
            handle_login(request2, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(
            result2.is_err(),
            "Second login on same connection should fail"
        );
    }

    #[tokio::test]
    async fn test_login_includes_auto_joined_channels_with_topic() {
        let mut test_ctx = create_test_context().await;

        // Create regular user with ChatJoin permission (required for auto-join)
        let password = "password";
        let hashed = get_cached_password_hash(password);
        let mut perms = db::Permissions::new();
        perms.add(db::Permission::ChatJoin);
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "alice",
                hashed_password: &hashed,
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

        test_ctx
            .channel_manager
            .initialize_persistent_channels(vec![crate::channels::Channel::with_settings(
                nexus_common::validators::DEFAULT_CHANNEL.to_string(),
                Some("Test server topic".to_string()),
                Some("admin".to_string()),
                false,
            )])
            .await;

        test_ctx
            .db
            .config
            .set_auto_join_channels(nexus_common::validators::DEFAULT_CHANNEL)
            .await
            .unwrap();

        let mut session_id = None;
        let handshake_complete = true;

        let request = LoginRequest {
            username: "alice".to_string(),
            password: password.to_string(),
            features: vec![FEATURE_CHAT.to_string()],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: None,
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Login should succeed");

        let response_msg = read_login_response(&mut test_ctx).await;
        match response_msg {
            ServerMessage::LoginResponse {
                success,
                server_info,
                channels,
                ..
            } => {
                assert!(success, "Login should succeed");
                assert!(server_info.is_some(), "Should include server_info");
                let info = server_info.unwrap();
                assert_eq!(
                    info.name,
                    Some("Nexus BBS".to_string()),
                    "Should include server name"
                );
                assert_eq!(
                    info.description,
                    Some("".to_string()),
                    "Should include server description"
                );
                assert!(
                    info.max_connections_per_ip.is_some(),
                    "All users should receive max_connections_per_ip"
                );
                assert!(channels.is_some(), "Should include channels");
                let channel_list = channels.unwrap();
                assert_eq!(channel_list.len(), 1, "Should have one auto-joined channel");
                let channel = &channel_list[0];
                assert_eq!(
                    channel.channel,
                    nexus_common::validators::DEFAULT_CHANNEL,
                    "Should be the default channel"
                );
                assert_eq!(
                    channel.topic,
                    Some("Test server topic".to_string()),
                    "Should include channel topic"
                );
                assert_eq!(
                    channel.topic_set_by,
                    Some("admin".to_string()),
                    "Should include topic setter"
                );
            }
            _ => panic!("Expected LoginResponse"),
        }
    }

    #[tokio::test]
    async fn test_login_no_channels_when_no_auto_join_configured() {
        let mut test_ctx = create_test_context().await;

        let password = "password";
        let hashed = get_cached_password_hash(password);
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "alice",
                hashed_password: &hashed,
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

        // Clear auto-join channels (default is #nexus).
        test_ctx.db.config.set_auto_join_channels("").await.unwrap();

        let mut session_id = None;
        let handshake_complete = true;

        let request = LoginRequest {
            username: "alice".to_string(),
            password: password.to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: None,
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Login should succeed");

        let response_msg = read_login_response(&mut test_ctx).await;
        match response_msg {
            ServerMessage::LoginResponse {
                success,
                server_info,
                channels,
                ..
            } => {
                assert!(success, "Login should succeed");
                assert!(server_info.is_some(), "Should include server_info");
                let info = server_info.unwrap();
                assert_eq!(
                    info.name,
                    Some("Nexus BBS".to_string()),
                    "Should include server name"
                );
                assert_eq!(
                    info.description,
                    Some("".to_string()),
                    "Should include server description"
                );
                assert!(
                    info.max_connections_per_ip.is_some(),
                    "All users should receive max_connections_per_ip"
                );
                assert!(
                    channels.is_none(),
                    "Should NOT include channels when none are auto-joined"
                );
            }
            _ => panic!("Expected LoginResponse"),
        }
    }

    #[tokio::test]
    async fn test_login_skips_auto_join_without_chat_join_permission() {
        let mut test_ctx = create_test_context().await;

        // Create user WITHOUT ChatJoin permission
        let password = "password";
        let hashed = get_cached_password_hash(password);
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "alice",
                hashed_password: &hashed,
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &db::Permissions::new(), // No permissions
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        test_ctx
            .channel_manager
            .initialize_persistent_channels(vec![crate::channels::Channel::new(
                nexus_common::validators::DEFAULT_CHANNEL.to_string(),
            )])
            .await;

        test_ctx
            .db
            .config
            .set_auto_join_channels(nexus_common::validators::DEFAULT_CHANNEL)
            .await
            .unwrap();

        let mut session_id = None;
        let handshake_complete = true;

        // Login WITH chat feature but WITHOUT ChatJoin permission.
        let request = LoginRequest {
            username: "alice".to_string(),
            password: password.to_string(),
            features: vec![FEATURE_CHAT.to_string()],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: None,
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Login should succeed");

        let response_msg = read_login_response(&mut test_ctx).await;
        match response_msg {
            ServerMessage::LoginResponse {
                success, channels, ..
            } => {
                assert!(success, "Login should succeed");
                assert!(
                    channels.as_ref().is_none_or(|c| c.is_empty()),
                    "Should NOT include channels when user lacks ChatJoin permission"
                );
            }
            _ => panic!("Expected LoginResponse"),
        }

        let channel_members = test_ctx
            .channel_manager
            .get_members(nexus_common::validators::DEFAULT_CHANNEL)
            .await
            .unwrap_or_default();
        assert!(
            channel_members.is_empty(),
            "User should not be in channel without ChatJoin permission"
        );
    }

    #[tokio::test]
    async fn test_login_skips_auto_join_channel_creation_without_chat_create_permission() {
        let mut test_ctx = create_test_context().await;

        // User has ChatJoin but NOT ChatCreate.
        let password = "password";
        let hashed = get_cached_password_hash(password);
        let mut perms = db::Permissions::new();
        perms.add(db::Permission::ChatJoin);
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "alice",
                hashed_password: &hashed,
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

        // No persistent channels initialized: auto-joining #nonexistent would require creating it.
        test_ctx
            .db
            .config
            .set_auto_join_channels("#nonexistent")
            .await
            .unwrap();

        let mut session_id = None;
        let handshake_complete = true;

        let request = LoginRequest {
            username: "alice".to_string(),
            password: password.to_string(),
            features: vec![FEATURE_CHAT.to_string()],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: None,
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Login should succeed");

        let response_msg = read_login_response(&mut test_ctx).await;
        match response_msg {
            ServerMessage::LoginResponse {
                success, channels, ..
            } => {
                assert!(success, "Login should succeed");
                assert!(
                    channels.as_ref().is_none_or(|c| c.is_empty()),
                    "Should NOT include channels when user lacks ChatCreate and channel doesn't exist"
                );
            }
            _ => panic!("Expected LoginResponse"),
        }

        assert!(
            test_ctx
                .channel_manager
                .get_channel("#nonexistent")
                .await
                .is_none(),
            "Channel should not be created without ChatCreate permission"
        );
    }

    #[tokio::test]
    async fn test_login_auto_joins_existing_channel_without_chat_create_permission() {
        let mut test_ctx = create_test_context().await;

        // User has ChatJoin but NOT ChatCreate; the channel exists before login so no create is needed.
        let password = "password";
        let hashed = get_cached_password_hash(password);
        let mut perms = db::Permissions::new();
        perms.add(db::Permission::ChatJoin);
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "alice",
                hashed_password: &hashed,
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

        test_ctx
            .channel_manager
            .initialize_persistent_channels(vec![crate::channels::Channel::new(
                nexus_common::validators::DEFAULT_CHANNEL.to_string(),
            )])
            .await;

        test_ctx
            .db
            .config
            .set_auto_join_channels(nexus_common::validators::DEFAULT_CHANNEL)
            .await
            .unwrap();

        let mut session_id = None;
        let handshake_complete = true;

        let request = LoginRequest {
            username: "alice".to_string(),
            password: password.to_string(),
            features: vec![FEATURE_CHAT.to_string()],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: None,
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Login should succeed");

        let response_msg = read_login_response(&mut test_ctx).await;
        match response_msg {
            ServerMessage::LoginResponse {
                success, channels, ..
            } => {
                assert!(success, "Login should succeed");
                let channels = channels.expect("Should include channels");
                assert_eq!(channels.len(), 1, "Should have joined one channel");
                assert_eq!(
                    channels[0].channel,
                    nexus_common::validators::DEFAULT_CHANNEL
                );
            }
            _ => panic!("Expected LoginResponse"),
        }

        let channel_members = test_ctx
            .channel_manager
            .get_members(nexus_common::validators::DEFAULT_CHANNEL)
            .await
            .expect("Channel should exist");
        assert!(
            !channel_members.is_empty(),
            "User should be in channel with ChatJoin permission for existing channel"
        );
    }

    #[tokio::test]
    async fn test_login_admin_receives_server_info_and_channels() {
        let mut test_ctx = create_test_context().await;

        let password = "password";
        let hashed = get_cached_password_hash(password);
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "admin",
                hashed_password: &hashed,
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

        test_ctx
            .channel_manager
            .initialize_persistent_channels(vec![crate::channels::Channel::with_settings(
                nexus_common::validators::DEFAULT_CHANNEL.to_string(),
                Some("Admin can see this".to_string()),
                Some("admin".to_string()),
                false,
            )])
            .await;

        test_ctx
            .db
            .config
            .set_auto_join_channels(nexus_common::validators::DEFAULT_CHANNEL)
            .await
            .unwrap();

        let mut session_id = None;
        let handshake_complete = true;

        let request = LoginRequest {
            username: "admin".to_string(),
            password: password.to_string(),
            features: vec![FEATURE_CHAT.to_string()],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: None,
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Login should succeed");

        let response_msg = read_login_response(&mut test_ctx).await;
        match response_msg {
            ServerMessage::LoginResponse {
                success,
                user_id,
                is_admin,
                server_info,
                channels,
                ..
            } => {
                assert!(success, "Login should succeed");
                assert!(user_id.is_some(), "Should return user ID");
                assert_eq!(is_admin, Some(true), "Should be admin");
                assert!(server_info.is_some(), "Admin should receive server_info");
                let info = server_info.unwrap();
                assert_eq!(
                    info.max_connections_per_ip,
                    Some(5),
                    "Admin should receive max_connections_per_ip"
                );
                assert!(channels.is_some(), "Admin should receive channels");
                let channel_list = channels.unwrap();
                assert_eq!(channel_list.len(), 1, "Should have one auto-joined channel");
                let channel = &channel_list[0];
                assert_eq!(
                    channel.topic,
                    Some("Admin can see this".to_string()),
                    "Channel should include topic"
                );
            }
            _ => panic!("Expected LoginResponse"),
        }
    }

    #[tokio::test]
    async fn test_login_disabled_account() {
        let mut test_ctx = create_test_context().await;

        // Pre-create a user so we aren't the first user (who would auto-register as admin).
        let password = "password";
        let hashed = get_cached_password_hash(password);
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "alice",
                hashed_password: &hashed,
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

        let bob_account = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: &hashed,
                is_admin: false,
                is_shared: false,
                enabled: false,
                permissions: &db::Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        assert!(!bob_account.enabled, "Bob should be disabled");

        let mut session_id = None;
        let handshake_complete = true;

        let request = LoginRequest {
            username: "bob".to_string(),
            password: password.to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: None,
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(result.is_err(), "Login with disabled account should fail");
        assert!(session_id.is_none(), "Session ID should remain None");

        let response_msg = read_server_message(&mut test_ctx).await;
        match response_msg {
            ServerMessage::Error { message, .. } => {
                assert!(
                    message.contains("Account")
                        && message.contains("bob")
                        && message.contains("disabled"),
                    "Should receive account disabled error with username, got: {}",
                    message
                );
            }
            _ => panic!("Expected Error message"),
        }
    }

    #[tokio::test]
    async fn test_login_error_uses_requested_locale() {
        let mut test_ctx = create_test_context().await;

        // Pre-create a user so we aren't the first user (who would auto-register as admin).
        let password = "password";
        let hashed = get_cached_password_hash(password);
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "alice",
                hashed_password: &hashed,
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

        let mut session_id = None;
        let handshake_complete = true;

        let request = LoginRequest {
            username: "alice".to_string(),
            password: "wrong_password".to_string(),
            features: vec![],
            locale: "es".to_string(),
            avatar: None,
            nickname: None,
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(result.is_err(), "Login with wrong password should fail");
        assert!(session_id.is_none(), "Session ID should remain None");

        let response_msg = read_server_message(&mut test_ctx).await;
        match response_msg {
            ServerMessage::Error { message, .. } => {
                // Error is in the requested locale (Spanish), not English.
                assert!(
                    message.contains("Usuario") || message.contains("contraseña"),
                    "Error message should be in Spanish, got: {}",
                    message
                );
            }
            _ => panic!("Expected Error message"),
        }
    }

    #[tokio::test]
    async fn test_login_error_defaults_to_english() {
        let mut test_ctx = create_test_context().await;

        // Pre-create a user so we aren't the first user (who would auto-register as admin).
        let password = "password";
        let hashed = get_cached_password_hash(password);
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "alice",
                hashed_password: &hashed,
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

        let mut session_id = None;
        let handshake_complete = true;

        // Empty locale must fall back to English.
        let request = LoginRequest {
            username: "alice".to_string(),
            password: "wrong_password".to_string(),
            features: vec![],
            locale: "".to_string(),
            avatar: None,
            nickname: None,
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(result.is_err(), "Login with wrong password should fail");
        assert!(session_id.is_none(), "Session ID should remain None");

        let response_msg = read_server_message(&mut test_ctx).await;
        match response_msg {
            ServerMessage::Error { message, .. } => {
                assert!(
                    message.contains("Invalid") || message.contains("username"),
                    "Error message should be in English (default), got: {}",
                    message
                );
            }
            _ => panic!("Expected Error message"),
        }
    }

    #[tokio::test]
    async fn test_login_with_valid_avatar() {
        let mut test_ctx = create_test_context().await;
        let mut session_id = None;
        let handshake_complete = true;

        let valid_avatar = "data:image/png;base64,iVBORw0KGgo=".to_string();

        let request = LoginRequest {
            username: "alice".to_string(),
            password: "password123".to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: Some(valid_avatar),
            nickname: None,
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Login with valid avatar should succeed");
        assert!(session_id.is_some(), "Session ID should be set");

        let response_msg = read_login_response(&mut test_ctx).await;
        match response_msg {
            ServerMessage::LoginResponse { success, .. } => {
                assert!(success, "Login should succeed with valid avatar");
            }
            _ => panic!("Expected LoginResponse"),
        }
    }

    #[tokio::test]
    async fn test_login_with_avatar_too_large() {
        let mut test_ctx = create_test_context().await;
        let mut session_id = None;
        let handshake_complete = true;

        // Build an avatar that exceeds MAX_AVATAR_DATA_URI_LENGTH.
        let prefix = "data:image/png;base64,";
        let padding = "A".repeat(validators::MAX_AVATAR_DATA_URI_LENGTH);
        let too_large_avatar = format!("{}{}", prefix, padding);

        let request = LoginRequest {
            username: "alice".to_string(),
            password: "password123".to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: Some(too_large_avatar),
            nickname: None,
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(result.is_err(), "Login with oversized avatar should fail");
        assert!(session_id.is_none(), "Session ID should remain None");

        let response_msg = read_server_message(&mut test_ctx).await;
        match response_msg {
            ServerMessage::Error { message, .. } => {
                assert!(
                    message.contains("too large") || message.contains("max"),
                    "Error should mention size limit, got: {}",
                    message
                );
            }
            _ => panic!("Expected Error message"),
        }
    }

    #[tokio::test]
    async fn test_login_with_avatar_invalid_format() {
        let mut test_ctx = create_test_context().await;
        let mut session_id = None;
        let handshake_complete = true;

        // Invalid format - missing base64 marker
        let invalid_avatar = "data:image/png,notbase64encoded".to_string();

        let request = LoginRequest {
            username: "alice".to_string(),
            password: "password123".to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: Some(invalid_avatar),
            nickname: None,
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(
            result.is_err(),
            "Login with invalid avatar format should fail"
        );
        assert!(session_id.is_none(), "Session ID should remain None");

        let response_msg = read_server_message(&mut test_ctx).await;
        match response_msg {
            ServerMessage::Error { message, .. } => {
                assert!(
                    message.contains("format") || message.contains("Invalid"),
                    "Error should mention invalid format, got: {}",
                    message
                );
            }
            _ => panic!("Expected Error message"),
        }
    }

    #[tokio::test]
    async fn test_login_with_avatar_unsupported_type() {
        let mut test_ctx = create_test_context().await;
        let mut session_id = None;
        let handshake_complete = true;

        // Unsupported type - GIF
        let unsupported_avatar =
            "data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7"
                .to_string();

        let request = LoginRequest {
            username: "alice".to_string(),
            password: "password123".to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: Some(unsupported_avatar),
            nickname: None,
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(
            result.is_err(),
            "Login with unsupported avatar type should fail"
        );
        assert!(session_id.is_none(), "Session ID should remain None");

        let response_msg = read_server_message(&mut test_ctx).await;
        match response_msg {
            ServerMessage::Error { message, .. } => {
                assert!(
                    message.contains("Unsupported")
                        || message.contains("PNG")
                        || message.contains("WebP")
                        || message.contains("SVG"),
                    "Error should mention unsupported type, got: {}",
                    message
                );
            }
            _ => panic!("Expected Error message"),
        }
    }

    #[tokio::test]
    async fn test_login_without_avatar_succeeds() {
        let mut test_ctx = create_test_context().await;
        let mut session_id = None;
        let handshake_complete = true;

        let request = LoginRequest {
            username: "alice".to_string(),
            password: "password123".to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: None,
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Login without avatar should succeed");
        assert!(session_id.is_some(), "Session ID should be set");

        let response_msg = read_login_response(&mut test_ctx).await;
        match response_msg {
            ServerMessage::LoginResponse { success, .. } => {
                assert!(success, "Login should succeed without avatar");
            }
            _ => panic!("Expected LoginResponse"),
        }
    }

    #[tokio::test]
    async fn test_login_shared_account_with_valid_nickname() {
        let mut test_ctx = create_test_context().await;
        let handshake_complete = true;

        let password = "password123";
        let hashed = get_cached_password_hash(password);

        // A regular admin must exist first so the next account isn't auto-registered as the admin.
        test_ctx
            .db
            .users
            .create_first_user_if_none_exist("admin", &hashed)
            .await
            .expect("admin creation should succeed");

        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "shared_acct",
                hashed_password: &hashed,
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

        let mut session_id = None;
        let request = LoginRequest {
            username: "shared_acct".to_string(),
            password: password.to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: Some("Alice".to_string()),
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Login with valid nickname should succeed");
        assert!(session_id.is_some(), "Session ID should be set");

        let response_msg = read_login_response(&mut test_ctx).await;
        match response_msg {
            ServerMessage::LoginResponse { success, .. } => {
                assert!(success, "Login response should indicate success");
            }
            _ => panic!("Expected LoginResponse"),
        }

        let session = test_ctx
            .user_manager
            .get_user_by_session_id(session_id.unwrap())
            .await
            .expect("session should exist");
        assert_eq!(session.nickname, "Alice");
        assert!(session.is_shared);
    }

    #[tokio::test]
    async fn test_login_shared_account_without_nickname_fails() {
        let mut test_ctx = create_test_context().await;
        let handshake_complete = true;

        let password = "password123";
        let hashed = get_cached_password_hash(password);

        test_ctx
            .db
            .users
            .create_first_user_if_none_exist("admin", &hashed)
            .await
            .expect("admin creation should succeed");

        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "shared_acct",
                hashed_password: &hashed,
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

        // A shared account requires a nickname; logging in with None must fail.
        let mut session_id = None;
        let request = LoginRequest {
            username: "shared_acct".to_string(),
            password: password.to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: None,
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(
            result.is_err(),
            "Login without nickname should fail for shared account"
        );
        assert!(session_id.is_none(), "Session ID should not be set");
    }

    #[tokio::test]
    async fn test_login_regular_account_with_nickname_ignored() {
        let mut test_ctx = create_test_context().await;
        let handshake_complete = true;

        // First login creates a regular (admin) account, so the supplied nickname is ignored.
        let mut session_id = None;
        let request = LoginRequest {
            username: "alice".to_string(),
            password: "password123".to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: Some("SomeNickname".to_string()),
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(
            result.is_ok(),
            "Login with nickname for regular account should succeed"
        );
        assert!(session_id.is_some(), "Session ID should be set");

        let session = test_ctx
            .user_manager
            .get_user_by_session_id(session_id.unwrap())
            .await
            .expect("session should exist");
        assert_eq!(
            session.nickname, "alice",
            "Nickname should equal username for regular account"
        );
        assert!(!session.is_shared);
    }

    #[tokio::test]
    async fn test_login_shared_account_nickname_collision_with_username() {
        let mut test_ctx = create_test_context().await;
        let handshake_complete = true;

        let password = "password123";
        let hashed = get_cached_password_hash(password);

        // Regular user "alice" is the collision target.
        test_ctx
            .db
            .users
            .create_first_user_if_none_exist("alice", &hashed)
            .await
            .expect("admin creation should succeed");

        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "shared_acct",
                hashed_password: &hashed,
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

        let mut session_id = None;
        let request = LoginRequest {
            username: "shared_acct".to_string(),
            password: password.to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: Some("alice".to_string()), // Collides with an existing username.
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(
            result.is_err(),
            "Login with nickname matching username should fail"
        );
        assert!(session_id.is_none(), "Session ID should not be set");
    }

    #[tokio::test]
    async fn test_login_shared_account_nickname_collision_with_username_case_insensitive() {
        let mut test_ctx = create_test_context().await;
        let handshake_complete = true;

        let password = "password123";
        let hashed = get_cached_password_hash(password);

        // Regular user "Alice"; the nickname "ALICE" collides case-insensitively.
        test_ctx
            .db
            .users
            .create_first_user_if_none_exist("Alice", &hashed)
            .await
            .expect("admin creation should succeed");

        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "shared_acct",
                hashed_password: &hashed,
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

        let mut session_id = None;
        let request = LoginRequest {
            username: "shared_acct".to_string(),
            password: password.to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: Some("ALICE".to_string()),
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(
            result.is_err(),
            "Login with nickname matching username (case-insensitive) should fail"
        );
        assert!(session_id.is_none(), "Session ID should not be set");
    }

    #[tokio::test]
    async fn test_login_shared_account_nickname_collision_with_username_unicode() {
        let mut test_ctx = create_test_context().await;
        let handshake_complete = true;

        let password = "password123";
        let hashed = get_cached_password_hash(password);

        // Regular user "Éclair"; a shared login with nickname "éclair" differs
        // only by Unicode case. The admission re-check rejects it via the folded
        // username lookup — under the old ASCII COLLATE NOCASE it would have been
        // admitted (É and é were distinct there).
        test_ctx
            .db
            .users
            .create_first_user_if_none_exist("Éclair", &hashed)
            .await
            .expect("admin creation should succeed");

        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "shared_acct",
                hashed_password: &hashed,
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

        let mut session_id = None;
        let request = LoginRequest {
            username: "shared_acct".to_string(),
            password: password.to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: Some("éclair".to_string()),
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(
            result.is_err(),
            "Login with nickname matching username (Unicode case) should fail"
        );
        assert!(session_id.is_none(), "Session ID should not be set");
    }

    #[tokio::test]
    async fn test_login_shared_account_nickname_collision_with_active_session() {
        let mut test_ctx = create_test_context().await;
        let handshake_complete = true;

        let password = "password123";
        let hashed = get_cached_password_hash(password);

        test_ctx
            .db
            .users
            .create_first_user_if_none_exist("admin", &hashed)
            .await
            .expect("admin creation should succeed");

        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "shared_acct",
                hashed_password: &hashed,
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

        // First session takes nickname "Bob".
        let mut session_id1 = None;
        let request1 = LoginRequest {
            username: "shared_acct".to_string(),
            password: password.to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: Some("Bob".to_string()),
            handshake_complete,
        };
        let result1 =
            handle_login(request1, &mut session_id1, &mut test_ctx.handler_context()).await;
        assert!(result1.is_ok(), "First login should succeed");
        assert!(session_id1.is_some());

        let _response1 = read_login_response(&mut test_ctx).await;

        // Second login with the same nickname as the active session must fail.
        let mut session_id2 = None;
        let request2 = LoginRequest {
            username: "shared_acct".to_string(),
            password: password.to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: Some("Bob".to_string()),
            handshake_complete,
        };
        let result2 =
            handle_login(request2, &mut session_id2, &mut test_ctx.handler_context()).await;

        assert!(
            result2.is_err(),
            "Login with duplicate nickname should fail"
        );
        assert!(
            session_id2.is_none(),
            "Session ID should not be set for duplicate nickname"
        );
    }

    #[tokio::test]
    async fn test_login_shared_account_two_users_different_nicknames() {
        let mut test_ctx = create_test_context().await;
        let handshake_complete = true;

        let password = "password123";
        let hashed = get_cached_password_hash(password);

        test_ctx
            .db
            .users
            .create_first_user_if_none_exist("admin", &hashed)
            .await
            .expect("admin creation should succeed");

        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "shared_acct",
                hashed_password: &hashed,
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

        // First session takes "Alice"; a second with a distinct nickname is allowed.
        let mut session_id1 = None;
        let request1 = LoginRequest {
            username: "shared_acct".to_string(),
            password: password.to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: Some("Alice".to_string()),
            handshake_complete,
        };
        let result1 =
            handle_login(request1, &mut session_id1, &mut test_ctx.handler_context()).await;
        assert!(result1.is_ok(), "First login should succeed");
        assert!(session_id1.is_some());

        let _response1 = read_login_response(&mut test_ctx).await;

        let mut session_id2 = None;
        let request2 = LoginRequest {
            username: "shared_acct".to_string(),
            password: password.to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: Some("Bob".to_string()),
            handshake_complete,
        };
        let result2 =
            handle_login(request2, &mut session_id2, &mut test_ctx.handler_context()).await;

        assert!(
            result2.is_ok(),
            "Second login with different nickname should succeed"
        );
        assert!(session_id2.is_some(), "Session ID should be set");

        let session1 = test_ctx
            .user_manager
            .get_user_by_session_id(session_id1.unwrap())
            .await
            .expect("session 1 should exist");
        assert_eq!(session1.nickname, "Alice");

        let session2 = test_ctx
            .user_manager
            .get_user_by_session_id(session_id2.unwrap())
            .await
            .expect("session 2 should exist");
        assert_eq!(session2.nickname, "Bob");
    }

    #[tokio::test]
    async fn test_login_shared_account_nickname_validation_empty() {
        let mut test_ctx = create_test_context().await;
        let handshake_complete = true;

        let password = "password123";
        let hashed = get_cached_password_hash(password);

        test_ctx
            .db
            .users
            .create_first_user_if_none_exist("admin", &hashed)
            .await
            .expect("admin creation should succeed");

        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "shared_acct",
                hashed_password: &hashed,
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

        let mut session_id = None;
        let request = LoginRequest {
            username: "shared_acct".to_string(),
            password: password.to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: Some("".to_string()),
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(result.is_err(), "Login with empty nickname should fail");
        assert!(session_id.is_none(), "Session ID should not be set");
    }

    #[tokio::test]
    async fn test_login_shared_account_nickname_validation_too_long() {
        let mut test_ctx = create_test_context().await;
        let handshake_complete = true;

        let password = "password123";
        let hashed = get_cached_password_hash(password);

        test_ctx
            .db
            .users
            .create_first_user_if_none_exist("admin", &hashed)
            .await
            .expect("admin creation should succeed");

        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "shared_acct",
                hashed_password: &hashed,
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

        let mut session_id = None;
        let request = LoginRequest {
            username: "shared_acct".to_string(),
            password: password.to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: Some("a".repeat(validators::MAX_NICKNAME_LENGTH + 1)),
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(result.is_err(), "Login with too long nickname should fail");
        assert!(session_id.is_none(), "Session ID should not be set");
    }

    #[tokio::test]
    async fn test_login_shared_account_nickname_validation_invalid_characters() {
        let mut test_ctx = create_test_context().await;
        let handshake_complete = true;

        let password = "password123";
        let hashed = get_cached_password_hash(password);

        test_ctx
            .db
            .users
            .create_first_user_if_none_exist("admin", &hashed)
            .await
            .expect("admin creation should succeed");

        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "shared_acct",
                hashed_password: &hashed,
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

        let mut session_id = None;
        let request = LoginRequest {
            username: "shared_acct".to_string(),
            password: password.to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: Some("Alice Smith".to_string()), // Spaces are not allowed in nicknames.
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(
            result.is_err(),
            "Login with invalid nickname characters should fail"
        );
        assert!(session_id.is_none(), "Session ID should not be set");
    }

    #[tokio::test]
    async fn test_login_shared_account_nickname_collision_with_logged_in_username() {
        let mut test_ctx = create_test_context().await;
        let handshake_complete = true;

        let password = "password123";
        let hashed = get_cached_password_hash(password);

        // Regular user "alice" is created and logged in below.
        test_ctx
            .db
            .users
            .create_first_user_if_none_exist("alice", &hashed)
            .await
            .expect("admin creation should succeed");

        let mut alice_session_id = None;
        let alice_request = LoginRequest {
            username: "alice".to_string(),
            password: password.to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: None,
            handshake_complete,
        };
        let alice_result = handle_login(
            alice_request,
            &mut alice_session_id,
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(alice_result.is_ok(), "Alice login should succeed");

        let _response = read_login_response(&mut test_ctx).await;

        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "shared_acct",
                hashed_password: &hashed,
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

        // Nickname "alice" must be rejected: it conflicts with the username of a logged-in user.
        let mut session_id = None;
        let request = LoginRequest {
            username: "shared_acct".to_string(),
            password: password.to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: Some("alice".to_string()),
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(
            result.is_err(),
            "Login with nickname matching logged-in username should fail"
        );
        assert!(session_id.is_none(), "Session ID should not be set");
    }

    #[tokio::test]
    async fn test_guest_login_with_empty_username() {
        let mut test_ctx = create_test_context().await;
        let handshake_complete = true;

        // Enable guest account
        test_ctx
            .db
            .users
            .update_user(db::UpdateUserParams {
                username: "guest",
                new_username: None,
                new_password_hash: None,
                is_admin: None,
                enabled: Some(true),
                permissions: None,
                revokes: None,
                remove_group: false,
                bandwidth_weight: None,
                inherit_bandwidth_weight: false,
                group_id: None,
                requester_is_admin: true,
                permission_write_scope: db::PermissionWriteScope::ReplaceAll,
                requester_bandwidth_max: None,
            })
            .await
            .unwrap();

        // Empty username + empty password is the guest-login signal.
        let mut session_id = None;
        let request = LoginRequest {
            username: String::new(),
            password: String::new(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: Some("GuestUser".to_string()),
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Guest login should succeed");
        assert!(session_id.is_some(), "Session ID should be set");

        let response = read_login_response(&mut test_ctx).await;
        match response {
            ServerMessage::LoginResponse {
                success,
                user_id,
                is_admin,
                ..
            } => {
                assert!(success, "Login should succeed");
                assert!(user_id.is_some(), "Should return user ID");
                assert_eq!(is_admin, Some(false), "Guest should not be admin");
            }
            _ => panic!("Expected LoginResponse"),
        }
    }

    #[tokio::test]
    async fn test_guest_login_with_guest_username() {
        let mut test_ctx = create_test_context().await;
        let handshake_complete = true;

        // Enable guest account
        test_ctx
            .db
            .users
            .update_user(db::UpdateUserParams {
                username: "guest",
                new_username: None,
                new_password_hash: None,
                is_admin: None,
                enabled: Some(true),
                permissions: None,
                revokes: None,
                remove_group: false,
                bandwidth_weight: None,
                inherit_bandwidth_weight: false,
                group_id: None,
                requester_is_admin: true,
                permission_write_scope: db::PermissionWriteScope::ReplaceAll,
                requester_bandwidth_max: None,
            })
            .await
            .unwrap();

        let mut session_id = None;
        let request = LoginRequest {
            username: "guest".to_string(),
            password: String::new(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: Some("AnotherGuest".to_string()),
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Guest login should succeed");
        assert!(session_id.is_some(), "Session ID should be set");
    }

    #[tokio::test]
    async fn test_guest_login_with_nonempty_password_fails() {
        let mut test_ctx = create_test_context().await;
        let handshake_complete = true;

        // Enable guest account
        test_ctx
            .db
            .users
            .update_user(db::UpdateUserParams {
                username: "guest",
                new_username: None,
                new_password_hash: None,
                is_admin: None,
                enabled: Some(true),
                permissions: None,
                revokes: None,
                remove_group: false,
                bandwidth_weight: None,
                inherit_bandwidth_weight: false,
                group_id: None,
                requester_is_admin: true,
                permission_write_scope: db::PermissionWriteScope::ReplaceAll,
                requester_bandwidth_max: None,
            })
            .await
            .unwrap();

        // A guest login with a non-empty password must be rejected.
        let mut session_id = None;
        let request = LoginRequest {
            username: "guest".to_string(),
            password: "somepassword".to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: Some("BadGuest".to_string()),
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(result.is_err(), "Guest login with password should fail");
        assert!(session_id.is_none(), "Session ID should not be set");
    }

    #[tokio::test]
    async fn test_guest_login_disabled_fails() {
        let mut test_ctx = create_test_context().await;
        let handshake_complete = true;

        // The guest account is disabled by default from the migration — no enable step here.
        let mut session_id = None;
        let request = LoginRequest {
            username: String::new(),
            password: String::new(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: Some("DisabledGuest".to_string()),
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(result.is_err(), "Guest login should fail when disabled");
        assert!(session_id.is_none(), "Session ID should not be set");
    }

    #[tokio::test]
    async fn test_guest_login_requires_nickname() {
        let mut test_ctx = create_test_context().await;
        let handshake_complete = true;

        // Enable guest account
        test_ctx
            .db
            .users
            .update_user(db::UpdateUserParams {
                username: "guest",
                new_username: None,
                new_password_hash: None,
                is_admin: None,
                enabled: Some(true),
                permissions: None,
                revokes: None,
                remove_group: false,
                bandwidth_weight: None,
                inherit_bandwidth_weight: false,
                group_id: None,
                requester_is_admin: true,
                permission_write_scope: db::PermissionWriteScope::ReplaceAll,
                requester_bandwidth_max: None,
            })
            .await
            .unwrap();

        let mut session_id = None;
        let request = LoginRequest {
            username: String::new(),
            password: String::new(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: None,
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(result.is_err(), "Guest login without nickname should fail");
        assert!(session_id.is_none(), "Session ID should not be set");
    }

    #[tokio::test]
    async fn test_guest_login_case_insensitive_username() {
        let mut test_ctx = create_test_context().await;
        let handshake_complete = true;

        // Enable guest account
        test_ctx
            .db
            .users
            .update_user(db::UpdateUserParams {
                username: "guest",
                new_username: None,
                new_password_hash: None,
                is_admin: None,
                enabled: Some(true),
                permissions: None,
                revokes: None,
                remove_group: false,
                bandwidth_weight: None,
                inherit_bandwidth_weight: false,
                group_id: None,
                requester_is_admin: true,
                permission_write_scope: db::PermissionWriteScope::ReplaceAll,
                requester_bandwidth_max: None,
            })
            .await
            .unwrap();

        // Uppercase "GUEST" must match the guest account case-insensitively.
        let mut session_id = None;
        let request = LoginRequest {
            username: "GUEST".to_string(),
            password: String::new(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: Some("CaseTest".to_string()),
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Guest login with uppercase should succeed");
        assert!(session_id.is_some(), "Session ID should be set");
    }

    #[tokio::test]
    async fn test_guest_login_returns_is_shared_true() {
        let mut test_ctx = create_test_context().await;
        let handshake_complete = true;

        // Enable guest account
        test_ctx
            .db
            .users
            .update_user(db::UpdateUserParams {
                username: "guest",
                new_username: None,
                new_password_hash: None,
                is_admin: None,
                enabled: Some(true),
                permissions: None,
                revokes: None,
                remove_group: false,
                bandwidth_weight: None,
                inherit_bandwidth_weight: false,
                group_id: None,
                requester_is_admin: true,
                permission_write_scope: db::PermissionWriteScope::ReplaceAll,
                requester_bandwidth_max: None,
            })
            .await
            .unwrap();

        let mut session_id = None;
        let request = LoginRequest {
            username: String::new(),
            password: String::new(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: Some("SharedGuest".to_string()),
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Guest login should succeed");

        let user = test_ctx
            .user_manager
            .get_user_by_session_id(session_id.unwrap())
            .await;
        assert!(user.is_some(), "User should exist in manager");
        assert!(user.unwrap().is_shared, "Guest should be marked as shared");
    }

    #[tokio::test]
    async fn test_first_admin_created_with_guest_account_existing() {
        let mut test_ctx = create_test_context().await;
        let handshake_complete = true;

        // The disabled migration guest account doesn't count: the first non-guest user becomes admin.
        let mut session_id = None;
        let request = LoginRequest {
            username: "firstadmin".to_string(),
            password: "password123".to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: None,
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "First user login should succeed");
        assert!(session_id.is_some(), "Session ID should be set");

        let response = read_login_response(&mut test_ctx).await;
        match response {
            ServerMessage::LoginResponse {
                success,
                user_id,
                is_admin,
                ..
            } => {
                assert!(success, "Login should succeed");
                assert!(user_id.is_some(), "Should return user ID");
                assert_eq!(is_admin, Some(true), "First non-guest user should be admin");
            }
            _ => panic!("Expected LoginResponse"),
        }
    }

    #[tokio::test]
    async fn test_login_inherits_away_status_from_existing_session() {
        use crate::users::user::NewSessionParams;
        use std::time::Instant;

        let mut test_ctx = create_test_context().await;
        let handshake_complete = true;

        let password = "password";
        let hashed = get_cached_password_hash(password);
        let account = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "alice",
                hashed_password: &hashed,
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

        // Existing session is away with a status set; a new session should inherit both.
        let _session1 = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                user_id: account.id,
                username: "alice".to_string(),
                is_admin: false,
                is_shared: false,
                permissions: std::collections::HashSet::new(),
                address: test_ctx.peer_addr,
                created_at: account.created_at,
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: DEFAULT_TEST_LOCALE.to_string(),
                avatar: None,
                nickname: "alice".to_string(),
                is_away: true,
                status: Some("grabbing lunch".to_string()),
                group_id: None,
                group_name: None,
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
                bandwidth_weight_override: None,
                last_activity: Instant::now(),
            })
            .await
            .expect("Failed to add first session");

        let mut session_id = None;
        let request = LoginRequest {
            username: "alice".to_string(),
            password: password.to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: None,
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Second login should succeed");
        let new_session_id = session_id.expect("Session ID should be set");

        let _response = read_login_response(&mut test_ctx).await;

        let new_session = test_ctx
            .user_manager
            .get_user_by_session_id(new_session_id)
            .await
            .expect("New session should exist");

        assert!(
            new_session.is_away,
            "New session should inherit is_away=true from existing session"
        );
        assert_eq!(
            new_session.status,
            Some("grabbing lunch".to_string()),
            "New session should inherit status from existing session"
        );
    }

    #[tokio::test]
    async fn test_login_no_inheritance_for_shared_accounts() {
        use crate::users::user::NewSessionParams;
        use std::time::Instant;

        let mut test_ctx = create_test_context().await;
        let handshake_complete = true;

        let password = "password";
        let hashed = get_cached_password_hash(password);
        let account = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "shared_acct",
                hashed_password: &hashed,
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

        // First shared session is away; a second session under a different nickname must NOT inherit it.
        let _session1 = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                user_id: account.id,
                username: "shared_acct".to_string(),
                is_admin: false,
                is_shared: true,
                permissions: std::collections::HashSet::new(),
                address: test_ctx.peer_addr,
                created_at: account.created_at,
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: DEFAULT_TEST_LOCALE.to_string(),
                avatar: None,
                nickname: "user_one".to_string(),
                is_away: true,
                status: Some("away message".to_string()),
                group_id: None,
                group_name: None,
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
                bandwidth_weight_override: None,
                last_activity: Instant::now(),
            })
            .await
            .expect("Failed to add first session");

        let mut session_id = None;
        let request = LoginRequest {
            username: "shared_acct".to_string(),
            password: password.to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: Some("user_two".to_string()),
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Second shared login should succeed");
        let new_session_id = session_id.expect("Session ID should be set");

        let _response = read_login_response(&mut test_ctx).await;

        let new_session = test_ctx
            .user_manager
            .get_user_by_session_id(new_session_id)
            .await
            .expect("New session should exist");

        assert!(
            !new_session.is_away,
            "Shared account session should NOT inherit is_away"
        );
        assert_eq!(
            new_session.status, None,
            "Shared account session should NOT inherit status"
        );
    }

    #[tokio::test]
    async fn test_login_inherits_from_latest_session() {
        use crate::users::user::NewSessionParams;
        use std::time::Instant;

        let mut test_ctx = create_test_context().await;
        let handshake_complete = true;

        let password = "password";
        let hashed = get_cached_password_hash(password);
        let account = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "alice",
                hashed_password: &hashed,
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

        // Older session with an away status.
        let _session1 = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                user_id: account.id,
                username: "alice".to_string(),
                is_admin: false,
                is_shared: false,
                permissions: std::collections::HashSet::new(),
                address: test_ctx.peer_addr,
                created_at: account.created_at,
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: DEFAULT_TEST_LOCALE.to_string(),
                avatar: None,
                nickname: "alice".to_string(),
                is_away: true,
                status: Some("old status".to_string()),
                group_id: None,
                group_name: None,
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
                bandwidth_weight_override: None,
                last_activity: Instant::now(),
            })
            .await
            .expect("Failed to add first session");

        // Sleep so the two sessions get distinct login timestamps.
        tokio::time::sleep(tokio::time::Duration::from_millis(1100)).await;

        // Newer session with a different away status — this is the one to inherit from.
        let _session2 = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                user_id: account.id,
                username: "alice".to_string(),
                is_admin: false,
                is_shared: false,
                permissions: std::collections::HashSet::new(),
                address: test_ctx.peer_addr,
                created_at: account.created_at,
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: DEFAULT_TEST_LOCALE.to_string(),
                avatar: None,
                nickname: "alice".to_string(),
                is_away: false,
                status: Some("new status".to_string()),
                group_id: None,
                group_name: None,
                bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
                bandwidth_weight_override: None,
                last_activity: Instant::now(),
            })
            .await
            .expect("Failed to add second session");

        let mut session_id = None;
        let request = LoginRequest {
            username: "alice".to_string(),
            password: password.to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: None,
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Third login should succeed");
        let new_session_id = session_id.expect("Session ID should be set");

        let _response = read_login_response(&mut test_ctx).await;

        let new_session = test_ctx
            .user_manager
            .get_user_by_session_id(new_session_id)
            .await
            .expect("New session should exist");

        assert!(
            !new_session.is_away,
            "Should inherit is_away=false from latest session"
        );
        assert_eq!(
            new_session.status,
            Some("new status".to_string()),
            "Should inherit status from latest session"
        );
    }

    #[tokio::test]
    async fn test_login_broadcasts_chat_user_joined_to_existing_channel_members() {
        let mut test_ctx = create_test_context().await;

        // Create two users with ChatJoin permission (required for auto-join)
        let password = "password";
        let hashed = get_cached_password_hash(password);
        let mut perms = db::Permissions::new();
        perms.add(db::Permission::ChatJoin);
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "alice",
                hashed_password: &hashed,
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
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: &hashed,
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

        test_ctx
            .channel_manager
            .initialize_persistent_channels(vec![crate::channels::Channel::new(
                nexus_common::validators::DEFAULT_CHANNEL.to_string(),
            )])
            .await;

        test_ctx
            .db
            .config
            .set_auto_join_channels(nexus_common::validators::DEFAULT_CHANNEL)
            .await
            .unwrap();

        // Alice logs in first and auto-joins the channel.
        let mut alice_session_id = None;
        let alice_request = LoginRequest {
            username: "alice".to_string(),
            password: password.to_string(),
            features: vec![FEATURE_CHAT.to_string()],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: None,
            handshake_complete: true,
        };
        let result = handle_login(
            alice_request,
            &mut alice_session_id,
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok(), "Alice login should succeed");

        let _alice_response = read_login_response(&mut test_ctx).await;

        let alice_sid = alice_session_id.expect("Alice should have session ID");
        assert!(
            test_ctx
                .channel_manager
                .is_member(nexus_common::validators::DEFAULT_CHANNEL, alice_sid)
                .await,
            "Alice should be in the default channel"
        );

        // Bob logs in and auto-joins, which should broadcast ChatUserJoined to Alice.
        let mut bob_session_id = None;
        let bob_request = LoginRequest {
            username: "bob".to_string(),
            password: password.to_string(),
            features: vec![FEATURE_CHAT.to_string()],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: None,
            handshake_complete: true,
        };
        let result = handle_login(
            bob_request,
            &mut bob_session_id,
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok(), "Bob login should succeed");

        let _bob_response = read_login_response(&mut test_ctx).await;

        let bob_sid = bob_session_id.expect("Bob should have session ID");
        assert!(
            test_ctx
                .channel_manager
                .is_member(nexus_common::validators::DEFAULT_CHANNEL, bob_sid)
                .await,
            "Bob should be in the default channel"
        );

        // All test sessions share one tx/rx channel, so scan rx for Bob's ChatUserJoined.
        let mut found_chat_user_joined = false;
        while let Ok((msg, _)) = test_ctx.rx.try_recv() {
            if matches!(
                &msg,
                ServerMessage::ChatUserJoined { channel, nickname, .. }
                    if channel == nexus_common::validators::DEFAULT_CHANNEL && nickname == "bob"
            ) {
                found_chat_user_joined = true;
                break;
            }
        }

        assert!(
            found_chat_user_joined,
            "Alice should have received ChatUserJoined for Bob when Bob auto-joined the channel"
        );
    }

    #[tokio::test]
    async fn test_login_group_permissions_resolved_end_to_end() {
        let mut test_ctx = create_test_context().await;

        let admin_password = "adminpass";
        let admin_hashed = get_cached_password_hash(admin_password);
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "admin",
                hashed_password: &admin_hashed,
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

        // Group "Staff" grants chat_send + user_list.
        let group = test_ctx
            .db
            .groups
            .create_group(
                "Staff",
                false,
                &db::Permissions::from(&[db::Permission::ChatSend, db::Permission::UserList]),
                1,
            )
            .await
            .unwrap();

        // Bob is assigned to the group with NO direct grants, so his perms come only from the group.
        let password = "password";
        let hashed = get_cached_password_hash(password);
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "bob",
                hashed_password: &hashed,
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

        let mut session_id = None;
        let request = LoginRequest {
            username: "bob".to_string(),
            password: password.to_string(),
            features: vec![FEATURE_CHAT.to_string()],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: None,
            handshake_complete: true,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;
        assert!(result.is_ok(), "Login should succeed");

        let response = read_login_response(&mut test_ctx).await;
        match response {
            ServerMessage::LoginResponse {
                success,
                user_id,
                permissions,
                group_id,
                group_name,
                error,
                ..
            } => {
                assert!(success);
                assert!(user_id.is_some(), "Should return user ID");
                assert!(error.is_none());

                // Group fields populated
                assert_eq!(group_id, Some(group.id));
                assert_eq!(group_name, Some("Staff".to_string()));

                // Permissions resolved from group (bob has no direct grants)
                let perms = permissions.expect("Should return permissions");
                assert!(
                    perms.contains(&"chat_send".to_string()),
                    "Should have chat_send from group"
                );
                assert!(
                    perms.contains(&"user_list".to_string()),
                    "Should have user_list from group"
                );
                assert_eq!(perms.len(), 2, "Should have exactly the group permissions");
            }
            _ => panic!("Expected LoginResponse"),
        }

        // Verify session cache has the permissions (the actual runtime check path)
        let bob_session_id = session_id.unwrap();
        let bob_session = test_ctx
            .user_manager
            .get_user_by_session_id(bob_session_id)
            .await
            .expect("Bob should be in UserManager");
        assert!(
            bob_session.has_permission(db::Permission::ChatSend),
            "Session should have chat_send from group"
        );
        assert!(
            bob_session.has_permission(db::Permission::UserList),
            "Session should have user_list from group"
        );
        assert!(
            !bob_session.has_permission(db::Permission::UserKick),
            "Session should NOT have user_kick"
        );

        // Verify group info cached on session
        assert_eq!(bob_session.group_id, Some(group.id));
        assert_eq!(bob_session.group_name, Some("Staff".to_string()));
    }
}
