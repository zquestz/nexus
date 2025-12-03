//! Login message handler

use std::io;

use nexus_common::protocol::{ServerInfo, ServerMessage, UserInfo};
use nexus_common::validators::{self, FeaturesError, LocaleError, PasswordError, UsernameError};

use super::{
    HandlerContext, current_timestamp, err_account_disabled, err_already_logged_in,
    err_authentication, err_database, err_failed_to_create_user, err_features_empty_feature,
    err_features_feature_too_long, err_features_invalid_characters, err_features_too_many,
    err_handshake_required, err_invalid_credentials, err_locale_invalid_characters,
    err_locale_too_long, err_password_empty, err_password_too_long, err_username_empty,
    err_username_invalid, err_username_too_long,
};
#[cfg(test)]
use crate::constants::FEATURE_CHAT;
use crate::db::{self, Permission};
use crate::users::user::NewSessionParams;

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
            .send_error_and_disconnect(&err_handshake_required(&locale), Some("Login"))
            .await;
    }

    // Check for duplicate login on same connection
    if session_id.is_some() {
        eprintln!("Duplicate login attempt from {}", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(&err_already_logged_in(&locale), Some("Login"))
            .await;
    }

    // Validate username
    if let Err(e) = validators::validate_username(&username) {
        let error_msg = match e {
            UsernameError::Empty => err_username_empty(&locale),
            UsernameError::TooLong => {
                err_username_too_long(&locale, validators::MAX_USERNAME_LENGTH)
            }
            UsernameError::InvalidCharacters => err_username_invalid(&locale),
        };
        return ctx
            .send_error_and_disconnect(&error_msg, Some("Login"))
            .await;
    }

    // Validate password
    if let Err(e) = validators::validate_password(&password) {
        let error_msg = match e {
            PasswordError::Empty => err_password_empty(&locale),
            PasswordError::TooLong => {
                err_password_too_long(&locale, validators::MAX_PASSWORD_LENGTH)
            }
        };
        return ctx
            .send_error_and_disconnect(&error_msg, Some("Login"))
            .await;
    }

    // Validate locale
    if let Err(e) = validators::validate_locale(&locale) {
        let error_msg = match e {
            LocaleError::TooLong => err_locale_too_long(&locale, validators::MAX_LOCALE_LENGTH),
            LocaleError::InvalidCharacters => err_locale_invalid_characters(&locale),
        };
        return ctx
            .send_error_and_disconnect(&error_msg, Some("Login"))
            .await;
    }

    // Validate features
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
            .send_error_and_disconnect(&error_msg, Some("Login"))
            .await;
    }

    // Look up user account in database
    let account = match ctx.db.users.get_user_by_username(&username).await {
        Ok(acc) => acc,
        Err(e) => {
            eprintln!("Database error looking up user {}: {}", username, e);
            return ctx
                .send_error_and_disconnect(&err_database(&locale), Some("Login"))
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
                        "Login from {} for disabled account: {}",
                        ctx.peer_addr, username
                    );
                    return ctx
                        .send_error_and_disconnect(
                            &err_account_disabled(&locale, &username),
                            Some("Login"),
                        )
                        .await;
                }
                account
            }
            Ok(false) => {
                eprintln!(
                    "Login from {} failed: invalid credentials for {}",
                    ctx.peer_addr, username
                );
                return ctx
                    .send_error_and_disconnect(&err_invalid_credentials(&locale), Some("Login"))
                    .await;
            }
            Err(e) => {
                eprintln!("Password verification error for {}: {}", username, e);
                return ctx
                    .send_error_and_disconnect(&err_authentication(&locale), Some("Login"))
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
                    .send_error_and_disconnect(
                        &err_failed_to_create_user(&locale, &username),
                        Some("Login"),
                    )
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
                // User doesn't exist and not first user - use same error as invalid password
                // to avoid revealing whether username exists
                return ctx
                    .send_error_and_disconnect(&err_invalid_credentials(&locale), Some("Login"))
                    .await;
            }
            Err(e) => {
                eprintln!("Failed to create first user {}: {}", username, e);
                return ctx
                    .send_error_and_disconnect(
                        &err_failed_to_create_user(&locale, &username),
                        Some("Login"),
                    )
                    .await;
            }
        }
    };

    // Fetch user permissions from database (used for both caching and LoginResponse)
    let cached_permissions = if authenticated_account.is_admin {
        // Admins bypass permission checks, so we can use an empty set
        std::collections::HashSet::new()
    } else {
        match ctx
            .db
            .users
            .get_user_permissions(authenticated_account.id)
            .await
        {
            Ok(perms) => perms.permissions,
            Err(e) => {
                eprintln!(
                    "Error fetching permissions for {}: {}",
                    authenticated_account.username, e
                );
                std::collections::HashSet::new()
            }
        }
    };

    // Create session in UserManager with cached permissions
    // Note: Features are client preferences (what they want to subscribe to)
    // Permissions are now cached in the User struct to avoid DB lookups during broadcasts
    let id = ctx
        .user_manager
        .add_user(NewSessionParams {
            session_id: 0, // Will be assigned by add_user
            db_user_id: authenticated_account.id,
            username: authenticated_account.username.clone(),
            is_admin: authenticated_account.is_admin,
            permissions: cached_permissions.clone(),
            address: ctx.peer_addr,
            created_at: authenticated_account.created_at,
            tx: ctx.tx.clone(),
            features,
            locale: locale.clone(),
        })
        .await;
    *session_id = Some(id);

    // Convert cached permissions to strings for LoginResponse
    let user_permissions: Vec<String> = if authenticated_account.is_admin {
        // Admins get all permissions automatically - return empty list
        // Client checks is_admin flag to know they have all permissions
        vec![]
    } else {
        cached_permissions
            .iter()
            .map(|p| p.as_str().to_string())
            .collect()
    };

    // Fetch server info if user has ChatTopic permission (use cached permissions)
    let server_info =
        if authenticated_account.is_admin || cached_permissions.contains(&Permission::ChatTopic) {
            // Fetch chat topic from database
            match ctx.db.config.get_topic().await {
                Ok(chat_topic) => Some(ServerInfo {
                    chat_topic: chat_topic.topic,
                    chat_topic_set_by: chat_topic.set_by,
                }),
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
        session_id: Some(id),
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
            vec![FEATURE_CHAT.to_string()],
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
            vec![FEATURE_CHAT.to_string()],
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
            .set_topic("Test server topic", "admin")
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
        test_ctx
            .db
            .config
            .set_topic("Secret topic", "admin")
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
            .set_topic("Admin can see this", "admin")
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
            response.contains("Account")
                && response.contains("bob")
                && response.contains("disabled"),
            "Should receive account disabled error with username"
        );
    }

    #[tokio::test]
    async fn test_login_error_uses_requested_locale() {
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

        let mut session_id = None;
        let handshake_complete = true;

        // Attempt login with wrong password using Spanish locale
        let result = handle_login(
            "alice".to_string(),
            "wrong_password".to_string(),
            vec![],
            "es".to_string(), // Request Spanish locale
            handshake_complete,
            &mut session_id,
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should fail with disconnect
        assert!(result.is_err(), "Login with wrong password should fail");
        assert!(session_id.is_none(), "Session ID should remain None");

        // Verify error message was sent in Spanish
        let mut buf = vec![0u8; 1024];
        let n = test_ctx.client.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);

        // Spanish error message should contain "Usuario o contraseña" (not English "Invalid username or password")
        assert!(
            response.contains("Usuario") || response.contains("contraseña"),
            "Error message should be in Spanish, got: {}",
            response
        );
    }

    #[tokio::test]
    async fn test_login_error_defaults_to_english() {
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

        let mut session_id = None;
        let handshake_complete = true;

        // Attempt login with wrong password using empty locale (should default to "en")
        let result = handle_login(
            "alice".to_string(),
            "wrong_password".to_string(),
            vec![],
            "".to_string(), // Empty locale should default to English
            handshake_complete,
            &mut session_id,
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should fail with disconnect
        assert!(result.is_err(), "Login with wrong password should fail");
        assert!(session_id.is_none(), "Session ID should remain None");

        // Verify error message was sent in English (default)
        let mut buf = vec![0u8; 1024];
        let n = test_ctx.client.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);

        // English error message should contain "Invalid username or password"
        assert!(
            response.contains("Invalid") && response.contains("username"),
            "Error message should be in English (default), got: {}",
            response
        );
    }
}
