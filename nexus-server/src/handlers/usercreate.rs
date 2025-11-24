//! UserCreate message handler

use super::{
    ERR_CANNOT_CREATE_ADMIN, ERR_DATABASE, ERR_NOT_LOGGED_IN, ERR_PERMISSION_DENIED,
    ERR_USERNAME_EXISTS, HandlerContext,
};
use crate::db::{hash_password, Permission, Permissions};
use nexus_common::protocol::ServerMessage;
use std::io;

/// Error message for empty username
const ERR_EMPTY_USERNAME: &str = "Username cannot be empty";

/// Error message for empty password
const ERR_EMPTY_PASSWORD: &str = "Password cannot be empty";

/// Handle a user creation request from the client
pub async fn handle_usercreate(
    username: String,
    password: String,
    is_admin: bool,
    permissions: Vec<String>,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_>,
) -> io::Result<()> {
    // Validate input fields
    if username.trim().is_empty() {
        eprintln!("UserCreate from {} with empty username", ctx.peer_addr);
        let error_msg = ServerMessage::UserCreateResponse {
            success: false,
            error: Some(ERR_EMPTY_USERNAME.to_string()),
        };
        ctx.send_message(&error_msg).await?;
        return Ok(());
    }

    if password.trim().is_empty() {
        eprintln!("UserCreate from {} with empty password", ctx.peer_addr);
        let error_msg = ServerMessage::UserCreateResponse {
            success: false,
            error: Some(ERR_EMPTY_PASSWORD.to_string()),
        };
        ctx.send_message(&error_msg).await?;
        return Ok(());
    }

    // Verify authentication
    let requesting_session_id = match session_id {
        Some(id) => id,
        None => {
            eprintln!("UserCreate request from {} without login", ctx.peer_addr);
            return ctx
                .send_error_and_disconnect(ERR_NOT_LOGGED_IN, Some("UserCreate"))
                .await;
        }
    };

    // Get requesting user from session
    let requesting_user = match ctx.user_manager.get_user_by_session_id(requesting_session_id).await {
        Some(u) => u,
        None => {
            eprintln!(
                "UserCreate request from unknown user {}",
                ctx.peer_addr
            );
            return ctx
                .send_error_and_disconnect(ERR_DATABASE, Some("UserCreate"))
                .await;
        }
    };

    // Check UserCreate permission
    let has_permission = match ctx
        .user_db
        .has_permission(requesting_user.db_user_id, Permission::UserCreate)
        .await
    {
        Ok(has) => has,
        Err(e) => {
            eprintln!("UserCreate permission check error: {}", e);
            return ctx
                .send_error_and_disconnect(ERR_DATABASE, Some("UserCreate"))
                .await;
        }
    };

    if !has_permission {
        eprintln!(
            "UserCreate from {} (user: {}) without permission",
            ctx.peer_addr, requesting_user.username
        );
        return ctx
            .send_error(ERR_PERMISSION_DENIED, Some("UserCreate"))
            .await;
    }

    // Verify admin creation privilege
    if is_admin {
        let requesting_account = match ctx
            .user_db
            .get_user_by_username(&requesting_user.username)
            .await
        {
            Ok(Some(acc)) => acc,
            _ => {
                return ctx
                    .send_error_and_disconnect(ERR_DATABASE, Some("UserCreate"))
                    .await;
            }
        };

        if !requesting_account.is_admin {
            eprintln!(
                "UserCreate request from {} to create admin without being admin",
                ctx.peer_addr
            );
            return ctx
                .send_error_and_disconnect(ERR_CANNOT_CREATE_ADMIN, Some("UserCreate"))
                .await;
        }
    }

    // Fetch requesting user's account for permission validation
    let requesting_account = match ctx
        .user_db
        .get_user_by_username(&requesting_user.username)
        .await
    {
        Ok(Some(acc)) => acc,
        _ => {
            return ctx
                .send_error_and_disconnect(ERR_DATABASE, Some("UserCreate"))
                .await;
        }
    };

    // Parse and validate requested permissions
    let mut perms = Permissions::new();
    for perm_str in permissions {
        if let Some(perm) = Permission::from_str(&perm_str) {
            // Non-admins can only grant permissions they have
            if !requesting_account.is_admin {
                let has_perm = match ctx
                    .user_db
                    .has_permission(requesting_user.db_user_id, perm)
                    .await
                {
                    Ok(has) => has,
                    Err(e) => {
                        eprintln!("Permission check error: {}", e);
                        return ctx
                            .send_error_and_disconnect(ERR_DATABASE, Some("UserCreate"))
                            .await;
                    }
                };

                if !has_perm {
                    eprintln!(
                        "UserCreate from {} (user: {}) trying to grant permission they don't have: {}",
                        ctx.peer_addr, requesting_user.username, perm_str
                    );
                    return ctx
                        .send_error(ERR_PERMISSION_DENIED, Some("UserCreate"))
                        .await;
                }
            }

            perms.permissions.insert(perm);
        } else {
            eprintln!("Warning: unknown permission '{}'", perm_str);
            // We could return an error here, but for now just skip invalid permissions
        }
    }

    // Check for duplicate username
    match ctx.user_db.get_user_by_username(&username).await {
        Ok(Some(_)) => {
            // Username already exists
            let response = ServerMessage::UserCreateResponse {
                success: false,
                error: Some(ERR_USERNAME_EXISTS.to_string()),
            };
            return ctx.send_message(&response).await;
        }
        Ok(None) => {
            // Username doesn't exist, proceed with creation
        }
        Err(e) => {
            eprintln!("Database error checking username: {}", e);
            return ctx
                .send_error_and_disconnect(ERR_DATABASE, Some("UserCreate"))
                .await;
        }
    }

    // Hash password for secure storage
    let password_hash = match hash_password(&password) {
        Ok(hash) => hash,
        Err(e) => {
            eprintln!("Password hashing error: {}", e);
            return ctx
                .send_error_and_disconnect(ERR_DATABASE, Some("UserCreate"))
                .await;
        }
    };

    // Create user in database
    match ctx
        .user_db
        .create_user(&username, &password_hash, is_admin, &perms)
        .await
    {
        Ok(_user) => {
            // Success
            let response = ServerMessage::UserCreateResponse {
                success: true,
                error: None,
            };
            ctx.send_message(&response).await
        }
        Err(e) => {
            eprintln!("Database error creating user: {}", e);
            return ctx
                .send_error_and_disconnect(ERR_DATABASE, Some("UserCreate"))
                .await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::handlers::testing::{create_test_context, login_user};
    use tokio::io::AsyncReadExt;

    #[tokio::test]
    async fn test_usercreate_requires_login() {
        let mut test_ctx = create_test_context().await;

        // Try to create user without being logged in
        let result = handle_usercreate(
            "newuser".to_string(),
            "password".to_string(),
            false,
            vec![],
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
        let user_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[],
            false,
        )
        .await;

        // Try to create user without permission
        let result = handle_usercreate(
            "newuser".to_string(),
            "password".to_string(),
            false,
            vec![],
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
        let admin_id = login_user(
            &mut test_ctx,
            "admin",
            "password",
            &[],
            true,
        )
        .await;

        // Create a new user
        let result = handle_usercreate(
            "newuser".to_string(),
            "newpassword".to_string(),
            false,
            vec![],
            Some(admin_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should succeed
        assert!(result.is_ok(), "Admin should be able to create users");

        // Close writer and read response
        drop(test_ctx.write_half);
        let mut response = String::new();
        test_ctx.client.read_to_string(&mut response).await.unwrap();

        // Parse and verify response
        let response_msg: ServerMessage = serde_json::from_str(&response.trim()).unwrap();
        match response_msg {
            ServerMessage::UserCreateResponse { success, error } => {
                assert!(success, "Response should indicate success");
                assert!(error.is_none(), "Should have no error message");
            }
            _ => panic!("Expected UserCreateResponse"),
        }

        // Verify user exists in database
        let created_user = test_ctx
            .user_db
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
            .user_db
            .create_user("admin", &hashed, true, &db::Permissions::new())
            .await
            .unwrap();

        // Create existing user
        let _existing = test_ctx
            .user_db
            .create_user("existing", &hashed, false, &db::Permissions::new())
            .await
            .unwrap();

        // Add admin to UserManager
        let admin_id = test_ctx
            .user_manager
            .add_user(
                admin.id,
                "admin".to_string(),
                test_ctx.peer_addr,
                admin.created_at,
                test_ctx.tx.clone(),
                vec![],
            )
            .await;

        // Try to create user with duplicate username
        let result = handle_usercreate(
            "existing".to_string(),
            "newpassword".to_string(),
            false,
            vec![],
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
        drop(test_ctx.write_half);
        let mut response = String::new();
        test_ctx.client.read_to_string(&mut response).await.unwrap();

        // Parse and verify response
        let response_msg: ServerMessage = serde_json::from_str(&response.trim()).unwrap();
        match response_msg {
            ServerMessage::UserCreateResponse { success, error } => {
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
        let admin_id = login_user(
            &mut test_ctx,
            "admin",
            "password",
            &[],
            true,
        )
        .await;

        // Create a new admin user
        let result = handle_usercreate(
            "newadmin".to_string(),
            "newpassword".to_string(),
            true, // is_admin = true
            vec![],
            Some(admin_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should succeed
        assert!(result.is_ok(), "Admin should be able to create admin users");

        // Close writer and read response
        drop(test_ctx.write_half);
        let mut response = String::new();
        test_ctx.client.read_to_string(&mut response).await.unwrap();

        // Parse and verify response
        let response_msg: ServerMessage = serde_json::from_str(&response.trim()).unwrap();
        match response_msg {
            ServerMessage::UserCreateResponse { success, error } => {
                assert!(success, "Response should indicate success");
                assert!(error.is_none(), "Should have no error message");
            }
            _ => panic!("Expected UserCreateResponse"),
        }

        // Verify user exists and is admin
        let created_user = test_ctx
            .user_db
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
        let result = handle_usercreate(
            "newuser".to_string(),
            "password".to_string(),
            false,
            vec!["user_list".to_string()],
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
        drop(test_ctx.write_half);
        let mut response = String::new();
        test_ctx.client.read_to_string(&mut response).await.unwrap();

        // Parse and verify response
        let response_msg: ServerMessage = serde_json::from_str(&response.trim()).unwrap();
        match response_msg {
            ServerMessage::UserCreateResponse { success, error } => {
                assert!(success, "Response should indicate success");
                assert!(error.is_none(), "Should have no error message");
            }
            _ => panic!("Expected UserCreateResponse"),
        }

        // Verify user exists
        let created_user = test_ctx
            .user_db
            .get_user_by_username("newuser")
            .await
            .unwrap();
        assert!(created_user.is_some(), "User should exist in database");
        
        // Verify permissions were granted
        let user = created_user.unwrap();
        let has_user_list = test_ctx
            .user_db
            .has_permission(user.id, db::Permission::UserList)
            .await
            .unwrap();
        assert!(has_user_list, "User should have UserList permission");
    }

    #[tokio::test]
    async fn test_usercreate_grants_specified_permissions() {
        let mut test_ctx = create_test_context().await;

        // Create an admin user
        let admin_id = login_user(
            &mut test_ctx,
            "admin",
            "password",
            &[],
            true,
        )
        .await;

        // Create a new user with specific permissions
        let result = handle_usercreate(
            "newuser".to_string(),
            "password".to_string(),
            false,
            vec![
                "user_list".to_string(),
                "user_info".to_string(),
                "chat_send".to_string(),
            ],
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
        drop(test_ctx.write_half);
        let mut response = String::new();
        test_ctx.client.read_to_string(&mut response).await.unwrap();

        // Parse and verify response
        let response_msg: ServerMessage = serde_json::from_str(&response.trim()).unwrap();
        match response_msg {
            ServerMessage::UserCreateResponse { success, error } => {
                assert!(success, "Response should indicate success");
                assert!(error.is_none(), "Should have no error message");
            }
            _ => panic!("Expected UserCreateResponse"),
        }

        // Verify user exists and has the specified permissions
        let created_user = test_ctx
            .user_db
            .get_user_by_username("newuser")
            .await
            .unwrap();
        assert!(created_user.is_some(), "User should exist in database");
        let user = created_user.unwrap();

        // Check granted permissions
        let has_user_list = test_ctx
            .user_db
            .has_permission(user.id, db::Permission::UserList)
            .await
            .unwrap();
        let has_user_info = test_ctx
            .user_db
            .has_permission(user.id, db::Permission::UserInfo)
            .await
            .unwrap();
        let has_chat_send = test_ctx
            .user_db
            .has_permission(user.id, db::Permission::ChatSend)
            .await
            .unwrap();

        assert!(has_user_list, "User should have UserList permission");
        assert!(has_user_info, "User should have UserInfo permission");
        assert!(has_chat_send, "User should have ChatSend permission");

        // Check permissions NOT granted
        let has_chat_receive = test_ctx
            .user_db
            .has_permission(user.id, db::Permission::ChatReceive)
            .await
            .unwrap();
        let has_user_delete = test_ctx
            .user_db
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
            .user_db
            .create_user("admin", &hashed, true, &db::Permissions::new())
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
            .user_db
            .create_user("creator", &hashed, false, &perms)
            .await
            .unwrap();

        // Add creator to UserManager
        let creator_id = test_ctx
            .user_manager
            .add_user(
                creator.id,
                "creator".to_string(),
                test_ctx.peer_addr,
                creator.created_at,
                test_ctx.tx.clone(),
                vec![],
            )
            .await;

        // Try to create an admin user as non-admin
        let result = handle_usercreate(
            "newadmin".to_string(),
            "password".to_string(),
            true, // is_admin = true
            vec![],
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
            .user_db
            .create_user("admin", &hashed, true, &db::Permissions::new())
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
            .user_db
            .create_user("creator", &hashed, false, &perms)
            .await
            .unwrap();

        // Add creator to UserManager
        let creator_id = test_ctx
            .user_manager
            .add_user(
                creator.id,
                "creator".to_string(),
                test_ctx.peer_addr,
                creator.created_at,
                test_ctx.tx.clone(),
                vec![],
            )
            .await;

        // Try to create a user with UserDelete permission (which creator doesn't have)
        let result = handle_usercreate(
            "newuser".to_string(),
            "password".to_string(),
            false,
            vec![
                "chat_send".to_string(),   // creator has this - OK
                "user_delete".to_string(), // creator doesn't have this - FAIL
            ],
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
        let session_id = login_user(
            &mut test_ctx,
            "admin",
            "password",
            &[],
            true,
        )
        .await;

        // Try to create user with empty username
        let result = handle_usercreate(
            "".to_string(),
            "password123".to_string(),
            false,
            vec![],
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
        let session_id = login_user(
            &mut test_ctx,
            "admin",
            "password",
            &[],
            true,
        )
        .await;

        // Try to create user with empty password
        let result = handle_usercreate(
            "newuser".to_string(),
            "".to_string(),
            false,
            vec![],
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
        let admin_id = login_user(
            &mut test_ctx,
            "admin",
            "password",
            &[],
            true,
        )
        .await;

        // Admin can grant ALL permissions even if not explicitly listed
        let result = handle_usercreate(
            "newuser".to_string(),
            "password".to_string(),
            false,
            vec![
                "user_list".to_string(),
                "user_info".to_string(),
                "chat_send".to_string(),
                "chat_receive".to_string(),
                "user_create".to_string(),
                "user_delete".to_string(),
            ],
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
        drop(test_ctx.write_half);
        let mut response = String::new();
        test_ctx.client.read_to_string(&mut response).await.unwrap();

        // Parse and verify response
        let response_msg: ServerMessage = serde_json::from_str(&response.trim()).unwrap();
        match response_msg {
            ServerMessage::UserCreateResponse { success, error } => {
                assert!(success, "Response should indicate success");
                assert!(error.is_none(), "Should have no error message");
            }
            _ => panic!("Expected UserCreateResponse"),
        }

        // Verify user has all permissions
        let created_user = test_ctx
            .user_db
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
                .user_db
                .has_permission(user.id, perm)
                .await
                .unwrap();
            assert!(
                has_perm,
                "User should have {:?} permission",
                perm
            );
        }
    }
}