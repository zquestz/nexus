//! UserCreate message handler

use std::io;
use std::sync::atomic::Ordering;

use tokio::io::AsyncWrite;
use tracing::{error, info, warn};

use crate::constants::{
    HANDLER_USER_CREATE, LOG_USER_CREATE_DB_ERROR, LOG_USER_CREATE_HASH_ERROR,
    LOG_USER_CREATE_NOT_LOGGED_IN, LOG_USER_CREATE_PERMISSION_DENIED, LOG_USER_CREATE_SUCCESS,
    LOG_USER_CREATE_UNOWNED_GROUP, LOG_USER_CREATE_UNOWNED_PERMISSION,
    LOG_USER_CREATE_UNOWNED_REVOKE,
};

use nexus_common::is_shared_account_permission;
use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{
    self, BandwidthWeightError, MIN_BANDWIDTH_WEIGHT, PasswordError, PermissionsError,
    UsernameError, validate_bandwidth_weight,
};

#[cfg(test)]
use super::testing::DEFAULT_TEST_LOCALE;
use super::{
    HandlerContext, err_admin_cannot_have_group, err_authentication,
    err_bandwidth_weight_delegation, err_bandwidth_weight_zero, err_cannot_create_admin,
    err_database, err_group_not_found, err_group_shared_mismatch, err_not_logged_in,
    err_password_empty, err_password_too_long, err_password_too_weak, err_permission_denied,
    err_permissions_contains_newlines, err_permissions_empty_permission,
    err_permissions_invalid_characters, err_permissions_permission_too_long,
    err_permissions_too_many, err_shared_cannot_be_admin, err_shared_invalid_permissions,
    err_unknown_permission, err_username_empty, err_username_exists, err_username_invalid,
    err_username_too_long,
};
use crate::db::{CreateUserParams, Permission, Permissions, hash_password_async};

/// User creation request parameters
pub struct UserCreateRequest {
    pub username: String,
    pub password: String,
    pub is_admin: bool,
    pub is_shared: bool,
    pub enabled: bool,
    pub permissions: Vec<String>,
    pub group_id: Option<i64>,
    pub revokes: Option<Vec<String>>,
    pub bandwidth_weight: Option<u16>,
    pub inherit_bandwidth_weight: Option<bool>,
}

/// Handle a user creation request from the client
pub async fn handle_user_create<W>(
    request: UserCreateRequest,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let UserCreateRequest {
        username,
        password,
        is_admin,
        is_shared,
        enabled,
        permissions,
        group_id,
        revokes,
        bandwidth_weight,
        inherit_bandwidth_weight,
    } = request;

    // Verify authentication
    let Some(requesting_session_id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_USER_CREATE_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_USER_CREATE))
            .await;
    };

    // Acquire the state lock before fetching the requester so
    // requester-dependent authorization sees a snapshot that can't
    // drift mid-handler. Dropped at every early-reject path before
    // socket I/O, and at the end of the success arm.
    let _state_guard = ctx.user_manager.lock_user_state().await;
    let requesting_user = match ctx
        .user_manager
        .get_user_by_session_id(requesting_session_id)
        .await
    {
        Some(u) => u,
        None => {
            drop(_state_guard);
            return ctx
                .send_error_and_disconnect(
                    &err_authentication(ctx.locale),
                    Some(HANDLER_USER_CREATE),
                )
                .await;
        }
    };

    // Check UserCreate permission (uses cached permissions, admin bypass built-in)
    if !requesting_user.has_permission(Permission::UserCreate) {
        warn!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_USER_CREATE_PERMISSION_DENIED);
        let response = ServerMessage::UserCreateResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            id: None,
            username: None,
        };
        drop(_state_guard);
        return ctx.send_message(&response).await;
    }

    // Validate username format
    if let Err(e) = validators::validate_username(&username) {
        let error_msg = match e {
            UsernameError::Empty => err_username_empty(ctx.locale),
            UsernameError::TooLong => {
                err_username_too_long(ctx.locale, validators::MAX_USERNAME_LENGTH)
            }
            UsernameError::InvalidCharacters => err_username_invalid(ctx.locale),
        };
        let response = ServerMessage::UserCreateResponse {
            success: false,
            error: Some(error_msg),
            id: None,
            username: None,
        };
        drop(_state_guard);
        return ctx.send_message(&response).await;
    }

    // Verify admin creation privilege (use is_admin from UserManager).
    // Matches `user_update.rs`'s analogous gate: typed response, not
    // disconnect — privilege escalation by a non-admin client should
    // be rejected cleanly, not by terminating the session.
    if is_admin && !requesting_user.is_admin {
        let response = ServerMessage::UserCreateResponse {
            success: false,
            error: Some(err_cannot_create_admin(ctx.locale)),
            id: None,
            username: None,
        };
        drop(_state_guard);
        return ctx.send_message(&response).await;
    }

    // Shared accounts cannot be admins
    if is_shared && is_admin {
        let response = ServerMessage::UserCreateResponse {
            success: false,
            error: Some(err_shared_cannot_be_admin(ctx.locale)),
            id: None,
            username: None,
        };
        drop(_state_guard);
        return ctx.send_message(&response).await;
    }

    // Admin XOR group invariant: admins cannot be members of a group.
    // Schema CHECK is the safety net; the handler check gives a clean
    // translated error instead of a generic DB-constraint failure.
    // Reject before the expensive password hashing further below.
    if is_admin && group_id.is_some() {
        let response = ServerMessage::UserCreateResponse {
            success: false,
            error: Some(err_admin_cannot_have_group(ctx.locale)),
            id: None,
            username: None,
        };
        drop(_state_guard);
        return ctx.send_message(&response).await;
    }

    // Validate password (zxcvbn-scored — non-trivial)
    let min_strength = ctx.db.config.get_min_password_strength().await;
    if let Err(e) = validators::validate_password(&password, min_strength, &[&username]) {
        let error_msg = match e {
            PasswordError::Empty => err_password_empty(ctx.locale),
            PasswordError::TooLong => {
                err_password_too_long(ctx.locale, validators::MAX_PASSWORD_LENGTH)
            }
            PasswordError::TooWeak { required, .. } => {
                err_password_too_weak(ctx.locale, required.score())
            }
        };
        let response = ServerMessage::UserCreateResponse {
            success: false,
            error: Some(error_msg),
            id: None,
            username: None,
        };
        drop(_state_guard);
        return ctx.send_message(&response).await;
    }

    // Validate permissions format
    if let Err(e) = validators::validate_permissions(&permissions) {
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
        let response = ServerMessage::UserCreateResponse {
            success: false,
            error: Some(error_msg),
            id: None,
            username: None,
        };
        drop(_state_guard);
        return ctx.send_message(&response).await;
    }

    // For shared accounts, validate that only allowed permissions are requested
    if is_shared {
        let forbidden: Vec<&str> = permissions
            .iter()
            .map(|s| s.as_str())
            .filter(|p| !is_shared_account_permission(p))
            .collect();

        if !forbidden.is_empty() {
            let response = ServerMessage::UserCreateResponse {
                success: false,
                error: Some(err_shared_invalid_permissions(
                    ctx.locale,
                    &forbidden.join(", "),
                )),
                id: None,
                username: None,
            };
            drop(_state_guard);
            return ctx.send_message(&response).await;
        }
    }

    // Validate group assignment if provided
    let validated_group_id = if let Some(gid) = group_id {
        // Fetch the group to verify it exists and check shared compatibility
        let group = match ctx.db.groups.get_group_by_id(gid).await {
            Ok(Some(g)) => g,
            Ok(None) => {
                let response = ServerMessage::UserCreateResponse {
                    success: false,
                    error: Some(err_group_not_found(ctx.locale)),
                    id: None,
                    username: None,
                };
                drop(_state_guard);
                return ctx.send_message(&response).await;
            }
            Err(e) => {
                error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_USER_CREATE_DB_ERROR);
                drop(_state_guard);
                return ctx
                    .send_error_and_disconnect(&err_database(ctx.locale), Some(HANDLER_USER_CREATE))
                    .await;
            }
        };

        // Shared account / shared group compatibility
        if is_shared && !group.is_shared {
            let response = ServerMessage::UserCreateResponse {
                success: false,
                error: Some(err_group_shared_mismatch(ctx.locale)),
                id: None,
                username: None,
            };
            drop(_state_guard);
            return ctx.send_message(&response).await;
        }
        if !is_shared && group.is_shared {
            let response = ServerMessage::UserCreateResponse {
                success: false,
                error: Some(err_group_shared_mismatch(ctx.locale)),
                id: None,
                username: None,
            };
            drop(_state_guard);
            return ctx.send_message(&response).await;
        }

        // Non-admin delegation: cannot assign a group whose bandwidth weight
        // exceeds the requester's own resolved weight. Closes the escalation
        // where a moderator could create users in a higher-weight group whose
        // permissions they happen to fully possess.
        if !requesting_user.is_admin
            && group.bandwidth_weight > requesting_user.bandwidth_weight.load(Ordering::Relaxed)
        {
            let response = ServerMessage::UserCreateResponse {
                success: false,
                error: Some(err_bandwidth_weight_delegation(ctx.locale)),
                id: None,
                username: None,
            };
            drop(_state_guard);
            return ctx.send_message(&response).await;
        }

        // Non-admin group assignment check: requester must have all group permissions
        if !requesting_user.is_admin {
            let group_perms = match ctx.db.groups.get_group_permissions(gid).await {
                Ok(p) => p,
                Err(e) => {
                    error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_USER_CREATE_DB_ERROR);
                    let response = ServerMessage::UserCreateResponse {
                        success: false,
                        error: Some(err_database(ctx.locale)),
                        id: None,
                        username: None,
                    };
                    drop(_state_guard);
                    return ctx.send_message(&response).await;
                }
            };
            for perm in &group_perms {
                if !requesting_user.has_permission(*perm) {
                    warn!(user = %requesting_user.username, ip = %ctx.peer_addr, perm = %perm.as_str(), "{}", LOG_USER_CREATE_UNOWNED_GROUP);
                    let response = ServerMessage::UserCreateResponse {
                        success: false,
                        error: Some(err_permission_denied(ctx.locale)),
                        id: None,
                        username: None,
                    };
                    drop(_state_guard);
                    return ctx.send_message(&response).await;
                }
            }
        }

        Some(gid)
    } else {
        None
    };

    // Bandwidth weight delegation: non-admins can set a per-user override
    // only if the requested value does not exceed their own resolved
    // bandwidth weight. Admins bypass the check. Skipped entirely when
    // `inherit_bandwidth_weight: Some(true)` is set — that flag wins and
    // the override value would be discarded, so checking it would reject
    // moot values from a defensive client.
    //
    // Checked here — before password hashing — so a malicious non-admin
    // can't burn server CPU on Argon2id by submitting an over-cap weight
    // and forcing a rejection mid-flight.
    if inherit_bandwidth_weight != Some(true) {
        if !requesting_user.is_admin
            && let Some(w) = bandwidth_weight
            && w > requesting_user.bandwidth_weight.load(Ordering::Relaxed)
        {
            let response = ServerMessage::UserCreateResponse {
                success: false,
                error: Some(err_bandwidth_weight_delegation(ctx.locale)),
                id: None,
                username: None,
            };
            drop(_state_guard);
            return ctx.send_message(&response).await;
        }
        if let Some(w) = bandwidth_weight
            && let Err(BandwidthWeightError::Zero) = validate_bandwidth_weight(w)
        {
            let response = ServerMessage::UserCreateResponse {
                success: false,
                error: Some(err_bandwidth_weight_zero(ctx.locale, MIN_BANDWIDTH_WEIGHT)),
                id: None,
                username: None,
            };
            drop(_state_guard);
            return ctx.send_message(&response).await;
        }
    }

    // Parse and validate revoke permissions (only meaningful with a group)
    let parsed_revokes: Vec<Permission> = if let Some(ref revoke_strings) = revokes {
        if validated_group_id.is_none() {
            // Revokes without a group are ignored
            Vec::new()
        } else {
            let mut parsed = Vec::new();
            for perm_str in revoke_strings {
                match Permission::parse(perm_str) {
                    Some(perm) => {
                        // Non-admins can only revoke permissions they themselves have
                        if !requesting_user.has_permission(perm) {
                            warn!(user = %requesting_user.username, ip = %ctx.peer_addr, perm = %perm_str, "{}", LOG_USER_CREATE_UNOWNED_REVOKE);
                            let response = ServerMessage::UserCreateResponse {
                                success: false,
                                error: Some(err_permission_denied(ctx.locale)),
                                id: None,
                                username: None,
                            };
                            drop(_state_guard);
                            return ctx.send_message(&response).await;
                        }
                        parsed.push(perm);
                    }
                    None => {
                        let response = ServerMessage::UserCreateResponse {
                            success: false,
                            error: Some(err_unknown_permission(ctx.locale, perm_str)),
                            id: None,
                            username: None,
                        };
                        drop(_state_guard);
                        return ctx.send_message(&response).await;
                    }
                }
            }
            parsed
        }
    } else {
        Vec::new()
    };

    // Parse and validate requested permissions
    let mut perms = Permissions::new();
    for perm_str in &permissions {
        let perm = match Permission::parse(perm_str) {
            Some(p) => p,
            None => {
                // Unknown permission - return error to client
                let response = ServerMessage::UserCreateResponse {
                    success: false,
                    error: Some(err_unknown_permission(ctx.locale, perm_str)),
                    id: None,
                    username: None,
                };
                drop(_state_guard);
                return ctx.send_message(&response).await;
            }
        };

        // Non-admins can only grant permissions they have
        // Check permission delegation authority (uses cached permissions, admin bypass built-in)
        if !requesting_user.has_permission(perm) {
            warn!(user = %requesting_user.username, ip = %ctx.peer_addr, perm = %perm_str, "{}", LOG_USER_CREATE_UNOWNED_PERMISSION);
            let response = ServerMessage::UserCreateResponse {
                success: false,
                error: Some(err_permission_denied(ctx.locale)),
                id: None,
                username: None,
            };
            drop(_state_guard);
            return ctx.send_message(&response).await;
        }

        perms.permissions.insert(perm);
    }

    // Check for duplicate username
    match ctx.db.users.get_user_by_username(&username).await {
        Ok(Some(_)) => {
            // Username already exists
            let response = ServerMessage::UserCreateResponse {
                success: false,
                error: Some(err_username_exists(ctx.locale, &username)),
                id: None,
                username: None,
            };
            drop(_state_guard);
            return ctx.send_message(&response).await;
        }
        Ok(None) => {
            // Username doesn't exist, proceed with creation
        }
        Err(e) => {
            error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_USER_CREATE_DB_ERROR);
            drop(_state_guard);
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some(HANDLER_USER_CREATE))
                .await;
        }
    }

    // Hash password for secure storage
    let password_hash = match hash_password_async(password.clone(), min_strength, false).await {
        Ok(hash) => hash,
        Err(e) => {
            error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_USER_CREATE_HASH_ERROR);
            drop(_state_guard);
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some(HANDLER_USER_CREATE))
                .await;
        }
    };

    let final_bandwidth_weight = if inherit_bandwidth_weight == Some(true) {
        None
    } else {
        bandwidth_weight
    };

    // Create user in database
    match ctx
        .db
        .users
        .create_user(CreateUserParams {
            username: &username,
            hashed_password: &password_hash,
            is_admin,
            is_shared,
            enabled,
            permissions: &perms,
            group_id: validated_group_id,
            revokes: &parsed_revokes,
            bandwidth_weight: final_bandwidth_weight,
        })
        .await
    {
        Ok(user) => {
            // Success (group override cleanup is handled atomically inside create_user)
            info!(user = %requesting_user.username, ip = %ctx.peer_addr, target = %username, "{}", LOG_USER_CREATE_SUCCESS);
            let response = ServerMessage::UserCreateResponse {
                success: true,
                error: None,
                id: Some(user.id),
                username: Some(username),
            };
            drop(_state_guard);
            ctx.send_message(&response).await
        }
        Err(e) => {
            error!(user = %requesting_user.username, ip = %ctx.peer_addr, err = %e, "{}", LOG_USER_CREATE_DB_ERROR);
            drop(_state_guard);
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some(HANDLER_USER_CREATE))
                .await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::handlers::testing::{
        create_test_context, get_cached_password_hash, login_user, read_server_message,
    };
    use crate::users::user::NewSessionParams;

    #[tokio::test]
    async fn test_usercreate_requires_login() {
        let mut test_ctx = create_test_context().await;

        // Try to create user without being logged in
        let result = handle_user_create(
            UserCreateRequest {
                username: "newuser".to_string(),
                password: "password".to_string(),
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: vec![],
                group_id: None,
                revokes: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: None,
            },
            None,
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should fail with disconnect
        assert!(result.is_err(), "UserCreate should require login");
    }

    #[tokio::test]
    async fn test_usercreate_requires_permission() {
        let mut test_ctx = create_test_context().await;

        // Create user WITHOUT UserCreate permission (non-admin)
        let user_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        // Try to create user without permission
        let result = handle_user_create(
            UserCreateRequest {
                username: "newuser".to_string(),
                password: "password".to_string(),
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: vec![],
                group_id: None,
                revokes: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: None,
            },
            Some(user_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should fail with disconnect
        // Should succeed (send error but not disconnect)
        assert!(
            result.is_ok(),
            "Should send error message but not disconnect"
        );
    }

    #[tokio::test]
    async fn test_usercreate_admin_can_create() {
        let mut test_ctx = create_test_context().await;

        // Create an admin user
        let admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a new user
        let result = handle_user_create(
            UserCreateRequest {
                username: "newuser".to_string(),
                password: "newpassword".to_string(),
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: vec![],
                group_id: None,
                revokes: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: None,
            },
            Some(admin_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should succeed
        assert!(result.is_ok(), "Admin should be able to create users");

        // Close writer and read response

        // Parse and verify response
        let response_msg = read_server_message(&mut test_ctx).await;
        match response_msg {
            ServerMessage::UserCreateResponse {
                success,
                error,
                id,
                username,
            } => {
                assert!(success, "Response should indicate success");
                assert!(error.is_none(), "Should have no error message");
                assert!(id.is_some(), "Should return created user ID");
                assert_eq!(username, Some("newuser".to_string()));
            }
            _ => panic!("Expected UserCreateResponse"),
        }

        // Verify user exists in database
        let created_user = test_ctx
            .db
            .users
            .get_user_by_username("newuser")
            .await
            .unwrap();
        assert!(created_user.is_some(), "User should exist in database");
        let user = created_user.unwrap();
        assert_eq!(user.username, "newuser");
        assert!(!user.is_admin, "User should not be admin");
    }

    #[tokio::test]
    async fn test_usercreate_duplicate_username() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let password = "password";
        let hashed = get_cached_password_hash(password);
        let admin = test_ctx
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

        // Create existing user
        let _existing = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "existing",
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

        // Add admin to UserManager
        let admin_id = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                user_id: admin.id,
                username: "admin".to_string(),
                is_admin: true,
                is_shared: false,
                permissions: std::collections::HashSet::new(),
                address: test_ctx.peer_addr,
                created_at: admin.created_at,
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

        // Try to create user with duplicate username
        let result = handle_user_create(
            UserCreateRequest {
                username: "existing".to_string(),
                password: "newpassword".to_string(),
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: vec![],
                group_id: None,
                revokes: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: None,
            },
            Some(admin_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should succeed (sends error response, doesn't disconnect)
        assert!(
            result.is_ok(),
            "Should send error response for duplicate username"
        );

        // Close writer and read response

        // Parse and verify response
        let response_msg = read_server_message(&mut test_ctx).await;
        match response_msg {
            ServerMessage::UserCreateResponse { success, error, .. } => {
                assert!(!success, "Response should indicate failure");
                assert!(error.is_some(), "Should have error message");
                let error_msg = error.unwrap();
                assert!(
                    error_msg.contains("exists") || error_msg.contains("already"),
                    "Error should mention username already exists, got: {}",
                    error_msg
                );
            }
            _ => panic!("Expected UserCreateResponse"),
        }
    }

    #[tokio::test]
    async fn test_usercreate_can_create_admin() {
        let mut test_ctx = create_test_context().await;

        // Create an admin user
        let admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a new admin user
        let result = handle_user_create(
            UserCreateRequest {
                username: "newadmin".to_string(),
                password: "newpassword".to_string(),
                is_admin: true,
                is_shared: false,
                enabled: true,
                permissions: vec![],
                group_id: None,
                revokes: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: None,
            },
            Some(admin_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should succeed
        assert!(result.is_ok(), "Admin should be able to create admin users");

        // Close writer and read response

        // Parse and verify response
        let response_msg = read_server_message(&mut test_ctx).await;
        match response_msg {
            ServerMessage::UserCreateResponse {
                success,
                error,
                id,
                username,
            } => {
                assert!(success, "Response should indicate success");
                assert!(error.is_none(), "Should have no error message");
                assert!(id.is_some(), "Should return created user ID");
                assert_eq!(username, Some("newadmin".to_string()));
            }
            _ => panic!("Expected UserCreateResponse"),
        }

        // Verify user exists and is admin
        let created_user = test_ctx
            .db
            .users
            .get_user_by_username("newadmin")
            .await
            .unwrap();
        assert!(created_user.is_some(), "User should exist in database");
        let user = created_user.unwrap();
        assert_eq!(user.username, "newadmin");
        assert!(user.is_admin, "User should be admin");
    }

    #[tokio::test]
    async fn test_usercreate_with_permission() {
        let mut test_ctx = create_test_context().await;

        // Create a non-admin user WITH UserCreate permission
        let creator_id = login_user(
            &mut test_ctx,
            "creator",
            "password",
            &[db::Permission::UserCreate, db::Permission::UserList],
            false,
        )
        .await;

        // Create a new user (can only grant permissions creator has)
        let result = handle_user_create(
            UserCreateRequest {
                username: "newuser".to_string(),
                password: "password".to_string(),
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: vec!["user_list".to_string()],
                group_id: None,
                revokes: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: None,
            },
            Some(creator_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should succeed
        assert!(
            result.is_ok(),
            "User with UserCreate permission should be able to create users"
        );

        // Close writer and read response

        // Parse and verify response
        let response_msg = read_server_message(&mut test_ctx).await;
        match response_msg {
            ServerMessage::UserCreateResponse {
                success,
                error,
                id,
                username,
            } => {
                assert!(success, "Response should indicate success");
                assert!(error.is_none(), "Should have no error message");
                assert!(id.is_some(), "Should return created user ID");
                assert_eq!(username, Some("newuser".to_string()));
            }
            _ => panic!("Expected UserCreateResponse"),
        }

        // Verify user exists
        let created_user = test_ctx
            .db
            .users
            .get_user_by_username("newuser")
            .await
            .unwrap();
        assert!(created_user.is_some(), "User should exist in database");

        // Verify permissions were granted
        let user = created_user.unwrap();
        let has_user_list = test_ctx
            .db
            .users
            .has_permission(user.id, db::Permission::UserList)
            .await
            .unwrap();
        assert!(has_user_list, "User should have UserList permission");
    }

    #[tokio::test]
    async fn test_usercreate_grants_specified_permissions() {
        let mut test_ctx = create_test_context().await;

        // Create an admin user
        let admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a new user with specific permissions
        let result = handle_user_create(
            UserCreateRequest {
                username: "newuser".to_string(),
                password: "password".to_string(),
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: vec![
                    "user_list".to_string(),
                    "user_info".to_string(),
                    "chat_send".to_string(),
                ],
                group_id: None,
                revokes: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: None,
            },
            Some(admin_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should succeed
        assert!(
            result.is_ok(),
            "Admin should be able to create users with permissions"
        );

        // Close writer and read response

        // Parse and verify response
        let response_msg = read_server_message(&mut test_ctx).await;
        match response_msg {
            ServerMessage::UserCreateResponse {
                success,
                error,
                id,
                username,
            } => {
                assert!(success, "Response should indicate success");
                assert!(error.is_none(), "Should have no error message");
                assert!(id.is_some(), "Should return created user ID");
                assert_eq!(username, Some("newuser".to_string()));
            }
            _ => panic!("Expected UserCreateResponse"),
        }

        // Verify user exists and has the specified permissions
        let created_user = test_ctx
            .db
            .users
            .get_user_by_username("newuser")
            .await
            .unwrap();
        assert!(created_user.is_some(), "User should exist in database");
        let user = created_user.unwrap();

        // Check granted permissions
        let has_user_list = test_ctx
            .db
            .users
            .has_permission(user.id, db::Permission::UserList)
            .await
            .unwrap();
        let has_user_info = test_ctx
            .db
            .users
            .has_permission(user.id, db::Permission::UserInfo)
            .await
            .unwrap();
        let has_chat_send = test_ctx
            .db
            .users
            .has_permission(user.id, db::Permission::ChatSend)
            .await
            .unwrap();

        assert!(has_user_list, "User should have UserList permission");
        assert!(has_user_info, "User should have UserInfo permission");
        assert!(has_chat_send, "User should have ChatSend permission");

        // Check permissions NOT granted
        let has_chat_receive = test_ctx
            .db
            .users
            .has_permission(user.id, db::Permission::ChatReceive)
            .await
            .unwrap();
        let has_user_delete = test_ctx
            .db
            .users
            .has_permission(user.id, db::Permission::UserDelete)
            .await
            .unwrap();

        assert!(
            !has_chat_receive,
            "User should NOT have ChatReceive permission"
        );
        assert!(
            !has_user_delete,
            "User should NOT have UserDelete permission"
        );
    }

    #[tokio::test]
    async fn test_usercreate_non_admin_cannot_create_admin() {
        let mut test_ctx = create_test_context().await;

        // Create first admin
        let password = "password";
        let hashed = get_cached_password_hash(password);
        let _admin = test_ctx
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

        // Create non-admin WITH UserCreate permission
        let mut perms = db::Permissions::new();
        use std::collections::HashSet;
        perms.permissions = {
            let mut set = HashSet::new();
            set.insert(db::Permission::UserCreate);
            set
        };
        let creator = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "creator",
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

        // Add creator to UserManager
        let creator_id = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                user_id: creator.id,
                username: "creator".to_string(),
                is_admin: false,
                is_shared: false,
                permissions: perms.permissions.clone(),
                address: test_ctx.peer_addr,
                created_at: creator.created_at,
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: DEFAULT_TEST_LOCALE.to_string(),
                avatar: None,
                nickname: "creator".to_string(),
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

        // Try to create an admin user as non-admin
        let result = handle_user_create(
            UserCreateRequest {
                username: "newadmin".to_string(),
                password: "password".to_string(),
                is_admin: true,
                is_shared: false,
                enabled: true,
                permissions: vec![],
                group_id: None,
                revokes: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: None,
            },
            Some(creator_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Typed rejection (no disconnect) — mirrors UserUpdate's analogous
        // gate at `handlers/user_update.rs`.
        assert!(
            result.is_ok(),
            "Should send typed rejection, not disconnect"
        );
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserCreateResponse {
                success,
                error,
                id,
                username,
            } => {
                assert!(
                    !success,
                    "Non-admin should not be able to create admin users"
                );
                assert_eq!(error.unwrap(), err_cannot_create_admin(DEFAULT_TEST_LOCALE));
                assert!(id.is_none());
                assert!(username.is_none());
            }
            other => panic!("Expected UserCreateResponse, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_usercreate_cannot_grant_permissions_user_doesnt_have() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let password = "password";
        let hashed = get_cached_password_hash(password);
        let _admin = test_ctx
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

        // Create user WITH UserCreate permission, but NOT UserDelete permission
        let mut perms = db::Permissions::new();
        use std::collections::HashSet;
        perms.permissions = {
            let mut set = HashSet::new();
            set.insert(db::Permission::UserCreate);
            set.insert(db::Permission::ChatSend);
            set
        };
        let creator = test_ctx
            .db
            .users
            .create_user(db::CreateUserParams {
                username: "creator",
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

        // Add creator to UserManager
        let creator_id = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                user_id: creator.id,
                username: "creator".to_string(),
                is_admin: false,
                is_shared: false,
                permissions: perms.permissions.clone(),
                address: test_ctx.peer_addr,
                created_at: creator.created_at,
                tx: test_ctx.tx.clone(),
                features: vec![],
                locale: DEFAULT_TEST_LOCALE.to_string(),
                avatar: None,
                nickname: "creator".to_string(),
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

        // Try to create a user with UserDelete permission (which creator doesn't have)
        let result = handle_user_create(
            UserCreateRequest {
                username: "newuser".to_string(),
                password: "password".to_string(),
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: vec![
                    "chat_send".to_string(),   // creator has this - OK
                    "user_delete".to_string(), // creator doesn't have this - FAIL
                ],
                group_id: None,
                revokes: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: None,
            },
            Some(creator_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should fail with disconnect
        // Should succeed (send error but not disconnect)
        assert!(
            result.is_ok(),
            "Should send error message but not disconnect"
        );
    }

    #[tokio::test]
    async fn test_usercreate_empty_username() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Try to create user with empty username
        let result = handle_user_create(
            UserCreateRequest {
                username: "".to_string(),
                password: "password123".to_string(),
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: vec![],
                group_id: None,
                revokes: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: None,
            },
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Should send error response, not disconnect");
    }

    #[tokio::test]
    async fn test_usercreate_empty_password() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Try to create user with empty password
        let result = handle_user_create(
            UserCreateRequest {
                username: "newuser".to_string(),
                password: "".to_string(),
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: vec![],
                group_id: None,
                revokes: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: None,
            },
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Should send error response, not disconnect");
    }

    #[tokio::test]
    async fn test_usercreate_admin_can_grant_any_permission() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Admin can grant ALL permissions even if not explicitly listed
        let result = handle_user_create(
            UserCreateRequest {
                username: "newuser".to_string(),
                password: "password".to_string(),
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: vec![
                    "user_list".to_string(),
                    "user_info".to_string(),
                    "chat_send".to_string(),
                    "chat_receive".to_string(),
                    "user_broadcast".to_string(),
                    "user_create".to_string(),
                    "user_delete".to_string(),
                    "user_edit".to_string(),
                    "user_kick".to_string(),
                    "user_message".to_string(),
                ],
                group_id: None,
                revokes: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: None,
            },
            Some(admin_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should succeed
        assert!(
            result.is_ok(),
            "Admin should be able to grant any permissions"
        );

        // Close writer and read response

        // Parse and verify response
        let response_msg = read_server_message(&mut test_ctx).await;
        match response_msg {
            ServerMessage::UserCreateResponse {
                success,
                error,
                id,
                username,
            } => {
                assert!(success, "Response should indicate success");
                assert!(error.is_none(), "Should have no error message");
                assert!(id.is_some(), "Should return created user ID");
                assert_eq!(username, Some("newuser".to_string()));
            }
            _ => panic!("Expected UserCreateResponse"),
        }

        // Verify user has all permissions
        let created_user = test_ctx
            .db
            .users
            .get_user_by_username("newuser")
            .await
            .unwrap();
        assert!(created_user.is_some(), "User should exist in database");
        let user = created_user.unwrap();

        // Check all permissions were granted
        let all_perms = vec![
            db::Permission::UserList,
            db::Permission::UserInfo,
            db::Permission::ChatSend,
            db::Permission::ChatReceive,
            db::Permission::UserCreate,
            db::Permission::UserDelete,
        ];

        for perm in all_perms {
            let has_perm = test_ctx
                .db
                .users
                .has_permission(user.id, perm)
                .await
                .unwrap();
            assert!(has_perm, "User should have {:?} permission", perm);
        }
    }

    #[tokio::test]
    async fn test_usercreate_with_enabled_false() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a disabled user
        let result = handle_user_create(
            UserCreateRequest {
                username: "disableduser".to_string(),
                password: "password".to_string(),
                is_admin: false,
                is_shared: false,
                enabled: false,
                permissions: vec!["chat_send".to_string()],
                group_id: None,
                revokes: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: None,
            },
            Some(admin_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Should successfully create disabled user");

        // Verify user exists in database and is disabled
        let created_user = test_ctx
            .db
            .users
            .get_user_by_username("disableduser")
            .await
            .unwrap();

        assert!(created_user.is_some(), "User should exist in database");
        let user = created_user.unwrap();
        assert!(!user.enabled, "User should be disabled");
    }

    // ========================================================================
    // Shared Account Tests
    // ========================================================================

    #[tokio::test]
    async fn test_usercreate_shared_account_cannot_be_admin() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Try to create a shared account with is_admin=true
        let result = handle_user_create(
            UserCreateRequest {
                username: "shared_acct".to_string(),
                password: "password".to_string(),
                is_admin: true,
                is_shared: true,
                enabled: true,
                permissions: vec![],
                group_id: None,
                revokes: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: None,
            },
            Some(admin_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Should send error response");

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserCreateResponse {
                success,
                error,
                id,
                username,
            } => {
                assert!(!success, "Should fail to create shared admin");
                assert_eq!(
                    error.unwrap(),
                    err_shared_cannot_be_admin(DEFAULT_TEST_LOCALE)
                );
                assert!(id.is_none());
                assert!(username.is_none());
            }
            _ => panic!("Expected UserCreateResponse"),
        }
    }

    #[tokio::test]
    async fn test_usercreate_shared_account_with_forbidden_permissions() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Try to create a shared account with forbidden permissions
        let result = handle_user_create(
            UserCreateRequest {
                username: "shared_acct".to_string(),
                password: "password".to_string(),
                is_admin: false,
                is_shared: true,
                enabled: true,
                permissions: vec![
                    "chat_send".to_string(),   // allowed
                    "user_create".to_string(), // forbidden
                    "user_kick".to_string(),   // forbidden
                ],
                group_id: None,
                revokes: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: None,
            },
            Some(admin_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Should send error response");

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserCreateResponse {
                success,
                error,
                id,
                username,
            } => {
                assert!(!success, "Should fail with forbidden permissions");
                assert!(error.is_some(), "Should have error message");
                let err_msg = error.unwrap();
                assert!(
                    err_msg.contains("user_create") || err_msg.contains("user_kick"),
                    "Error should mention forbidden permissions: {}",
                    err_msg
                );
                assert!(id.is_none());
                assert!(username.is_none());
            }
            _ => panic!("Expected UserCreateResponse"),
        }
    }

    #[tokio::test]
    async fn test_usercreate_shared_account_with_allowed_permissions() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a shared account with only allowed permissions
        let result = handle_user_create(
            UserCreateRequest {
                username: "shared_acct".to_string(),
                password: "password".to_string(),
                is_admin: false,
                is_shared: true,
                enabled: true,
                permissions: vec![
                    "chat_send".to_string(),
                    "chat_receive".to_string(),
                    "user_list".to_string(),
                    "user_message".to_string(),
                ],
                group_id: None,
                revokes: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: None,
            },
            Some(admin_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Should succeed");

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserCreateResponse {
                success,
                error,
                id,
                username,
            } => {
                assert!(success, "Should successfully create shared account");
                assert!(error.is_none(), "Should have no error");
                assert!(id.is_some(), "Should return created user ID");
                assert_eq!(username, Some("shared_acct".to_string()));
            }
            _ => panic!("Expected UserCreateResponse"),
        }

        // Verify user exists and is marked as shared
        let created_user = test_ctx
            .db
            .users
            .get_user_by_username("shared_acct")
            .await
            .unwrap();
        assert!(created_user.is_some(), "User should exist");
        let user = created_user.unwrap();
        assert!(user.is_shared, "User should be marked as shared");
        assert!(!user.is_admin, "User should not be admin");
    }

    #[tokio::test]
    async fn test_usercreate_shared_account_no_permissions() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a shared account with no permissions (allowed)
        let result = handle_user_create(
            UserCreateRequest {
                username: "shared_acct".to_string(),
                password: "password".to_string(),
                is_admin: false,
                is_shared: true,
                enabled: true,
                permissions: vec![],
                group_id: None,
                revokes: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: None,
            },
            Some(admin_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Should succeed");

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserCreateResponse { success, .. } => {
                assert!(
                    success,
                    "Should successfully create shared account with no permissions"
                );
            }
            _ => panic!("Expected UserCreateResponse"),
        }
    }

    #[tokio::test]
    async fn test_usercreate_with_group() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

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
        let result = handle_user_create(
            UserCreateRequest {
                username: "bob".to_string(),
                password: "password".to_string(),
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: vec![],
                group_id: Some(group.id),
                revokes: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: None,
            },
            Some(admin_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Should succeed creating user with group");

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserCreateResponse {
                success,
                error,
                id,
                username,
            } => {
                assert!(success, "Should successfully create user with group");
                assert!(error.is_none(), "Should have no error");
                assert!(id.is_some(), "Should return created user ID");
                assert_eq!(username, Some("bob".to_string()));
            }
            _ => panic!("Expected UserCreateResponse"),
        }

        // Verify user is in the group
        let created_user = test_ctx
            .db
            .users
            .get_user_by_username("bob")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(created_user.group_id, Some(group.id));
    }

    #[tokio::test]
    async fn test_usercreate_shared_group_mismatch() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a non-shared group
        let regular_group = test_ctx
            .db
            .groups
            .create_group(
                "Regular",
                false,
                &db::Permissions::from(&[db::Permission::ChatSend]),
                1,
            )
            .await
            .unwrap();

        // Create a shared group
        let shared_group = test_ctx
            .db
            .groups
            .create_group(
                "Shared",
                true,
                &db::Permissions::from(&[db::Permission::ChatSend]),
                1,
            )
            .await
            .unwrap();

        // Try: shared user + non-shared group → error
        let result = handle_user_create(
            UserCreateRequest {
                username: "shared_acct".to_string(),
                password: "password".to_string(),
                is_admin: false,
                is_shared: true,
                enabled: true,
                permissions: vec![],
                group_id: Some(regular_group.id),
                revokes: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: None,
            },
            Some(admin_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Should send error response");
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserCreateResponse {
                success,
                error,
                id,
                username,
            } => {
                assert!(!success, "Shared user + non-shared group should fail");
                assert!(error.is_some(), "Should have error message");
                assert!(id.is_none());
                assert!(username.is_none());
            }
            _ => panic!("Expected UserCreateResponse"),
        }

        // Try: non-shared user + shared group → error
        let result = handle_user_create(
            UserCreateRequest {
                username: "regular_user".to_string(),
                password: "password".to_string(),
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: vec![],
                group_id: Some(shared_group.id),
                revokes: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: None,
            },
            Some(admin_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Should send error response");
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserCreateResponse {
                success,
                error,
                id,
                username,
            } => {
                assert!(!success, "Non-shared user + shared group should fail");
                assert!(error.is_some(), "Should have error message");
                assert!(id.is_none());
                assert!(username.is_none());
            }
            _ => panic!("Expected UserCreateResponse"),
        }
    }

    #[tokio::test]
    async fn test_usercreate_non_admin_cannot_assign_group_with_unowned_perms() {
        let mut test_ctx = create_test_context().await;

        // Login as non-admin with UserCreate + ChatSend (but NOT UserKick)
        let editor_session = login_user(
            &mut test_ctx,
            "editor",
            "password",
            &[db::Permission::UserCreate, db::Permission::ChatSend],
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

        // Non-admin tries to create a user assigned to the group
        let result = handle_user_create(
            UserCreateRequest {
                username: "bob".to_string(),
                password: "password".to_string(),
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: vec![],
                group_id: Some(group.id),
                revokes: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: None,
            },
            Some(editor_session),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Should send error, not disconnect");

        // Should be rejected — editor doesn't have UserKick
        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserCreateResponse {
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
            other => panic!("Expected UserCreateResponse (permission denied), got: {other:?}"),
        }

        // Verify bob was NOT created
        let bob = test_ctx.db.users.get_user_by_username("bob").await.unwrap();
        assert!(bob.is_none(), "User should not have been created");
    }

    #[tokio::test]
    async fn test_usercreate_non_admin_cannot_assign_to_higher_weight_group() {
        let mut test_ctx = create_test_context().await;

        // editor's own group: weight 10 with [UserCreate, ChatSend]
        let editor_group = test_ctx
            .db
            .groups
            .create_group(
                "Editors",
                false,
                &db::Permissions::from(&[db::Permission::UserCreate, db::Permission::ChatSend]),
                10,
            )
            .await
            .unwrap();

        let editor_session = login_user(
            &mut test_ctx,
            "editor",
            "password",
            &[db::Permission::UserCreate, db::Permission::ChatSend],
            false,
        )
        .await;

        // Admin moves editor into the editors group so their session weight is 10.
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;
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
        let _ = read_server_message(&mut test_ctx).await;

        // High-weight group with the SAME permissions so the existing
        // permission-subset rule alone would pass.
        let high_group = test_ctx
            .db
            .groups
            .create_group(
                "PowerCreators",
                false,
                &db::Permissions::from(&[db::Permission::UserCreate, db::Permission::ChatSend]),
                50,
            )
            .await
            .unwrap();

        let result = handle_user_create(
            UserCreateRequest {
                username: "bob".to_string(),
                password: "password".to_string(),
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: vec![],
                group_id: Some(high_group.id),
                revokes: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: None,
            },
            Some(editor_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok(), "Should send error, not disconnect");

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserCreateResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(
                    error.unwrap(),
                    err_bandwidth_weight_delegation(DEFAULT_TEST_LOCALE)
                );
            }
            other => panic!("Expected UserCreateResponse, got: {other:?}"),
        }

        let bob = test_ctx.db.users.get_user_by_username("bob").await.unwrap();
        assert!(bob.is_none(), "User should not have been created");
    }

    /// Shared helper: set up an editor with a known resolved bandwidth weight
    /// by creating an Editors group at `weight` and assigning the editor to it.
    async fn setup_editor_at_weight(
        test_ctx: &mut crate::handlers::testing::TestContext,
        weight: u16,
    ) -> u32 {
        let editor_group = test_ctx
            .db
            .groups
            .create_group(
                "Editors",
                false,
                &db::Permissions::from(&[db::Permission::UserCreate, db::Permission::ChatSend]),
                weight,
            )
            .await
            .unwrap();
        let editor_session = login_user(
            test_ctx,
            "editor",
            "password",
            &[db::Permission::UserCreate, db::Permission::ChatSend],
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
    async fn test_usercreate_non_admin_can_set_lower_bandwidth_weight() {
        let mut test_ctx = create_test_context().await;
        let editor_session = setup_editor_at_weight(&mut test_ctx, 25).await;

        // Create bob with override 10 (≤ editor's 25): delegation allows it.
        let result = handle_user_create(
            UserCreateRequest {
                username: "bob".to_string(),
                password: "password".to_string(),
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: vec![],
                group_id: None,
                revokes: None,
                bandwidth_weight: Some(10),
                inherit_bandwidth_weight: None,
            },
            Some(editor_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserCreateResponse { success, error, .. } => {
                assert!(success, "delegation-OK weight should succeed: {:?}", error);
            }
            other => panic!("Expected UserCreateResponse, got: {other:?}"),
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
    async fn test_usercreate_non_admin_cannot_set_higher_bandwidth_weight() {
        let mut test_ctx = create_test_context().await;
        let editor_session = setup_editor_at_weight(&mut test_ctx, 25).await;

        // Try to create bob with override 100 (> editor's 25): delegation rejects.
        let result = handle_user_create(
            UserCreateRequest {
                username: "bob".to_string(),
                password: "password".to_string(),
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: vec![],
                group_id: None,
                revokes: None,
                bandwidth_weight: Some(100),
                inherit_bandwidth_weight: None,
            },
            Some(editor_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserCreateResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(
                    error.unwrap(),
                    err_bandwidth_weight_delegation(DEFAULT_TEST_LOCALE)
                );
            }
            other => panic!("Expected UserCreateResponse, got: {other:?}"),
        }

        let bob = test_ctx.db.users.get_user_by_username("bob").await.unwrap();
        assert!(bob.is_none(), "User should not have been created");
    }

    #[tokio::test]
    async fn test_usercreate_admin_bypasses_delegation() {
        let mut test_ctx = create_test_context().await;
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Admin creates a user with a weight far above the admin default —
        // delegation rule does not apply to admins.
        let result = handle_user_create(
            UserCreateRequest {
                username: "bob".to_string(),
                password: "password".to_string(),
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: vec![],
                group_id: None,
                revokes: None,
                bandwidth_weight: Some(10_000),
                inherit_bandwidth_weight: None,
            },
            Some(admin_session),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserCreateResponse { success, error, .. } => {
                assert!(success, "admin bypass should succeed: {:?}", error);
            }
            other => panic!("Expected UserCreateResponse, got: {other:?}"),
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
    async fn test_usercreate_admin_with_group_rejected() {
        let mut test_ctx = create_test_context().await;

        let admin_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

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

        let result = handle_user_create(
            UserCreateRequest {
                username: "newadmin".to_string(),
                password: "password".to_string(),
                is_admin: true,
                is_shared: false,
                enabled: true,
                permissions: vec![],
                group_id: Some(group.id),
                revokes: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: None,
            },
            Some(admin_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Should send error response");

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserCreateResponse {
                success,
                error,
                id,
                username,
            } => {
                assert!(!success, "Admin + group must be rejected");
                assert_eq!(
                    error.unwrap(),
                    err_admin_cannot_have_group(DEFAULT_TEST_LOCALE)
                );
                assert!(id.is_none());
                assert!(username.is_none());
            }
            other => panic!("Expected UserCreateResponse, got: {other:?}"),
        }
    }
}
