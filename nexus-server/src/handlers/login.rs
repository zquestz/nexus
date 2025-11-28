//! Login message handler

use super::{
    ERR_ACCOUNT_DISABLED, ERR_ALREADY_LOGGED_IN, ERR_AUTHENTICATION, ERR_DATABASE,
    ERR_FAILED_TO_CREATE_USER, ERR_HANDSHAKE_REQUIRED, ERR_INVALID_CREDENTIALS,
};
use super::{HandlerContext, current_timestamp};
use crate::db::{self, Permission};
use crate::users::user::NewUserParams;
use nexus_common::protocol::{ServerInfo, ServerMessage, UserInfo};
use std::io;

/// Handle a login request from the client
pub async fn handle_login(
    username: String,
    password: String,
    features: Vec<String>,
    locale: String,
    handshake_complete: bool,
    session_id: &mut Option<u32>,
    ctx: &mut HandlerContext<'_>,
) -> io::Result<()> {
    // Verify handshake completed
    if !handshake_complete {
        eprintln!("Login attempt from {} without handshake", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(ERR_HANDSHAKE_REQUIRED, Some("Login"))
            .await;
    }

    // Check for duplicate login on same connection
    if session_id.is_some() {
        eprintln!("Duplicate login attempt from {}", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(ERR_ALREADY_LOGGED_IN, Some("Login"))
            .await;
    }

    // Look up user account in database
    let account = match ctx.db.users.get_user_by_username(&username).await {
        Ok(acc) => acc,
        Err(e) => {
            eprintln!("Database error looking up user {}: {}", username, e);
            return ctx
                .send_error_and_disconnect(ERR_DATABASE, Some("Login"))
                .await;
        }
    };

    // Authenticate user or create first admin
    let authenticated_account = if let Some(account) = account {
        // User exists - verify password
        match db::verify_password(&password, &account.hashed_password) {
            Ok(true) => {
                // Password is correct - check if account is enabled
                if !account.enabled {
                    eprintln!(
                        "Login attempt for disabled account {} from {}",
                        username, ctx.peer_addr
                    );
                    return ctx
                        .send_error_and_disconnect(ERR_ACCOUNT_DISABLED, Some("Login"))
                        .await;
                }
                account
            }
            Ok(false) => {
                eprintln!(
                    "Invalid password for user {} from {}",
                    username, ctx.peer_addr
                );
                return ctx
                    .send_error_and_disconnect(ERR_INVALID_CREDENTIALS, Some("Login"))
                    .await;
            }
            Err(e) => {
                eprintln!("Password verification error for {}: {}", username, e);
                return ctx
                    .send_error_and_disconnect(ERR_AUTHENTICATION, Some("Login"))
                    .await;
            }
        }
    } else {
        // User doesn't exist - try to create as first user (atomic operation)
        let hashed_password = match db::hash_password(&password) {
            Ok(hash) => hash,
            Err(e) => {
                eprintln!("Failed to hash password for {}: {}", username, e);
                return ctx
                    .send_error_and_disconnect(ERR_FAILED_TO_CREATE_USER, Some("Login"))
                    .await;
            }
        };

        // Try to create as first admin - the database method will handle atomicity
        match ctx
            .db
            .users
            .create_first_user_if_none_exist(&username, &hashed_password)
            .await
        {
            Ok(Some(account)) => {
                println!(
                    "Created first user (admin): '{}' from {}",
                    username, ctx.peer_addr
                );
                account
            }
            Ok(None) => {
                // Another connection created the first user already
                eprintln!("User {} does not exist and not first user", username);
                return ctx
                    .send_error_and_disconnect(ERR_INVALID_CREDENTIALS, Some("Login"))
                    .await;
            }
            Err(e) => {
                eprintln!("Failed to create first user {}: {}", username, e);
                return ctx
                    .send_error_and_disconnect(ERR_FAILED_TO_CREATE_USER, Some("Login"))
                    .await;
            }
        }
    };

    // Create session in UserManager
    // Note: Features are client preferences (what they want to subscribe to)
    // Permissions are checked when executing commands, not at login
    let id = ctx
        .user_manager
        .add_user(NewUserParams {
            session_id: 0, // Will be assigned by add_user
            db_user_id: authenticated_account.id,
            username: authenticated_account.username.clone(),
            address: ctx.peer_addr,
            created_at: authenticated_account.created_at,
            tx: ctx.tx.clone(),
            features,
            locale: locale.clone(),
        })
        .await;
    *session_id = Some(id);

    // Fetch user permissions for LoginResponse
    let user_permissions = if authenticated_account.is_admin {
        // Admins get all permissions automatically - return empty list
        // Client checks is_admin flag to know they have all permissions
        vec![]
    } else {
        // Fetch permissions from database for non-admin users
        match ctx
            .db
            .users
            .get_user_permissions(authenticated_account.id)
            .await
        {
            Ok(perms) => perms
                .to_vec()
                .iter()
                .map(|p| p.as_str().to_string())
                .collect(),
            Err(e) => {
                eprintln!(
                    "Error fetching permissions for {}: {}",
                    authenticated_account.username, e
                );
                vec![]
            }
        }
    };

    // Fetch server info if user has ChatTopic permission
    let server_info = if authenticated_account.is_admin
        || ctx
            .db
            .users
            .has_permission(authenticated_account.id, Permission::ChatTopic)
            .await
            .unwrap_or(false)
    {
        // Fetch chat topic from database
        match ctx.db.config.get_topic().await {
            Ok(chat_topic) => Some(ServerInfo { chat_topic }),
            Err(e) => {
                eprintln!(
                    "Error fetching chat topic for {}: {}",
                    authenticated_account.username, e
                );
                None
            }
        }
    } else {
        None
    };

    let response = ServerMessage::LoginResponse {
        success: true,
        session_id: Some(id.to_string()),
        is_admin: Some(authenticated_account.is_admin),
        permissions: Some(user_permissions),
        server_info,
        locale: Some(locale.clone()),
        error: None,
    };
    ctx.send_message(&response).await?;

    if ctx.debug {
        println!("User '{}' logged in from {}", username, ctx.peer_addr);
    }

    // Notify other users about new connection
    let user_info = UserInfo {
        username,
        login_time: current_timestamp(),
        is_admin: authenticated_account.is_admin,
        session_ids: vec![id],
        locale: locale.clone(),
    };
    ctx.user_manager
        .broadcast_user_event(
            ServerMessage::UserConnected { user: user_info },
            &ctx.db.users,
            Some(id), // Don't send to the connecting user
        )
        .await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::testing::{DEFAULT_TEST_LOCALE, create_test_context};
    use tokio::io::AsyncReadExt;

    #[tokio::test]
    async fn test_login_requires_handshake() {
        let mut test_ctx = create_test_context().await;
        let mut session_id = None;
        let handshake_complete = false; // Not completed

        // Try to login without handshake
        let result = handle_login(
            "alice".to_string(),
            "password".to_string(),
            vec![],
            DEFAULT_TEST_LOCALE.to_string(),
            handshake_complete,
            &mut session_id,
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should fail
        assert!(result.is_err(), "Login should fail without handshake");
        assert!(session_id.is_none(), "Session ID should remain None");
    }

    #[tokio::test]
    async fn test_first_login_creates_admin() {
        let mut test_ctx = create_test_context().await;
        let mut session_id = None;
        let handshake_complete = true;

        // First user login
        let result = handle_login(
            "alice".to_string(),
            "password123".to_string(),
            vec!["chat".to_string()],
            DEFAULT_TEST_LOCALE.to_string(),
            handshake_complete,
            &mut session_id,
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should succeed
        assert!(result.is_ok(), "First login should succeed");
        assert!(session_id.is_some(), "Session ID should be set");

        // Read response
        drop(test_ctx.write_half);
        let mut response = String::new();
        test_ctx.client.read_to_string(&mut response).await.unwrap();

        let response_msg: ServerMessage = serde_json::from_str(response.trim()).unwrap();

        // Verify successful login response with admin flag and empty permissions
        match response_msg {
            ServerMessage::LoginResponse {
                success,
                session_id,
                is_admin,
                permissions,
                error,
                ..
            } => {
                assert!(success, "Login should indicate success");
                assert!(session_id.is_some(), "Should return session ID");
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

        // Verify user was created as admin in database
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

        // Pre-create a user
        // Create a user account with permissions
        let password = "mypassword";
        let hashed = db::hash_password(password).unwrap();
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
            .create_user("bob", &hashed, false, true, &perms)
            .await
            .unwrap();

        let mut session_id = None;
        let handshake_complete = true;

        // Login with correct password
        let result = handle_login(
            "bob".to_string(),
            password.to_string(),
            vec![],
            DEFAULT_TEST_LOCALE.to_string(),
            handshake_complete,
            &mut session_id,
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should succeed
        assert!(result.is_ok(), "Login with correct password should succeed");
        assert!(session_id.is_some(), "Session ID should be set");

        // Read response
        drop(test_ctx.write_half);
        let mut response = String::new();
        test_ctx.client.read_to_string(&mut response).await.unwrap();

        let response_msg: ServerMessage = serde_json::from_str(response.trim()).unwrap();

        // Verify successful login response with is_admin and permissions
        match response_msg {
            ServerMessage::LoginResponse {
                success,
                session_id,
                is_admin,
                permissions,
                error,
                ..
            } => {
                assert!(success, "Login should succeed");
                assert!(session_id.is_some(), "Should return session ID");
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

        // Pre-create a user
        let password = "correctpassword";
        let hashed = db::hash_password(password).unwrap();
        test_ctx
            .db
            .users
            .create_user("bob", &hashed, false, true, &db::Permissions::new())
            .await
            .unwrap();

        let mut session_id = None;
        let handshake_complete = true;

        // Login with wrong password
        let result = handle_login(
            "bob".to_string(),
            "wrongpassword".to_string(),
            vec![],
            DEFAULT_TEST_LOCALE.to_string(),
            handshake_complete,
            &mut session_id,
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should fail
        assert!(result.is_err(), "Login with wrong password should fail");
        assert!(session_id.is_none(), "Session ID should remain None");
    }

    #[tokio::test]
    async fn test_login_nonexistent_user() {
        let mut test_ctx = create_test_context().await;

        // Create a user first (so we're not the first user who would auto-register)
        let password = "password";
        let hashed = db::hash_password(password).unwrap();
        test_ctx
            .db
            .users
            .create_user("existing", &hashed, true, true, &db::Permissions::new())
            .await
            .unwrap();

        let mut session_id = None;
        let handshake_complete = true;

        // Try to login as non-existent user
        let result = handle_login(
            "nonexistent".to_string(),
            "password".to_string(),
            vec![],
            DEFAULT_TEST_LOCALE.to_string(),
            handshake_complete,
            &mut session_id,
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should fail
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
        let admin_hashed = db::hash_password(admin_password).unwrap();
        let _admin = test_ctx
            .db
            .users
            .create_user("admin", &admin_hashed, true, true, &db::Permissions::new())
            .await
            .unwrap();

        // Create a non-admin user with specific permissions
        let password = "password";
        let hashed = db::hash_password(password).unwrap();
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
            .create_user("alice", &hashed, false, true, &perms)
            .await
            .unwrap();

        let handshake_complete = true;
        let mut session_id = None;

        // Attempt login
        let result = handle_login(
            "alice".to_string(),
            password.to_string(),
            vec!["chat".to_string()],
            DEFAULT_TEST_LOCALE.to_string(),
            handshake_complete,
            &mut session_id,
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Login should succeed");

        // Read response
        drop(test_ctx.write_half);
        let mut response = String::new();
        test_ctx.client.read_to_string(&mut response).await.unwrap();

        let response_msg: ServerMessage = serde_json::from_str(response.trim()).unwrap();

        // Verify response includes correct permissions
        match response_msg {
            ServerMessage::LoginResponse {
                success,
                session_id,
                is_admin,
                permissions,
                error,
                ..
            } => {
                assert!(success, "Login should succeed");
                assert!(session_id.is_some(), "Should return session ID");
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

        // Create user first
        let password = "password";
        let hashed = db::hash_password(password).unwrap();
        test_ctx
            .db
            .users
            .create_user("alice", &hashed, false, true, &db::Permissions::new())
            .await
            .unwrap();

        let mut session_id = None;
        let handshake_complete = true;

        // First login
        let result1 = handle_login(
            "alice".to_string(),
            "password".to_string(),
            vec![],
            DEFAULT_TEST_LOCALE.to_string(),
            handshake_complete,
            &mut session_id,
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result1.is_ok(), "First login should succeed");
        assert!(session_id.is_some(), "Session ID should be set");

        // Second login on same connection (should fail)
        let result2 = handle_login(
            "alice".to_string(),
            "password".to_string(),
            vec![],
            DEFAULT_TEST_LOCALE.to_string(),
            handshake_complete,
            &mut session_id,
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(
            result2.is_err(),
            "Second login on same connection should fail"
        );
    }

    #[tokio::test]
    async fn test_login_includes_server_info_with_chat_topic() {
        let mut test_ctx = create_test_context().await;

        // Create user with ChatTopic permission
        let password = "password";
        let hashed = db::hash_password(password).unwrap();
        let mut perms = db::Permissions::new();
        perms.permissions.insert(Permission::ChatTopic);
        test_ctx
            .db
            .users
            .create_user("alice", &hashed, false, true, &perms)
            .await
            .unwrap();

        // Set a topic
        test_ctx
            .db
            .config
            .set_topic("Test server topic")
            .await
            .unwrap();

        let mut session_id = None;
        let handshake_complete = true;

        let result = handle_login(
            "alice".to_string(),
            password.to_string(),
            vec![],
            DEFAULT_TEST_LOCALE.to_string(),
            handshake_complete,
            &mut session_id,
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Login should succeed");

        // Read response
        drop(test_ctx.write_half);
        let mut response = String::new();
        test_ctx.client.read_to_string(&mut response).await.unwrap();

        let response_msg: ServerMessage = serde_json::from_str(response.trim()).unwrap();

        // Verify LoginResponse includes server_info with chat_topic
        match response_msg {
            ServerMessage::LoginResponse {
                success,
                server_info,
                ..
            } => {
                assert!(success, "Login should succeed");
                assert!(server_info.is_some(), "Should include server_info");
                let info = server_info.unwrap();
                assert_eq!(
                    info.chat_topic, "Test server topic",
                    "Should include chat topic"
                );
            }
            _ => panic!("Expected LoginResponse"),
        }
    }

    #[tokio::test]
    async fn test_login_excludes_server_info_without_permission() {
        let mut test_ctx = create_test_context().await;

        // Create user without ChatTopic permission
        let password = "password";
        let hashed = db::hash_password(password).unwrap();
        test_ctx
            .db
            .users
            .create_user("alice", &hashed, false, true, &db::Permissions::new())
            .await
            .unwrap();

        // Set a topic (user shouldn't see it)
        test_ctx.db.config.set_topic("Secret topic").await.unwrap();

        let mut session_id = None;
        let handshake_complete = true;

        let result = handle_login(
            "alice".to_string(),
            password.to_string(),
            vec![],
            DEFAULT_TEST_LOCALE.to_string(),
            handshake_complete,
            &mut session_id,
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Login should succeed");

        // Read response
        drop(test_ctx.write_half);
        let mut response = String::new();
        test_ctx.client.read_to_string(&mut response).await.unwrap();

        let response_msg: ServerMessage = serde_json::from_str(response.trim()).unwrap();

        // Verify LoginResponse excludes server_info
        match response_msg {
            ServerMessage::LoginResponse {
                success,
                server_info,
                ..
            } => {
                assert!(success, "Login should succeed");
                assert!(
                    server_info.is_none(),
                    "Should NOT include server_info without permission"
                );
            }
            _ => panic!("Expected LoginResponse"),
        }
    }

    #[tokio::test]
    async fn test_login_admin_receives_server_info() {
        let mut test_ctx = create_test_context().await;

        // Create admin user (no explicit ChatTopic permission needed)
        let password = "password";
        let hashed = db::hash_password(password).unwrap();
        test_ctx
            .db
            .users
            .create_user("admin", &hashed, true, true, &db::Permissions::new())
            .await
            .unwrap();

        // Set a topic
        test_ctx
            .db
            .config
            .set_topic("Admin can see this")
            .await
            .unwrap();

        let mut session_id = None;
        let handshake_complete = true;

        let result = handle_login(
            "admin".to_string(),
            password.to_string(),
            vec![],
            DEFAULT_TEST_LOCALE.to_string(),
            handshake_complete,
            &mut session_id,
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok(), "Login should succeed");

        // Read response
        drop(test_ctx.write_half);
        let mut response = String::new();
        test_ctx.client.read_to_string(&mut response).await.unwrap();

        let response_msg: ServerMessage = serde_json::from_str(response.trim()).unwrap();

        // Verify admin receives server_info
        match response_msg {
            ServerMessage::LoginResponse {
                success,
                is_admin,
                server_info,
                ..
            } => {
                assert!(success, "Login should succeed");
                assert_eq!(is_admin, Some(true), "Should be admin");
                assert!(server_info.is_some(), "Admin should receive server_info");
                let info = server_info.unwrap();
                assert_eq!(info.chat_topic, "Admin can see this");
            }
            _ => panic!("Expected LoginResponse"),
        }
    }

    #[tokio::test]
    async fn test_login_disabled_account() {
        let mut test_ctx = create_test_context().await;

        // Create a user first (so we're not the first user)
        let password = "password";
        let hashed = db::hash_password(password).unwrap();
        test_ctx
            .db
            .users
            .create_user("alice", &hashed, false, true, &db::Permissions::new())
            .await
            .unwrap();

        // Create a disabled user
        let bob_account = test_ctx
            .db
            .users
            .create_user("bob", &hashed, false, false, &db::Permissions::new())
            .await
            .unwrap();

        assert!(!bob_account.enabled, "Bob should be disabled");

        let mut session_id = None;
        let handshake_complete = true;

        // Attempt login with disabled account
        let result = handle_login(
            "bob".to_string(),
            password.to_string(),
            vec![],
            DEFAULT_TEST_LOCALE.to_string(),
            handshake_complete,
            &mut session_id,
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should fail with disconnect
        assert!(result.is_err(), "Login with disabled account should fail");
        assert!(session_id.is_none(), "Session ID should remain None");

        // Verify error message was sent
        let mut buf = vec![0u8; 1024];
        let n = test_ctx.client.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);
        assert!(
            response.contains("Account disabled"),
            "Should receive account disabled error"
        );
    }
}
