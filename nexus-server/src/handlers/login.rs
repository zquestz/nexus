//! Login message handler

use std::io;
use std::net::SocketAddr;
use std::sync::atomic::Ordering;

use tokio::io::AsyncWrite;
use tracing::{debug, error, info, warn};

use nexus_common::names::fold_name;
use nexus_common::protocol::{ChannelJoinInfo, ServerMessage};
use nexus_common::rate_limiter::{RateCheck, RateLimiter};
use nexus_common::validators::{
    self, AvatarError, FeaturesError, LocaleError, NicknameError, PasswordError, UsernameError,
};

use super::duration::format_duration_remaining;
use super::{
    HandlerContext, ServerInfoOptions, ServerInfoValues, build_channel_join_info,
    build_server_info, err_account_disabled, err_already_logged_in, err_authentication,
    err_avatar_invalid_format, err_avatar_too_large, err_avatar_undecodable,
    err_avatar_unsupported_type, err_banned_permanent, err_banned_with_expiry, err_database,
    err_failed_to_create_user, err_features_empty_feature, err_features_feature_too_long,
    err_features_invalid_characters, err_features_too_many, err_guest_disabled,
    err_handshake_required, err_internal_error, err_invalid_credentials,
    err_locale_invalid_characters, err_locale_too_long, err_login_bandwidth_failed,
    err_login_group_failed, err_login_permissions_failed, err_login_rate_limited,
    err_nickname_empty, err_nickname_invalid, err_nickname_required, err_nickname_too_long,
    err_nickname_unavailable, err_password_too_long, err_username_empty, err_username_invalid,
    err_username_too_long,
};
use crate::channels::{ChannelManager, JoinPolicy};
use crate::constants::{
    FEATURE_CHAT, FEATURE_VOICE, HANDLER_LOGIN, LOG_BANDWIDTH_WEIGHT_RESOLVE_FAILED,
    LOG_EGRESS_TRANSITION_FAILED, LOG_LOGIN_ACCOUNT_DISABLED, LOG_LOGIN_ALREADY_LOGGED_IN,
    LOG_LOGIN_AVATAR_VALIDATE_ERROR, LOG_LOGIN_CREATE_USER_ERROR, LOG_LOGIN_DB_ERROR,
    LOG_LOGIN_DB_NICKNAME, LOG_LOGIN_FIRST_ADMIN, LOG_LOGIN_GROUP_ERROR,
    LOG_LOGIN_HANDSHAKE_REQUIRED, LOG_LOGIN_HASH_ERROR, LOG_LOGIN_INVALID_CREDENTIALS,
    LOG_LOGIN_PASSWORD_CHANGED, LOG_LOGIN_PASSWORD_VERIFY_ERROR, LOG_LOGIN_PERMISSIONS_ERROR,
    LOG_LOGIN_RATE_LIMITED, LOG_LOGIN_RENAMED_MID_LOGIN, LOG_LOGIN_SUCCESS, SUPPORTED_FEATURES,
};
use crate::db::sql::GUEST_USERNAME;
use crate::db::{self, Database, LoginSnapshotError, Permission, UserAccount};
use crate::egress::task::EgressHandle;
use crate::ip_rule_cache::IpAdmission;
use crate::scheduler::ConnectionId;
use crate::users::manager::{AddUserError, UserManager};
use crate::users::user::{NewSessionParams, UserSession};
use crate::voice::VoiceRegistry;

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

struct LoginSuccess {
    response: Box<ServerMessage>,
}

fn activate_supported_features(features: Vec<String>) -> Vec<String> {
    let mut activated: Vec<String> = features
        .into_iter()
        .filter(|feature| SUPPORTED_FEATURES.contains(&feature.as_str()))
        .collect();
    activated.sort();
    activated.dedup();
    activated
}

fn transition_login_to_user_flow(
    egress: &EgressHandle,
    egress_connection_id: ConnectionId,
    user_id: i64,
    weight: u16,
    peer_addr: SocketAddr,
) {
    match egress.transition_to_user(egress_connection_id, user_id, weight) {
        Ok(()) => {}
        Err(e) => {
            warn!(
                ip = %peer_addr,
                egress_connection_id = egress_connection_id.get(),
                user_id,
                weight,
                err = ?e,
                "{}",
                LOG_EGRESS_TRANSITION_FAILED
            );
        }
    }
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

fn late_ban_error(locale: &str, expires_at: Option<i64>) -> String {
    match expires_at {
        Some(expiry) => err_banned_with_expiry(locale, &format_duration_remaining(locale, expiry)),
        None => err_banned_permanent(locale),
    }
}

async fn validate_login_avatar(
    avatar: &Option<String>,
    locale: &str,
    peer_addr: SocketAddr,
) -> Result<(), String> {
    let Some(avatar_data) = avatar else {
        return Ok(());
    };

    // Decode-validation (raster decode / SVG parse under `image-decode`) is
    // CPU-bound; run it on the blocking pool so it can't stall the async
    // runtime's message dispatch. Callers run this only after authentication
    // succeeds, except the fresh-install first-admin path where validation
    // must happen before creating the account.
    let owned = avatar_data.clone();
    match tokio::task::spawn_blocking(move || validators::validate_avatar(&owned)).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => {
            let error_msg = match e {
                AvatarError::TooLarge => {
                    err_avatar_too_large(locale, validators::MAX_AVATAR_DATA_URI_LENGTH)
                }
                AvatarError::InvalidFormat => err_avatar_invalid_format(locale),
                AvatarError::UnsupportedType => err_avatar_unsupported_type(locale),
                AvatarError::Undecodable => err_avatar_undecodable(locale),
            };
            Err(error_msg)
        }
        Err(e) => {
            error!(ip = %peer_addr, err = %e, "{}", LOG_LOGIN_AVATAR_VALIDATE_ERROR);
            Err(err_internal_error(locale))
        }
    }
}

/// Read-only environment for [`authenticate_or_bootstrap`], bundled so the
/// helper stays free of the handler's `W` generic (matching the ctx-free
/// helper style in this file).
struct AuthEnv<'a> {
    db: &'a Database,
    login_limiter: &'a RateLimiter,
    peer_addr: SocketAddr,
    login_ip_trusted: bool,
}

/// Phase 2 of login: resolve the account — verify credentials for an
/// existing account, or bootstrap the first admin on a fresh server.
/// Debits the failed-login limiter on every unauthenticated rejection via
/// `debit` (trusted IPs exempt; successes never debit); never touches the
/// socket — every `Err` is the translated rejection message.
///
/// Returns `(account, avatar_validated)`: the bootstrap path validates
/// the avatar before hashing, so the caller must skip re-validation.
async fn authenticate_or_bootstrap(
    env: &AuthEnv<'_>,
    username: &str,
    password: &str,
    avatar: &Option<String>,
    locale: &str,
) -> Result<(UserAccount, bool), String> {
    let debit = || {
        if !env.login_ip_trusted {
            env.login_limiter.record_failure(env.peer_addr.ip());
        }
    };

    let account = match env.db.users.get_user_by_username(username).await {
        Ok(acc) => acc,
        Err(e) => {
            error!(ip = %env.peer_addr, target = %username, err = %e, "{}", LOG_LOGIN_DB_ERROR);
            return Err(err_database(locale));
        }
    };

    if let Some(account) = account {
        // Guest accounts have an empty hash, which requires an empty password.
        let password_valid = if account.hashed_password.is_empty() {
            password.is_empty()
        } else {
            match db::verify_password_async(password.to_owned(), account.hashed_password.clone())
                .await
            {
                Ok(valid) => valid,
                Err(e) => {
                    error!(ip = %env.peer_addr, target = %username, err = %e, "{}", LOG_LOGIN_PASSWORD_VERIFY_ERROR);
                    return Err(err_authentication(locale));
                }
            }
        };

        if !password_valid {
            warn!(ip = %env.peer_addr, target = %username, "{}", LOG_LOGIN_INVALID_CREDENTIALS);
            debit();
            return Err(err_invalid_credentials(locale));
        }

        if !account.enabled {
            warn!(ip = %env.peer_addr, target = %username, "{}", LOG_LOGIN_ACCOUNT_DISABLED);
            debit();
            return Err(if fold_name(username) == GUEST_USERNAME {
                err_guest_disabled(locale)
            } else {
                err_account_disabled(locale, username)
            });
        }

        return Ok((account, false));
    }

    // Unknown username. This COUNT is one query the wrong-password path
    // (account found → verify) doesn't run, so it's a deliberate timing
    // asymmetry — but it's tens of µs against the dummy verify's ~50–500 ms
    // Argon2 (and network jitter), far below any measurable signal, so it's
    // not a usable enumeration oracle. Not hoisted onto every login to
    // equalize, since that would add a query to the valid-login hot path to
    // chase a sub-noise residual.
    let has_non_guest_users = match env.db.users.has_non_guest_users().await {
        Ok(has_users) => has_users,
        Err(e) => {
            error!(ip = %env.peer_addr, target = %username, err = %e, "{}", LOG_LOGIN_DB_ERROR);
            return Err(err_database(locale));
        }
    };

    if has_non_guest_users {
        // Equalize timing against the wrong-password path so response time
        // does not reveal whether the username exists. The helper runs the
        // real verify path against a fixed dummy hash; the result is
        // intentionally discarded — externally this is invalid credentials
        // either way.
        let _ = db::verify_unknown_user_password_for_timing(password.to_owned()).await;
        debit();
        return Err(err_invalid_credentials(locale));
    }

    if let Err(error_msg) = validate_login_avatar(avatar, locale, env.peer_addr).await {
        debit();
        return Err(error_msg);
    }

    let min_strength = env.db.config.get_min_password_strength().await;
    let hashed_password = match db::hash_password_async(password.to_owned(), min_strength, false)
        .await
    {
        Ok(hash) => hash,
        Err(e) => {
            error!(ip = %env.peer_addr, target = %username, err = %e, "{}", LOG_LOGIN_HASH_ERROR);
            // Attacker-influenced on a fresh server: hash_password
            // revalidates password strength, so a weak password fails
            // here. Debit like any other failed unauthenticated login.
            debit();
            return Err(err_failed_to_create_user(locale, username));
        }
    };

    // First user becomes admin; the DB method enforces atomicity.
    match env
        .db
        .users
        .create_first_user_if_none_exist(username, &hashed_password)
        .await
    {
        Ok(Some(account)) => {
            info!(user = %username, ip = %env.peer_addr, "{}", LOG_LOGIN_FIRST_ADMIN);
            // Avatar already validated above (before the hash work).
            Ok((account, true))
        }
        Ok(None) => {
            // Reuse the invalid-credentials error so we don't reveal whether
            // the username exists.
            debit();
            Err(err_invalid_credentials(locale))
        }
        Err(e) => {
            error!(ip = %env.peer_addr, target = %username, err = %e, "{}", LOG_LOGIN_CREATE_USER_ERROR);
            Err(err_failed_to_create_user(locale, username))
        }
    }
}

/// Phase 3 of login: shared accounts must present a unique nickname —
/// validated, colliding with no username (case-insensitive) and no active
/// session's nickname. Regular accounts ignore the field. This is the
/// pre-lock check; the `'locked` block re-checks both collisions under the
/// user-state guard. Never touches the socket.
async fn validate_shared_nickname(
    db: &Database,
    user_manager: &UserManager,
    account_is_shared: bool,
    nickname: Option<String>,
    username: &str,
    peer_addr: SocketAddr,
    locale: &str,
) -> Result<Option<String>, String> {
    if !account_is_shared {
        return Ok(None);
    }

    let Some(nickname) = nickname else {
        return Err(err_nickname_required(locale));
    };

    if let Err(e) = validators::validate_nickname(&nickname) {
        return Err(match e {
            NicknameError::Empty => err_nickname_empty(locale),
            NicknameError::TooLong => {
                err_nickname_too_long(locale, validators::MAX_NICKNAME_LENGTH)
            }
            NicknameError::InvalidCharacters => err_nickname_invalid(locale),
        });
    }

    // A nickname must not collide with any existing username (case-insensitive).
    match db.users.username_exists(&nickname).await {
        Ok(true) => {
            return Err(err_nickname_unavailable(locale));
        }
        Ok(false) => {}
        Err(e) => {
            error!(ip = %peer_addr, target = %username, err = %e, "{}", LOG_LOGIN_DB_NICKNAME);
            return Err(err_database(locale));
        }
    }

    // …nor with an active session's nickname (case-insensitive).
    if user_manager.is_nickname_in_use(&nickname).await {
        return Err(err_nickname_unavailable(locale));
    }

    Ok(Some(nickname))
}

/// Auto-join the admin-configured channels for a freshly created session,
/// returning the join infos for the LoginResponse. Must run inside the
/// `read_user_state` guard: the first-presence `ChatUserJoined` broadcasts
/// below serialize against renames. Per-channel errors (missing channel
/// without ChatCreate, at capacity, …) skip that channel by design.
/// `wants_voiced` gates the voiced list on the voice feature plus
/// `voice_listen`, both resolved by the caller from the session.
async fn auto_join_channels(
    channel_manager: &ChannelManager,
    user_manager: &UserManager,
    voice_registry: &VoiceRegistry,
    session: &UserSession,
    channel_names: Vec<String>,
    policy: JoinPolicy,
    wants_voiced: bool,
) -> Vec<ChannelJoinInfo> {
    let mut joined_channels = Vec::new();
    for channel_name in channel_names {
        // Skip on any error (missing channel + no ChatCreate, at limit, …).
        let Ok(result) = channel_manager
            .join(&channel_name, session.session_id, policy)
            .await
        else {
            continue;
        };

        joined_channels.push(
            build_channel_join_info(
                user_manager,
                voice_registry,
                session,
                channel_name,
                result,
                wants_voiced,
            )
            .await,
        );
    }
    joined_channels
}

/// Phase 1 of login: protocol gates and input validation, plus feature
/// activation. Pure (no DB, no limiter, no socket) — every rejection is
/// the translated message for the caller's `send_error_and_disconnect`.
fn validate_login_inputs(
    username: &str,
    password: &str,
    features: Vec<String>,
    locale: &str,
    handshake_complete: bool,
    already_logged_in: bool,
    peer_addr: SocketAddr,
) -> Result<Vec<String>, String> {
    if !handshake_complete {
        warn!(ip = %peer_addr, "{}", LOG_LOGIN_HANDSHAKE_REQUIRED);
        return Err(err_handshake_required(locale));
    }

    if already_logged_in {
        warn!(ip = %peer_addr, "{}", LOG_LOGIN_ALREADY_LOGGED_IN);
        return Err(err_already_logged_in(locale));
    }

    if let Err(e) = validators::validate_username(username) {
        return Err(match e {
            UsernameError::Empty => err_username_empty(locale),
            UsernameError::TooLong => {
                err_username_too_long(locale, validators::MAX_USERNAME_LENGTH)
            }
            UsernameError::InvalidCharacters => err_username_invalid(locale),
        });
    }

    // Empty password is allowed (guest login); only length is rejected here.
    if let Err(PasswordError::TooLong) = validators::validate_password_input(password) {
        return Err(err_password_too_long(
            locale,
            validators::MAX_PASSWORD_LENGTH,
        ));
    }

    if let Err(e) = validators::validate_locale(locale) {
        return Err(match e {
            LocaleError::TooLong => err_locale_too_long(locale, validators::MAX_LOCALE_LENGTH),
            LocaleError::InvalidCharacters => err_locale_invalid_characters(locale),
        });
    }

    if let Err(e) = validators::validate_features(&features) {
        return Err(match e {
            FeaturesError::TooMany => err_features_too_many(locale, validators::MAX_FEATURES_COUNT),
            FeaturesError::EmptyFeature => err_features_empty_feature(locale),
            FeaturesError::FeatureTooLong => {
                err_features_feature_too_long(locale, validators::MAX_FEATURE_LENGTH)
            }
            FeaturesError::InvalidCharacters => err_features_invalid_characters(locale),
        });
    }

    Ok(activate_supported_features(features))
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

    let activated_features = match validate_login_inputs(
        &username,
        &password,
        features,
        &locale,
        handshake_complete,
        session_id.is_some(),
        ctx.peer_addr,
    ) {
        Ok(activated) => activated,
        Err(error_msg) => {
            return ctx
                .send_error_and_disconnect(&error_msg, Some(HANDLER_LOGIN))
                .await;
        }
    };

    // Per-IP failed-login limiter, checked before the account lookup and any
    // Argon2 work. The rejection is uniform for every username — limited IPs
    // can't distinguish existing from nonexistent accounts — so this cannot
    // reopen username enumeration. Trusted IPs (e.g. an operator-trusted
    // shared NAT) bypass the limiter; successes never debit it.
    let login_ip = ctx.peer_addr.ip();
    let login_ip_trusted = ctx.ip_rule_cache.is_trusted(login_ip);
    if !login_ip_trusted && ctx.login_limiter.check_only(login_ip) == RateCheck::Limited {
        warn!(ip = %ctx.peer_addr, "{}", LOG_LOGIN_RATE_LIMITED);
        return ctx
            .send_error_and_disconnect(&err_login_rate_limited(&locale), Some(HANDLER_LOGIN))
            .await;
    }

    let auth_env = AuthEnv {
        db: ctx.db,
        login_limiter: ctx.login_limiter.as_ref(),
        peer_addr: ctx.peer_addr,
        login_ip_trusted,
    };
    let (authenticated_account, avatar_validated) =
        match authenticate_or_bootstrap(&auth_env, &username, &password, &avatar, &locale).await {
            Ok(authenticated) => authenticated,
            Err(error_msg) => {
                return ctx
                    .send_error_and_disconnect(&error_msg, Some(HANDLER_LOGIN))
                    .await;
            }
        };

    // Shared accounts require a unique nickname; regular accounts ignore it.
    let validated_nickname = match validate_shared_nickname(
        ctx.db,
        ctx.user_manager,
        authenticated_account.is_shared,
        nickname,
        &username,
        ctx.peer_addr,
        &locale,
    )
    .await
    {
        Ok(validated) => validated,
        Err(error_msg) => {
            return ctx
                .send_error_and_disconnect(&error_msg, Some(HANDLER_LOGIN))
                .await;
        }
    };

    if !avatar_validated
        && let Err(error_msg) = validate_login_avatar(&avatar, &locale, ctx.peer_addr).await
    {
        return ctx
            .send_error_and_disconnect(&error_msg, Some(HANDLER_LOGIN))
            .await;
    }

    let result: Result<LoginSuccess, String> = 'locked: {
        let _user_state = ctx.user_manager.read_user_state().await;

        let user_snapshot = match ctx
            .db
            .get_login_session_snapshot(authenticated_account.id)
            .await
        {
            Ok(Some(s)) if s.account.enabled => s,
            Ok(Some(_)) => {
                break 'locked Err(err_account_disabled(
                    &locale,
                    &authenticated_account.username,
                ));
            }
            Ok(None) => {
                break 'locked Err(err_authentication(&locale));
            }
            Err(e) => {
                break 'locked Err(handle_login_snapshot_error(
                    &e,
                    &authenticated_account.username,
                    ctx.peer_addr,
                    &locale,
                ));
            }
        };

        // A rename or password reset between initial fetch and lock acquisition —
        // reject as stale credentials.
        if fold_name(&username) != fold_name(&user_snapshot.account.username) {
            warn!(
                user = %username,
                new_username = %user_snapshot.account.username,
                ip = %ctx.peer_addr,
                "{}", LOG_LOGIN_RENAMED_MID_LOGIN
            );
            break 'locked Err(err_invalid_credentials(&locale));
        }
        if authenticated_account.hashed_password != user_snapshot.account.hashed_password {
            warn!(
                user = %authenticated_account.username,
                ip = %ctx.peer_addr,
                "{}", LOG_LOGIN_PASSWORD_CHANGED
            );
            break 'locked Err(err_invalid_credentials(&locale));
        }

        let has_chat_feature = activated_features.iter().any(|f| f == FEATURE_CHAT);
        let has_voice_feature = activated_features.iter().any(|f| f == FEATURE_VOICE);

        // Regular accounts inherit is_away/status from the latest existing
        // session so a multi-device login doesn't clear away state.
        let (inherited_is_away, inherited_status) = if !user_snapshot.account.is_shared {
            let existing_sessions = ctx
                .user_manager
                .get_sessions_by_user_id(user_snapshot.account.id)
                .await;
            if let Some(latest) = existing_sessions
                .iter()
                .max_by_key(|s| (s.login_time, s.session_id))
            {
                (latest.is_away, latest.status.clone())
            } else {
                (false, None)
            }
        } else {
            (false, None)
        };

        // Re-check username_exists under the lock: a rename may have committed
        // since the pre-check, including renaming an offline account into the
        // chosen nickname.
        if let Some(ref nickname) = validated_nickname {
            match ctx.db.users.username_exists(nickname).await {
                Ok(true) => {
                    break 'locked Err(err_nickname_unavailable(&locale));
                }
                Ok(false) => {}
                Err(e) => {
                    error!(ip = %ctx.peer_addr, target = %username, err = %e, "{}", LOG_LOGIN_DB_NICKNAME);
                    break 'locked Err(err_database(&locale));
                }
            }
        }

        // Late ban check, before creating the session: a ban may have committed
        // between the pre-TLS accept check and acquiring user_state.
        // check_admission waits for any in-flight ban mutation, so it observes
        // every ban committed before now. Checking here (still under
        // read_user_state) means a banned login never creates a transient
        // session; a ban that commits after this point is blocked until we
        // release user_state, then handled by ban_create's own teardown.
        if let IpAdmission::Banned { expires_at } =
            ctx.ip_rule_cache.check_admission(ctx.peer_addr.ip()).await
        {
            break 'locked Err(late_ban_error(&locale, expires_at));
        }

        // add_user rechecks active-nickname uniqueness and inserts atomically.
        let id = match ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0, // Will be assigned by add_user
                user_id: user_snapshot.account.id,
                username: user_snapshot.account.username.clone(),
                is_admin: user_snapshot.account.is_admin,
                is_shared: user_snapshot.account.is_shared,
                permissions: user_snapshot.permissions.permissions.clone(),
                address: ctx.peer_addr,
                created_at: user_snapshot.account.created_at,
                tx: ctx.tx.clone(),
                features: activated_features.clone(),
                locale: locale.clone(),
                avatar: avatar.clone(),
                nickname: validated_nickname
                    .clone()
                    .unwrap_or_else(|| user_snapshot.account.username.clone()),
                is_away: inherited_is_away,
                status: inherited_status,
                group_id: user_snapshot.account.group_id,
                group_name: user_snapshot.group_name.clone(),
                bandwidth_weight: user_snapshot.resolved_bandwidth_weight,
                bandwidth_weight_override: user_snapshot.account.bandwidth_weight,
                last_activity: std::time::Instant::now(),
            })
            .await
        {
            Ok(id) => id,
            Err(AddUserError::NicknameInUse) => {
                break 'locked Err(err_nickname_unavailable(&locale));
            }
        };

        *session_id = Some(id);

        // Session is the source of truth for everything after this point.
        let session = match ctx.user_manager.get_user_by_session_id(id).await {
            Some(s) => s,
            None => {
                *session_id = None;
                break 'locked Err(err_authentication(&locale));
            }
        };

        let can_auto_join = has_chat_feature && session.has_permission(Permission::ChatJoin);
        let has_chat_create_permission = session.has_permission(Permission::ChatCreate);
        let has_voice_listen_permission = session.has_permission(Permission::VoiceListen);

        // Acquire server info read lock right before config read.
        let _server_info = ctx.user_manager.read_server_info_state().await;
        let config = ctx.db.config.get_all().await;

        // Admin-configured auto-join channels.
        let auto_join_channel_names = if can_auto_join {
            crate::db::ConfigDb::parse_channel_list(&config.auto_join_channels)
        } else {
            Vec::new()
        };

        let auto_join_policy = if has_chat_create_permission {
            JoinPolicy::CreateIfMissing
        } else {
            JoinPolicy::ExistingOnly
        };

        let joined_channels = auto_join_channels(
            ctx.channel_manager,
            ctx.user_manager,
            ctx.voice_registry,
            &session,
            auto_join_channel_names,
            auto_join_policy,
            has_voice_feature && has_voice_listen_permission,
        )
        .await;

        // Resolved effective permissions. Admins get an empty list; the client
        // infers "all" from the is_admin flag.
        let user_permissions: Vec<String> = if session.is_admin {
            vec![]
        } else {
            session
                .permissions
                .iter()
                .map(|p| p.as_str().to_string())
                .collect()
        };

        let server_info_values =
            ServerInfoValues::from_config(config, ctx.transfer_port, ctx.transfer_websocket_port);

        let server_info_options = ServerInfoOptions {
            is_admin: session.is_admin,
            has_file_reindex: session.has_permission(Permission::FileReindex),
            has_chat_join: can_auto_join,
            include_image: true,
        };

        let server_info = Some(build_server_info(&server_info_values, &server_info_options));

        let channels = if joined_channels.is_empty() {
            None
        } else {
            Some(joined_channels)
        };

        let response = ServerMessage::LoginResponse {
            success: true,
            session_id: Some(id),
            user_id: Some(session.user_id),
            is_admin: Some(session.is_admin),
            permissions: Some(user_permissions),
            features: Some(activated_features.clone()),
            server_info,
            locale: Some(locale.clone()),
            channels,
            nickname: Some(session.nickname.clone()),
            error: None,
            group_id: session.group_id,
            group_name: session.group_name.clone(),
        };

        // Build UserConnected inside the lock while session data is consistent.
        let mut user_info = UserManager::build_user_info_from_session(&session);
        user_info.avatar = if session.is_shared {
            session.avatar.as_deref().map(String::from)
        } else {
            let sessions = ctx
                .user_manager
                .get_sessions_by_user_id(session.user_id)
                .await;
            UserManager::aggregate_avatar(sessions.iter())
        };
        let user_connected = ServerMessage::UserConnected { user: user_info };
        let bandwidth_weight = session.bandwidth_weight.load(Ordering::Relaxed);
        if ctx.egress_connection_registered {
            transition_login_to_user_flow(
                ctx.egress,
                ctx.egress_connection_id,
                session.user_id,
                bandwidth_weight,
                ctx.peer_addr,
            );
        }

        // Announce UserConnected while still holding read_user_state so a
        // concurrent rename can't serialize its UserUpdated out ahead of this and
        // leave other clients with a ghost nickname. The broadcast is queued
        // (non-blocking) and takes only the user-map lock, not user_state, so it
        // is safe under the guard. The LoginResponse send stays outside — it is
        // direct socket I/O and must not hold the lock.
        ctx.user_manager
            .broadcast_user_event(user_connected, Some(id))
            .await;

        debug!(user = %session.username, ip = %ctx.peer_addr, "{}", LOG_LOGIN_SUCCESS);
        Ok(LoginSuccess {
            response: Box::new(response),
        })
        // _user_state and _server_info drop here
    };

    match result {
        Ok(success) => {
            // UserConnected was already broadcast under the read_user_state guard
            // (above), before this LoginResponse, so other clients know about the
            // user before they can interact with it.
            ctx.send_message(&success.response).await?;
        }
        Err(msg) => {
            return ctx
                .send_error_and_disconnect(&msg, Some(HANDLER_LOGIN))
                .await;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::{FEATURE_FILES, FEATURE_NEWS, FEATURE_VOICE};
    use crate::egress::task::EgressSettingsCommand;
    use crate::handlers::testing::{
        DEFAULT_TEST_LOCALE, TestContext, create_test_context, get_cached_password_hash,
        login_user_with_features, read_login_response, read_server_message,
    };
    use crate::voice::VoiceSession;

    const INVALID_AVATAR_FORMAT: &str = "data:image/png,notbase64encoded";

    fn expect_egress_transition(test_ctx: &mut TestContext) -> (i64, u16) {
        let transition = test_ctx
            .egress_settings_rx
            .try_recv()
            .expect("Login should transition egress to the user flow");
        match transition {
            EgressSettingsCommand::TransitionToUser {
                connection_id,
                user_id,
                weight,
            } => {
                assert_eq!(
                    connection_id, test_ctx.egress_connection_id,
                    "Transition should target this egress connection"
                );
                (user_id, weight)
            }
            _ => panic!("Expected egress TransitionToUser command"),
        }
    }

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
        assert!(
            test_ctx.egress_settings_rx.try_recv().is_err(),
            "Failed login should not transition egress"
        );
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

        let session = test_ctx
            .user_manager
            .get_user_by_session_id(session_id.expect("Session ID should be set"))
            .await
            .expect("Session should exist");
        let (transition_user_id, transition_weight) = expect_egress_transition(&mut test_ctx);
        assert_eq!(transition_user_id, session.user_id);
        assert_eq!(
            transition_weight,
            session.bandwidth_weight.load(Ordering::Relaxed),
            "Transition should use the session's resolved bandwidth weight"
        );

        let response_msg = read_login_response(&mut test_ctx).await;
        match response_msg {
            ServerMessage::LoginResponse {
                success,
                session_id,
                user_id,
                is_admin,
                permissions,
                features,
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
                assert_eq!(
                    features,
                    Some(vec![FEATURE_CHAT.to_string()]),
                    "LoginResponse should return activated features"
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
    async fn test_login_filters_unsupported_features() {
        let mut test_ctx = create_test_context().await;
        let mut session_id = None;
        let requested_features = vec![
            FEATURE_VOICE.to_string(),
            FEATURE_CHAT.to_string(),
            "boards".to_string(),
            FEATURE_NEWS.to_string(),
            FEATURE_CHAT.to_string(),
            FEATURE_FILES.to_string(),
        ];
        let activated_features = vec![
            FEATURE_CHAT.to_string(),
            FEATURE_FILES.to_string(),
            FEATURE_NEWS.to_string(),
            FEATURE_VOICE.to_string(),
        ];

        let request = LoginRequest {
            username: "alice".to_string(),
            password: "password123".to_string(),
            features: requested_features,
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: None,
            handshake_complete: true,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Login should succeed with unknown features");
        let session = test_ctx
            .user_manager
            .get_user_by_session_id(session_id.expect("Session ID should be set"))
            .await
            .expect("Session should exist");
        assert_eq!(
            &*session.features,
            activated_features.as_slice(),
            "Session should store only server-supported features"
        );

        let response_msg = read_login_response(&mut test_ctx).await;
        match response_msg {
            ServerMessage::LoginResponse {
                success, features, ..
            } => {
                assert!(success, "Login should indicate success");
                assert_eq!(
                    features,
                    Some(session.features.to_vec()),
                    "LoginResponse should return the activated feature list"
                );
            }
            _ => panic!("Expected LoginResponse"),
        }
    }

    #[tokio::test]
    async fn test_login_succeeds_when_egress_transition_channel_closed() {
        let mut test_ctx = create_test_context().await;
        let (_replacement_tx, replacement_rx) = tokio::sync::mpsc::unbounded_channel();
        let old_rx = std::mem::replace(&mut test_ctx.egress_settings_rx, replacement_rx);
        drop(old_rx);

        let mut session_id = None;
        let request = LoginRequest {
            username: "alice".to_string(),
            password: "password123".to_string(),
            features: vec![FEATURE_CHAT.to_string()],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: None,
            handshake_complete: true,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(
            result.is_ok(),
            "Login should succeed when egress transition cannot be sent"
        );
        assert!(session_id.is_some(), "Session ID should be set");

        let response_msg = read_login_response(&mut test_ctx).await;
        match response_msg {
            ServerMessage::LoginResponse { success, error, .. } => {
                assert!(success, "Login should still indicate success");
                assert!(error.is_none(), "Login should have no error");
            }
            _ => panic!("Expected LoginResponse"),
        }
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
    async fn test_login_late_ban_check_rejects_after_admission_race() {
        let mut test_ctx = create_test_context().await;

        let password = "mypassword";
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

        let peer_ip = test_ctx.peer_addr.ip().to_string();
        {
            let mut cache = test_ctx.ip_rule_cache.write();
            cache.add_ban(&peer_ip, None);
        }

        let mut session_id = None;
        let request = LoginRequest {
            username: "bob".to_string(),
            password: password.to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: None,
            handshake_complete: true,
        };

        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(result.is_err(), "banned IP should be rejected at login");
        assert_eq!(
            result.unwrap_err().to_string(),
            err_banned_permanent(DEFAULT_TEST_LOCALE)
        );
        assert!(session_id.is_none(), "Session ID should remain unset");
        assert_eq!(
            test_ctx.user_manager.user_count().await,
            0,
            "late ban rejection must not create the session"
        );
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
    async fn test_login_wrong_password_does_not_validate_avatar() {
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
        let request = LoginRequest {
            username: "bob".to_string(),
            password: "wrongpassword".to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: Some(INVALID_AVATAR_FORMAT.to_string()),
            nickname: None,
            handshake_complete: true,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(result.is_err(), "Login with wrong password should fail");
        assert!(session_id.is_none(), "Session ID should remain None");

        let response_msg = read_server_message(&mut test_ctx).await;
        match response_msg {
            ServerMessage::Error { message, .. } => {
                assert_eq!(message, err_invalid_credentials(DEFAULT_TEST_LOCALE));
            }
            _ => panic!("Expected Error message"),
        }
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
    async fn test_login_nonexistent_user_with_weak_password_uses_generic_auth_error() {
        let mut test_ctx = create_test_context().await;

        let hashed = get_cached_password_hash("password");
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
        let request = LoginRequest {
            username: "nonexistent".to_string(),
            // The dummy-verify timing helper itself is pinned in db::password
            // tests; this handler-level test guards the user-visible behavior.
            password: "a".to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: None,
            handshake_complete: true,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(result.is_err());
        assert!(session_id.is_none());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::Error { message, .. } => {
                assert_eq!(message, err_invalid_credentials(DEFAULT_TEST_LOCALE));
            }
            other => panic!("Expected Error message, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_login_nonexistent_user_does_not_validate_avatar() {
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
        let request = LoginRequest {
            username: "nonexistent".to_string(),
            password: "password".to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: Some(INVALID_AVATAR_FORMAT.to_string()),
            nickname: None,
            handshake_complete: true,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(
            result.is_err(),
            "Login as non-existent user should fail after first user"
        );
        assert!(session_id.is_none(), "Session ID should remain None");

        let response_msg = read_server_message(&mut test_ctx).await;
        match response_msg {
            ServerMessage::Error { message, .. } => {
                assert_eq!(message, err_invalid_credentials(DEFAULT_TEST_LOCALE));
            }
            _ => panic!("Expected Error message"),
        }
    }

    async fn create_plain_user(test_ctx: &mut crate::handlers::testing::TestContext, name: &str) {
        let hashed = get_cached_password_hash("password");
        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: name,
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
    }

    fn login_request_for(username: &str, password: &str) -> LoginRequest {
        LoginRequest {
            username: username.to_string(),
            password: password.to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: None,
            handshake_complete: true,
        }
    }

    async fn enable_guest_account(test_ctx: &mut TestContext) {
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
    }

    async fn read_error_message(test_ctx: &mut TestContext) -> String {
        match read_server_message(test_ctx).await {
            ServerMessage::Error { message, .. } => message,
            _ => panic!("Expected Error message"),
        }
    }

    #[tokio::test]
    async fn test_login_rate_limited_after_failed_attempts() {
        let mut test_ctx = create_test_context().await;
        // Small burst so the test exercises the limit quickly.
        test_ctx.login_limiter = std::sync::Arc::new(
            nexus_common::rate_limiter::RateLimiter::with_burst_and_refill(2, 1)
                .key_ipv6_by_prefix(),
        );
        create_plain_user(&mut test_ctx, "bob").await;

        for _ in 0..2 {
            let mut session_id = None;
            let request = login_request_for("bob", "wrongpassword");
            let result =
                handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;
            assert!(result.is_err());
            let msg = read_server_message(&mut test_ctx).await;
            match msg {
                ServerMessage::Error { message, .. } => {
                    assert_eq!(message, err_invalid_credentials(DEFAULT_TEST_LOCALE));
                }
                other => panic!("expected Error, got {other:?}"),
            }
        }

        // Burst exhausted: even a correct password is rejected before any
        // account lookup or Argon2 work.
        let mut session_id = None;
        let request = login_request_for("bob", "password");
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;
        assert!(result.is_err());
        assert!(session_id.is_none());
        let msg = read_server_message(&mut test_ctx).await;
        match msg {
            ServerMessage::Error { message, .. } => {
                assert_eq!(message, err_login_rate_limited(DEFAULT_TEST_LOCALE));
            }
            other => panic!("expected Error, got {other:?}"),
        }

        // Uniform response: a nonexistent username gets the identical error,
        // so the limited state cannot be used for username enumeration.
        let mut session_id = None;
        let request = login_request_for("ghost", "password");
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;
        assert!(result.is_err());
        let msg = read_server_message(&mut test_ctx).await;
        match msg {
            ServerMessage::Error { message, .. } => {
                assert_eq!(message, err_login_rate_limited(DEFAULT_TEST_LOCALE));
            }
            other => panic!("expected Error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_login_success_does_not_debit_limiter() {
        let mut test_ctx = create_test_context().await;
        test_ctx.login_limiter = std::sync::Arc::new(
            nexus_common::rate_limiter::RateLimiter::with_burst_and_refill(1, 1)
                .key_ipv6_by_prefix(),
        );
        create_plain_user(&mut test_ctx, "bob").await;

        let mut session_id = None;
        let request = login_request_for("bob", "password");
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());
        assert!(session_id.is_some());

        // A successful login must not consume the (single) token.
        let login_ip = test_ctx.peer_addr.ip();
        assert_eq!(
            test_ctx.login_limiter.check_only(login_ip),
            nexus_common::rate_limiter::RateCheck::Allowed
        );
    }

    #[tokio::test]
    async fn test_login_rate_limit_trusted_ip_bypasses() {
        let mut test_ctx = create_test_context().await;
        test_ctx.login_limiter = std::sync::Arc::new(
            nexus_common::rate_limiter::RateLimiter::with_burst_and_refill(1, 1)
                .key_ipv6_by_prefix(),
        );
        create_plain_user(&mut test_ctx, "bob").await;
        let login_ip = test_ctx.peer_addr.ip();
        assert!(
            test_ctx
                .ip_rule_cache
                .write()
                .add_trust(&login_ip.to_string(), None)
        );

        // Trusted IPs are never limited and never debited: repeated failures
        // keep yielding invalid-credentials, not the rate-limited error.
        for _ in 0..3 {
            let mut session_id = None;
            let request = login_request_for("bob", "wrongpassword");
            let result =
                handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;
            assert!(result.is_err());
            let msg = read_server_message(&mut test_ctx).await;
            match msg {
                ServerMessage::Error { message, .. } => {
                    assert_eq!(message, err_invalid_credentials(DEFAULT_TEST_LOCALE));
                }
                other => panic!("expected Error, got {other:?}"),
            }
        }
        assert_eq!(
            test_ctx.login_limiter.check_only(login_ip),
            nexus_common::rate_limiter::RateCheck::Allowed
        );
    }

    #[tokio::test]
    async fn test_first_admin_failures_debit_limiter() {
        let mut test_ctx = create_test_context().await;
        test_ctx.login_limiter = std::sync::Arc::new(
            nexus_common::rate_limiter::RateLimiter::with_burst_and_refill(1, 1)
                .key_ipv6_by_prefix(),
        );

        // Fresh server (no accounts): a failed first-admin attempt is still an
        // unauthenticated failed login and must debit the limiter.
        let mut session_id = None;
        let mut request = login_request_for("alice", "password123");
        request.avatar = Some(INVALID_AVATAR_FORMAT.to_string());
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;
        assert!(result.is_err());
        assert!(session_id.is_none());
        let msg = read_server_message(&mut test_ctx).await;
        match msg {
            ServerMessage::Error { message, .. } => {
                assert_eq!(message, err_avatar_invalid_format(DEFAULT_TEST_LOCALE));
            }
            other => panic!("expected Error, got {other:?}"),
        }

        let login_ip = test_ctx.peer_addr.ip();
        assert_eq!(
            test_ctx.login_limiter.check_only(login_ip),
            nexus_common::rate_limiter::RateCheck::Limited
        );

        // The drained bucket now rejects further attempts up front.
        let mut session_id = None;
        let request = login_request_for("alice", "password123");
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;
        assert!(result.is_err());
        let msg = read_server_message(&mut test_ctx).await;
        match msg {
            ServerMessage::Error { message, .. } => {
                assert_eq!(message, err_login_rate_limited(DEFAULT_TEST_LOCALE));
            }
            other => panic!("expected Error, got {other:?}"),
        }
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
    async fn test_login_auto_join_voiced_requires_voice_feature_and_voice_listen() {
        let mut test_ctx = create_test_context().await;
        let channel = nexus_common::validators::DEFAULT_CHANNEL;

        let alice_session = login_user_with_features(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::VoiceListen],
            false,
            vec![FEATURE_VOICE.to_string()],
        )
        .await;
        test_ctx
            .channel_manager
            .join(
                channel,
                alice_session,
                crate::channels::JoinPolicy::CreateIfMissing,
            )
            .await
            .unwrap();
        test_ctx
            .voice_registry
            .add(VoiceSession::new(
                "alice".to_string(),
                vec![channel.to_string()],
                alice_session,
            ))
            .await
            .unwrap();
        test_ctx
            .db
            .config
            .set_auto_join_channels(channel)
            .await
            .unwrap();

        let password = "password";
        let hashed = get_cached_password_hash(password);
        let mut perms = db::Permissions::new();
        perms.add(db::Permission::ChatJoin);
        perms.add(db::Permission::VoiceListen);

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
        handle_login(
            bob_request,
            &mut bob_session_id,
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        match read_login_response(&mut test_ctx).await {
            ServerMessage::LoginResponse {
                success, channels, ..
            } => {
                assert!(success);
                let channels = channels.expect("Bob should auto-join the channel");
                assert_eq!(channels[0].voiced, None);
            }
            other => panic!("Expected LoginResponse, got {other:?}"),
        }

        test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "charlie",
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

        let mut charlie_session_id = None;
        let charlie_request = LoginRequest {
            username: "charlie".to_string(),
            password: password.to_string(),
            features: vec![FEATURE_CHAT.to_string(), FEATURE_VOICE.to_string()],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: None,
            handshake_complete: true,
        };
        handle_login(
            charlie_request,
            &mut charlie_session_id,
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();

        match read_login_response(&mut test_ctx).await {
            ServerMessage::LoginResponse {
                success, channels, ..
            } => {
                assert!(success);
                let channels = channels.expect("Charlie should auto-join the channel");
                assert_eq!(channels[0].voiced, Some(vec!["alice".to_string()]));
            }
            other => panic!("Expected LoginResponse, got {other:?}"),
        }
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

        // A real, complete SVG — usvg parses it (a bare "<svg>" would not).
        let valid_avatar =
            "data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHdpZHRoPSIxIiBoZWlnaHQ9IjEiPjwvc3ZnPg=="
                .to_string();

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
        let invalid_avatar = INVALID_AVATAR_FORMAT.to_string();

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
    async fn test_first_admin_invalid_avatar_does_not_create_account() {
        let mut test_ctx = create_test_context().await;
        let mut session_id = None;

        let request = LoginRequest {
            username: "alice".to_string(),
            password: "password123".to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: Some(INVALID_AVATAR_FORMAT.to_string()),
            nickname: None,
            handshake_complete: true,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(
            result.is_err(),
            "First-admin login with invalid avatar should fail"
        );
        assert!(session_id.is_none(), "Session ID should remain None");
        assert_eq!(
            test_ctx.user_manager.user_count().await,
            0,
            "Invalid first-admin avatar must not create a session"
        );

        let account = test_ctx
            .db
            .users
            .get_user_by_username("alice")
            .await
            .expect("user lookup should succeed");
        assert!(
            account.is_none(),
            "Invalid first-admin avatar must not create the account"
        );

        let response_msg = read_server_message(&mut test_ctx).await;
        match response_msg {
            ServerMessage::Error { message, .. } => {
                assert_eq!(message, err_avatar_invalid_format(DEFAULT_TEST_LOCALE));
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
        assert_eq!(
            read_error_message(&mut test_ctx).await,
            err_nickname_unavailable(DEFAULT_TEST_LOCALE)
        );
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
        assert_eq!(
            read_error_message(&mut test_ctx).await,
            err_nickname_unavailable(DEFAULT_TEST_LOCALE)
        );
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
        assert_eq!(
            read_error_message(&mut test_ctx).await,
            err_nickname_unavailable(DEFAULT_TEST_LOCALE)
        );
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
        assert_eq!(
            read_error_message(&mut test_ctx).await,
            err_nickname_unavailable(DEFAULT_TEST_LOCALE)
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

        let (first_transition_user_id, _) = expect_egress_transition(&mut test_ctx);
        let _response1 = read_login_response(&mut test_ctx).await;

        test_ctx.egress_connection_id = crate::scheduler::ConnectionId::new(2);
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

        let (second_transition_user_id, _) = expect_egress_transition(&mut test_ctx);
        let _response2 = read_login_response(&mut test_ctx).await;

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
        assert_eq!(
            first_transition_user_id, session1.user_id,
            "First shared-account transition should use the shared account user id"
        );
        assert_eq!(
            second_transition_user_id, session2.user_id,
            "Second shared-account transition should use the shared account user id"
        );
        assert_eq!(
            session1.user_id, session2.user_id,
            "Shared-account sessions should share one egress user flow"
        );
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
        assert_eq!(
            read_error_message(&mut test_ctx).await,
            err_nickname_unavailable(DEFAULT_TEST_LOCALE)
        );
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
    async fn test_guest_login_nickname_username_collision_is_generic() {
        let mut test_ctx = create_test_context().await;
        let handshake_complete = true;

        let hashed = get_cached_password_hash("password123");
        test_ctx
            .db
            .users
            .create_first_user_if_none_exist("alice", &hashed)
            .await
            .expect("admin creation should succeed");
        enable_guest_account(&mut test_ctx).await;

        let mut session_id = None;
        let request = LoginRequest {
            username: String::new(),
            password: String::new(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: Some("Alice".to_string()),
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(
            result.is_err(),
            "Guest login with username-colliding nickname should fail"
        );
        assert!(session_id.is_none(), "Session ID should not be set");
        assert_eq!(
            read_error_message(&mut test_ctx).await,
            err_nickname_unavailable(DEFAULT_TEST_LOCALE)
        );
    }

    #[tokio::test]
    async fn test_guest_login_active_nickname_collision_is_generic() {
        let mut test_ctx = create_test_context().await;
        let handshake_complete = true;
        enable_guest_account(&mut test_ctx).await;

        let mut first_session_id = None;
        let first_request = LoginRequest {
            username: String::new(),
            password: String::new(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: Some("GuestTaken".to_string()),
            handshake_complete,
        };
        let first_result = handle_login(
            first_request,
            &mut first_session_id,
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(first_result.is_ok(), "Initial guest login should succeed");
        let _transition = expect_egress_transition(&mut test_ctx);
        let _response = read_login_response(&mut test_ctx).await;

        test_ctx.egress_connection_id = crate::scheduler::ConnectionId::new(2);
        let mut second_session_id = None;
        let second_request = LoginRequest {
            username: String::new(),
            password: String::new(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: Some("GuestTaken".to_string()),
            handshake_complete,
        };
        let second_result = handle_login(
            second_request,
            &mut second_session_id,
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(
            second_result.is_err(),
            "Guest login with active nickname collision should fail"
        );
        assert!(second_session_id.is_none(), "Session ID should not be set");
        assert_eq!(
            read_error_message(&mut test_ctx).await,
            err_nickname_unavailable(DEFAULT_TEST_LOCALE)
        );
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
        while let Ok(event) = test_ctx.rx.try_recv() {
            let (msg, _) = event.expect_message();
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
