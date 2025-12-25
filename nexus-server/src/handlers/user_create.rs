//! UserCreate message handler

use std::io;

use tokio::io::AsyncWrite;

use nexus_common::is_shared_account_permission;
use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{self, PasswordError, PermissionsError, UsernameError};

#[cfg(test)]
use super::testing::DEFAULT_TEST_LOCALE;
use super::{
    HandlerContext, err_authentication, err_cannot_create_admin, err_database, err_not_logged_in,
    err_password_empty, err_password_too_long, err_permission_denied,
    err_permissions_contains_newlines, err_permissions_empty_permission,
    err_permissions_invalid_characters, err_permissions_permission_too_long,
    err_permissions_too_many, err_shared_cannot_be_admin, err_shared_invalid_permissions,
    err_unknown_permission, err_username_empty, err_username_exists, err_username_invalid,
    err_username_too_long,
};
use crate::db::{Permission, Permissions, hash_password};

/// User creation request parameters
pub struct UserCreateRequest {
    pub username: String,
    pub password: String,
    pub is_admin: bool,
    pub is_shared: bool,
    pub enabled: bool,
    pub permissions: Vec<String>,
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
    } = request;

    // Verify authentication first (before revealing validation errors to unauthenticated users)
    let Some(requesting_session_id) = session_id else {
        eprintln!("UserCreate request from {} without login", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("UserCreate"))
            .await;
    };

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
            username: None,
        };
        return ctx.send_message(&response).await;
    }

    // Validate password
    if let Err(e) = validators::validate_password(&password) {
        let error_msg = match e {
            PasswordError::Empty => err_password_empty(ctx.locale),
            PasswordError::TooLong => {
                err_password_too_long(ctx.locale, validators::MAX_PASSWORD_LENGTH)
            }
        };
        let response = ServerMessage::UserCreateResponse {
            success: false,
            error: Some(error_msg),
            username: None,
        };
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
                .send_error_and_disconnect(&err_authentication(ctx.locale), Some("UserCreate"))
                .await;
        }
    };

    // Check UserCreate permission (uses cached permissions, admin bypass built-in)
    if !requesting_user.has_permission(Permission::UserCreate) {
        eprintln!(
            "UserCreate from {} (user: {}) without permission",
            ctx.peer_addr, requesting_user.username
        );
        return ctx
            .send_error(&err_permission_denied(ctx.locale), Some("UserCreate"))
            .await;
    }

    // Verify admin creation privilege (use is_admin from UserManager)
    if is_admin && !requesting_user.is_admin {
        return ctx
            .send_error_and_disconnect(&err_cannot_create_admin(ctx.locale), Some("UserCreate"))
            .await;
    }

    // Shared accounts cannot be admins
    if is_shared && is_admin {
        let response = ServerMessage::UserCreateResponse {
            success: false,
            error: Some(err_shared_cannot_be_admin(ctx.locale)),
            username: None,
        };
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
                username: None,
            };
            return ctx.send_message(&response).await;
        }
    }

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
                    username: None,
                };
                return ctx.send_message(&response).await;
            }
        };

        // Non-admins can only grant permissions they have
        // Check permission delegation authority (uses cached permissions, admin bypass built-in)
        if !requesting_user.has_permission(perm) {
            eprintln!(
                "UserCreate from {} (user: {}) trying to grant permission they don't have: {}",
                ctx.peer_addr, requesting_user.username, perm_str
            );
            return ctx
                .send_error(&err_permission_denied(ctx.locale), Some("UserCreate"))
                .await;
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
                username: None,
            };
            return ctx.send_message(&response).await;
        }
        Ok(None) => {
            // Username doesn't exist, proceed with creation
        }
        Err(e) => {
            eprintln!("Database error checking username: {}", e);
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some("UserCreate"))
                .await;
        }
    }

    // Hash password for secure storage
    let password_hash = match hash_password(&password) {
        Ok(hash) => hash,
        Err(e) => {
            eprintln!("Password hashing error: {}", e);
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some("UserCreate"))
                .await;
        }
    };

    // Create user in database
    match ctx
        .db
        .users
        .create_user(
            &username,
            &password_hash,
            is_admin,
            is_shared,
            enabled,
            &perms,
        )
        .await
    {
        Ok(_user) => {
            // Success
            let response = ServerMessage::UserCreateResponse {
                success: true,
                error: None,
                username: Some(username),
            };
            ctx.send_message(&response).await
        }
        Err(e) => {
            eprintln!("Database error creating user: {}", e);
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some("UserCreate"))
                .await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::handlers::testing::{create_test_context, login_user, read_server_message};
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
            },
            Some(admin_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should succeed
        assert!(result.is_ok(), "Admin should be able to create users");

        // Close writer and read response

        // Parse and verify response
        let response_msg = read_server_message(&mut test_ctx.client).await;
        match response_msg {
            ServerMessage::UserCreateResponse {
                success,
                error,
                username,
            } => {
                assert!(success, "Response should indicate success");
                assert!(error.is_none(), "Should have no error message");
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
        let hashed = db::hash_password(password).unwrap();
        let admin = test_ctx
            .db
            .users
            .create_user("admin", &hashed, true, false, true, &db::Permissions::new())
            .await
            .unwrap();

        // Create existing user
        let _existing = test_ctx
            .db
            .users
            .create_user(
                "existing",
                &hashed,
                false,
                false,
                true,
                &db::Permissions::new(),
            )
            .await
            .unwrap();

        // Add admin to UserManager
        let admin_id = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                db_user_id: admin.id,
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
        let response_msg = read_server_message(&mut test_ctx.client).await;
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
            },
            Some(admin_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should succeed
        assert!(result.is_ok(), "Admin should be able to create admin users");

        // Close writer and read response

        // Parse and verify response
        let response_msg = read_server_message(&mut test_ctx.client).await;
        match response_msg {
            ServerMessage::UserCreateResponse {
                success,
                error,
                username,
            } => {
                assert!(success, "Response should indicate success");
                assert!(error.is_none(), "Should have no error message");
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
        let response_msg = read_server_message(&mut test_ctx.client).await;
        match response_msg {
            ServerMessage::UserCreateResponse {
                success,
                error,
                username,
            } => {
                assert!(success, "Response should indicate success");
                assert!(error.is_none(), "Should have no error message");
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
        let response_msg = read_server_message(&mut test_ctx.client).await;
        match response_msg {
            ServerMessage::UserCreateResponse {
                success,
                error,
                username,
            } => {
                assert!(success, "Response should indicate success");
                assert!(error.is_none(), "Should have no error message");
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
        let hashed = db::hash_password(password).unwrap();
        let _admin = test_ctx
            .db
            .users
            .create_user("admin", &hashed, true, false, true, &db::Permissions::new())
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
            .create_user("creator", &hashed, false, false, true, &perms)
            .await
            .unwrap();

        // Add creator to UserManager
        let creator_id = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                db_user_id: creator.id,
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
            },
            Some(creator_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should fail with disconnect
        assert!(
            result.is_err(),
            "Non-admin should not be able to create admin users"
        );
    }

    #[tokio::test]
    async fn test_usercreate_cannot_grant_permissions_user_doesnt_have() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let password = "password";
        let hashed = db::hash_password(password).unwrap();
        let _admin = test_ctx
            .db
            .users
            .create_user("admin", &hashed, true, false, true, &db::Permissions::new())
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
            .create_user("creator", &hashed, false, false, true, &perms)
            .await
            .unwrap();

        // Add creator to UserManager
        let creator_id = test_ctx
            .user_manager
            .add_user(NewSessionParams {
                session_id: 0,
                db_user_id: creator.id,
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
        let response_msg = read_server_message(&mut test_ctx.client).await;
        match response_msg {
            ServerMessage::UserCreateResponse {
                success,
                error,
                username,
            } => {
                assert!(success, "Response should indicate success");
                assert!(error.is_none(), "Should have no error message");
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
            },
            Some(admin_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Should send error response");

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserCreateResponse {
                success,
                error,
                username,
            } => {
                assert!(!success, "Should fail to create shared admin");
                assert!(error.is_some(), "Should have error message");
                assert!(
                    error.unwrap().contains("cannot be admin"),
                    "Error should mention shared accounts cannot be admins"
                );
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
            },
            Some(admin_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Should send error response");

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserCreateResponse {
                success,
                error,
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
            },
            Some(admin_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Should succeed");

        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserCreateResponse {
                success,
                error,
                username,
            } => {
                assert!(success, "Should successfully create shared account");
                assert!(error.is_none(), "Should have no error");
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
            },
            Some(admin_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Should succeed");

        let response = read_server_message(&mut test_ctx.client).await;
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
}
