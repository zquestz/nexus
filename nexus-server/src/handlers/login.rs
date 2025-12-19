//! Login message handler

use std::io;

use tokio::io::AsyncWrite;

use nexus_common::protocol::{ChatInfo, ServerInfo, ServerMessage, UserInfo};
use nexus_common::validators::{
    self, AvatarError, FeaturesError, LocaleError, NicknameError, PasswordError, UsernameError,
};

use super::{
    HandlerContext, current_timestamp, err_account_disabled, err_already_logged_in,
    err_authentication, err_avatar_invalid_format, err_avatar_too_large,
    err_avatar_unsupported_type, err_database, err_failed_to_create_user,
    err_features_empty_feature, err_features_feature_too_long, err_features_invalid_characters,
    err_features_too_many, err_handshake_required, err_invalid_credentials,
    err_locale_invalid_characters, err_locale_too_long, err_nickname_empty, err_nickname_in_use,
    err_nickname_invalid, err_nickname_is_username, err_nickname_required, err_nickname_too_long,
    err_password_empty, err_password_too_long, err_username_empty, err_username_invalid,
    err_username_too_long,
};
#[cfg(test)]
use crate::constants::FEATURE_CHAT;
use crate::db::{self, Permission};
use crate::users::manager::AddUserError;
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

/// Handle a login request from the client
pub async fn handle_login<W>(
    request: LoginRequest,
    session_id: &mut Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let LoginRequest {
        username,
        password,
        features,
        locale,
        avatar,
        nickname,
        handshake_complete,
    } = request;

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

    // Validate avatar (if provided)
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

    // Handle nickname for shared accounts
    // For shared accounts: nickname is required and must be unique
    // For regular accounts: nickname is silently ignored
    let validated_nickname = if authenticated_account.is_shared {
        // Shared account - nickname is required
        let Some(nickname) = nickname else {
            return ctx
                .send_error_and_disconnect(&err_nickname_required(&locale), Some("Login"))
                .await;
        };

        // Validate nickname format
        if let Err(e) = validators::validate_nickname(&nickname) {
            let error_msg = match e {
                NicknameError::Empty => err_nickname_empty(&locale),
                NicknameError::TooLong => {
                    err_nickname_too_long(&locale, validators::MAX_NICKNAME_LENGTH)
                }
                NicknameError::InvalidCharacters => err_nickname_invalid(&locale),
            };
            return ctx
                .send_error_and_disconnect(&error_msg, Some("Login"))
                .await;
        }

        // Check if nickname matches an existing username in database (case-insensitive)
        match ctx.db.users.username_exists(&nickname).await {
            Ok(true) => {
                return ctx
                    .send_error_and_disconnect(&err_nickname_is_username(&locale), Some("Login"))
                    .await;
            }
            Ok(false) => {}
            Err(e) => {
                eprintln!("Database error checking nickname uniqueness: {}", e);
                return ctx
                    .send_error_and_disconnect(&err_database(&locale), Some("Login"))
                    .await;
            }
        }

        // Check if nickname is in use by an active session (case-insensitive)
        if ctx.user_manager.is_nickname_in_use(&nickname).await {
            return ctx
                .send_error_and_disconnect(&err_nickname_in_use(&locale), Some("Login"))
                .await;
        }

        Some(nickname)
    } else {
        // Regular account - ignore any provided nickname
        None
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
    //
    // For shared accounts, add_user performs an atomic nickname uniqueness check
    // to prevent race conditions where two users could claim the same nickname.
    let id = match ctx
        .user_manager
        .add_user(NewSessionParams {
            session_id: 0, // Will be assigned by add_user
            db_user_id: authenticated_account.id,
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
            nickname: validated_nickname.clone(),
        })
        .await
    {
        Ok(id) => id,
        Err(AddUserError::NicknameInUse) => {
            return ctx
                .send_error_and_disconnect(&err_nickname_in_use(&locale), Some("Login"))
                .await;
        }
        Err(AddUserError::NicknameMatchesUsername) => {
            // This shouldn't happen because we check username_exists() earlier,
            // but handle it gracefully in case of race condition
            return ctx
                .send_error_and_disconnect(&err_nickname_in_use(&locale), Some("Login"))
                .await;
        }
    };
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

    // Fetch server info (name/description/image always, topic requires permission, max_conn requires admin)
    let name = ctx.db.config.get_server_name().await;
    let description = ctx.db.config.get_server_description().await;
    let image = ctx.db.config.get_server_image().await;

    // Fetch max connections per IP (admin only)
    let max_connections_per_ip = if authenticated_account.is_admin {
        Some(ctx.db.config.get_max_connections_per_ip().await as u32)
    } else {
        None
    };

    let server_info = Some(ServerInfo {
        name: Some(name),
        description: Some(description),
        version: Some(env!("CARGO_PKG_VERSION").to_string()),
        max_connections_per_ip,
        image: Some(image),
    });

    // Fetch chat info only if user has ChatTopic permission
    let chat_info =
        if authenticated_account.is_admin || cached_permissions.contains(&Permission::ChatTopic) {
            match ctx.db.chat.get_topic().await {
                Ok(topic) => Some(ChatInfo {
                    topic: topic.topic,
                    topic_set_by: topic.set_by,
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
        chat_info,
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
        nickname: validated_nickname,
        login_time: current_timestamp(),
        is_admin: authenticated_account.is_admin,
        is_shared: authenticated_account.is_shared,
        session_ids: vec![id],
        locale: locale.clone(),
        avatar,
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
    use crate::handlers::testing::{DEFAULT_TEST_LOCALE, create_test_context, read_server_message};

    #[tokio::test]
    async fn test_login_requires_handshake() {
        let mut test_ctx = create_test_context().await;
        let mut session_id = None;
        let handshake_complete = false; // Not completed

        // Try to login without handshake
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

        // Should succeed
        assert!(result.is_ok(), "First login should succeed");
        assert!(session_id.is_some(), "Session ID should be set");

        // Read response

        let response_msg = read_server_message(&mut test_ctx.client).await;

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
            .create_user("bob", &hashed, false, false, true, &perms)
            .await
            .unwrap();

        let mut session_id = None;
        let handshake_complete = true;

        // Login with correct password
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

        // Should succeed
        assert!(result.is_ok(), "Login with correct password should succeed");
        assert!(session_id.is_some(), "Session ID should be set");

        // Read response

        let response_msg = read_server_message(&mut test_ctx.client).await;

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
            .create_user("bob", &hashed, false, false, true, &db::Permissions::new())
            .await
            .unwrap();

        let mut session_id = None;
        let handshake_complete = true;

        // Login with wrong password
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
            .create_user(
                "existing",
                &hashed,
                true,
                false,
                true,
                &db::Permissions::new(),
            )
            .await
            .unwrap();

        let mut session_id = None;
        let handshake_complete = true;

        // Try to login as non-existent user
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
            .create_user(
                "admin",
                &admin_hashed,
                true,
                false,
                true,
                &db::Permissions::new(),
            )
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
            .create_user("alice", &hashed, false, false, true, &perms)
            .await
            .unwrap();

        let handshake_complete = true;
        let mut session_id = None;

        // Attempt login
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

        // Read response

        let response_msg = read_server_message(&mut test_ctx.client).await;

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
            .create_user(
                "alice",
                &hashed,
                false,
                false,
                true,
                &db::Permissions::new(),
            )
            .await
            .unwrap();

        let mut session_id = None;
        let handshake_complete = true;

        // First login
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

        // Second login on same connection (should fail)
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
            .create_user("alice", &hashed, false, false, true, &perms)
            .await
            .unwrap();

        // Set a topic
        test_ctx
            .db
            .chat
            .set_topic("Test server topic", "admin")
            .await
            .unwrap();

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

        // Read response

        let response_msg = read_server_message(&mut test_ctx.client).await;

        // Verify LoginResponse includes server_info with chat_topic
        match response_msg {
            ServerMessage::LoginResponse {
                success,
                server_info,
                chat_info,
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
                    info.max_connections_per_ip.is_none(),
                    "Non-admin should not receive max_connections_per_ip"
                );
                assert!(chat_info.is_some(), "Should include chat_info");
                let chat = chat_info.unwrap();
                assert_eq!(chat.topic, "Test server topic", "Should include chat topic");
            }
            _ => panic!("Expected LoginResponse"),
        }
    }

    #[tokio::test]
    async fn test_login_excludes_topic_without_permission() {
        let mut test_ctx = create_test_context().await;

        // Create user without ChatTopic permission
        let password = "password";
        let hashed = db::hash_password(password).unwrap();
        test_ctx
            .db
            .users
            .create_user(
                "alice",
                &hashed,
                false,
                false,
                true,
                &db::Permissions::new(),
            )
            .await
            .unwrap();

        // Set a topic (user shouldn't see it)
        // Set a topic that should NOT be visible
        test_ctx
            .db
            .chat
            .set_topic("Secret topic", "admin")
            .await
            .unwrap();

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

        // Read response

        let response_msg = read_server_message(&mut test_ctx.client).await;

        // Verify LoginResponse includes server_info with name/description but excludes topic
        match response_msg {
            ServerMessage::LoginResponse {
                success,
                server_info,
                chat_info,
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
                    info.max_connections_per_ip.is_none(),
                    "Non-admin should not receive max_connections_per_ip"
                );
                assert!(
                    chat_info.is_none(),
                    "Should NOT include chat_info without permission"
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
            .create_user("admin", &hashed, true, false, true, &db::Permissions::new())
            .await
            .unwrap();

        // Set a topic
        test_ctx
            .db
            .chat
            .set_topic("Admin can see this", "admin")
            .await
            .unwrap();

        let mut session_id = None;
        let handshake_complete = true;

        let request = LoginRequest {
            username: "admin".to_string(),
            password: password.to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: None,
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Login should succeed");

        // Read response

        let response_msg = read_server_message(&mut test_ctx.client).await;

        // Verify admin receives server_info and chat_info
        match response_msg {
            ServerMessage::LoginResponse {
                success,
                is_admin,
                server_info,
                chat_info,
                ..
            } => {
                assert!(success, "Login should succeed");
                assert_eq!(is_admin, Some(true), "Should be admin");
                assert!(server_info.is_some(), "Admin should receive server_info");
                let info = server_info.unwrap();
                assert_eq!(
                    info.max_connections_per_ip,
                    Some(5),
                    "Admin should receive max_connections_per_ip"
                );
                assert!(chat_info.is_some(), "Admin should receive chat_info");
                let chat = chat_info.unwrap();
                assert_eq!(chat.topic, "Admin can see this");
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
            .create_user(
                "alice",
                &hashed,
                false,
                false,
                true,
                &db::Permissions::new(),
            )
            .await
            .unwrap();

        // Create a disabled user
        let bob_account = test_ctx
            .db
            .users
            .create_user("bob", &hashed, false, false, false, &db::Permissions::new())
            .await
            .unwrap();

        assert!(!bob_account.enabled, "Bob should be disabled");

        let mut session_id = None;
        let handshake_complete = true;

        // Attempt login with disabled account
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

        // Should fail with disconnect
        assert!(result.is_err(), "Login with disabled account should fail");
        assert!(session_id.is_none(), "Session ID should remain None");

        // Verify error message was sent
        let response_msg = read_server_message(&mut test_ctx.client).await;
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

        // Create a user first (so we're not the first user)
        let password = "password";
        let hashed = db::hash_password(password).unwrap();
        test_ctx
            .db
            .users
            .create_user(
                "alice",
                &hashed,
                false,
                false,
                true,
                &db::Permissions::new(),
            )
            .await
            .unwrap();

        let mut session_id = None;
        let handshake_complete = true;

        // Attempt login with wrong password using Spanish locale
        let request = LoginRequest {
            username: "alice".to_string(),
            password: "wrong_password".to_string(),
            features: vec![],
            locale: "es".to_string(), // Request Spanish locale
            avatar: None,
            nickname: None,
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        // Should fail with disconnect
        assert!(result.is_err(), "Login with wrong password should fail");
        assert!(session_id.is_none(), "Session ID should remain None");

        // Verify error message was sent in Spanish
        let response_msg = read_server_message(&mut test_ctx.client).await;
        match response_msg {
            ServerMessage::Error { message, .. } => {
                // Spanish error message should contain "Usuario o contraseña" (not English "Invalid username or password")
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

        // Create a user first (so we're not the first user)
        let password = "password";
        let hashed = db::hash_password(password).unwrap();
        test_ctx
            .db
            .users
            .create_user(
                "alice",
                &hashed,
                false,
                false,
                true,
                &db::Permissions::new(),
            )
            .await
            .unwrap();

        let mut session_id = None;
        let handshake_complete = true;

        // Attempt login with wrong password using empty locale (should default to "en")
        let request = LoginRequest {
            username: "alice".to_string(),
            password: "wrong_password".to_string(),
            features: vec![],
            locale: "".to_string(), // Empty locale should default to English
            avatar: None,
            nickname: None,
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        // Should fail with disconnect
        assert!(result.is_err(), "Login with wrong password should fail");
        assert!(session_id.is_none(), "Session ID should remain None");

        // Verify error message was sent in English (default)
        let response_msg = read_server_message(&mut test_ctx.client).await;
        match response_msg {
            ServerMessage::Error { message, .. } => {
                // Should be English (contains "Invalid" or "username")
                assert!(
                    message.contains("Invalid") || message.contains("username"),
                    "Error message should be in English (default), got: {}",
                    message
                );
            }
            _ => panic!("Expected Error message"),
        }
    }

    // =========================================================================
    // Avatar validation tests
    // =========================================================================

    #[tokio::test]
    async fn test_login_with_valid_avatar() {
        let mut test_ctx = create_test_context().await;
        let mut session_id = None;
        let handshake_complete = true;

        // Valid PNG data URI (minimal)
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

        let response_msg = read_server_message(&mut test_ctx.client).await;
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

        // Create avatar that exceeds MAX_AVATAR_DATA_URI_LENGTH
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

        // Should fail with disconnect
        assert!(result.is_err(), "Login with oversized avatar should fail");
        assert!(session_id.is_none(), "Session ID should remain None");

        let response_msg = read_server_message(&mut test_ctx.client).await;
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

        let response_msg = read_server_message(&mut test_ctx.client).await;
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

        let response_msg = read_server_message(&mut test_ctx.client).await;
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

        // No avatar (None)
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

        let response_msg = read_server_message(&mut test_ctx.client).await;
        match response_msg {
            ServerMessage::LoginResponse { success, .. } => {
                assert!(success, "Login should succeed without avatar");
            }
            _ => panic!("Expected LoginResponse"),
        }
    }

    // ========================================================================
    // Shared Account Nickname Tests
    // ========================================================================

    #[tokio::test]
    async fn test_login_shared_account_with_valid_nickname() {
        let mut test_ctx = create_test_context().await;
        let handshake_complete = true;

        // Create a shared account in the database
        let password = "password123";
        let hashed = db::hash_password(password).expect("hash should succeed");

        // First create a regular admin so we can create the shared account
        test_ctx
            .db
            .users
            .create_first_user_if_none_exist("admin", &hashed)
            .await
            .expect("admin creation should succeed");

        // Create shared account
        test_ctx
            .db
            .users
            .create_user(
                "shared_acct",
                &hashed,
                false,
                true,
                true,
                &db::Permissions::new(),
            )
            .await
            .expect("shared account creation should succeed");

        // Login with valid nickname
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

        let response_msg = read_server_message(&mut test_ctx.client).await;
        match response_msg {
            ServerMessage::LoginResponse { success, .. } => {
                assert!(success, "Login response should indicate success");
            }
            _ => panic!("Expected LoginResponse"),
        }

        // Verify session has nickname stored
        let session = test_ctx
            .user_manager
            .get_user_by_session_id(session_id.unwrap())
            .await
            .expect("session should exist");
        assert_eq!(session.nickname, Some("Alice".to_string()));
        assert!(session.is_shared);
    }

    #[tokio::test]
    async fn test_login_shared_account_without_nickname_fails() {
        let mut test_ctx = create_test_context().await;
        let handshake_complete = true;

        // Create shared account
        let password = "password123";
        let hashed = db::hash_password(password).expect("hash should succeed");

        test_ctx
            .db
            .users
            .create_first_user_if_none_exist("admin", &hashed)
            .await
            .expect("admin creation should succeed");

        test_ctx
            .db
            .users
            .create_user(
                "shared_acct",
                &hashed,
                false,
                true,
                true,
                &db::Permissions::new(),
            )
            .await
            .expect("shared account creation should succeed");

        // Login without nickname (None)
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

        // First login creates admin (regular account)
        let mut session_id = None;
        let request = LoginRequest {
            username: "alice".to_string(),
            password: "password123".to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: Some("SomeNickname".to_string()), // Should be ignored
            handshake_complete,
        };
        let result = handle_login(request, &mut session_id, &mut test_ctx.handler_context()).await;

        assert!(
            result.is_ok(),
            "Login with nickname for regular account should succeed"
        );
        assert!(session_id.is_some(), "Session ID should be set");

        // Verify session has no nickname (ignored for regular accounts)
        let session = test_ctx
            .user_manager
            .get_user_by_session_id(session_id.unwrap())
            .await
            .expect("session should exist");
        assert_eq!(
            session.nickname, None,
            "Nickname should be None for regular account"
        );
        assert!(!session.is_shared);
    }

    #[tokio::test]
    async fn test_login_shared_account_nickname_collision_with_username() {
        let mut test_ctx = create_test_context().await;
        let handshake_complete = true;

        let password = "password123";
        let hashed = db::hash_password(password).expect("hash should succeed");

        // Create regular user "alice"
        test_ctx
            .db
            .users
            .create_first_user_if_none_exist("alice", &hashed)
            .await
            .expect("admin creation should succeed");

        // Create shared account
        test_ctx
            .db
            .users
            .create_user(
                "shared_acct",
                &hashed,
                false,
                true,
                true,
                &db::Permissions::new(),
            )
            .await
            .expect("shared account creation should succeed");

        // Try to login with nickname that matches existing username
        let mut session_id = None;
        let request = LoginRequest {
            username: "shared_acct".to_string(),
            password: password.to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: Some("alice".to_string()), // Collides with existing username
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
        let hashed = db::hash_password(password).expect("hash should succeed");

        // Create regular user "Alice"
        test_ctx
            .db
            .users
            .create_first_user_if_none_exist("Alice", &hashed)
            .await
            .expect("admin creation should succeed");

        // Create shared account
        test_ctx
            .db
            .users
            .create_user(
                "shared_acct",
                &hashed,
                false,
                true,
                true,
                &db::Permissions::new(),
            )
            .await
            .expect("shared account creation should succeed");

        // Try to login with nickname that matches existing username (different case)
        let mut session_id = None;
        let request = LoginRequest {
            username: "shared_acct".to_string(),
            password: password.to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: Some("ALICE".to_string()), // Collides case-insensitively
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
    async fn test_login_shared_account_nickname_collision_with_active_session() {
        let mut test_ctx = create_test_context().await;
        let handshake_complete = true;

        let password = "password123";
        let hashed = db::hash_password(password).expect("hash should succeed");

        // Create admin first
        test_ctx
            .db
            .users
            .create_first_user_if_none_exist("admin", &hashed)
            .await
            .expect("admin creation should succeed");

        // Create shared account
        test_ctx
            .db
            .users
            .create_user(
                "shared_acct",
                &hashed,
                false,
                true,
                true,
                &db::Permissions::new(),
            )
            .await
            .expect("shared account creation should succeed");

        // First login with nickname "Bob"
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

        // Read the login response
        let _response1 = read_server_message(&mut test_ctx.client).await;

        // Second login attempt with same nickname "Bob"
        let mut session_id2 = None;
        let request2 = LoginRequest {
            username: "shared_acct".to_string(),
            password: password.to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: Some("Bob".to_string()), // Same nickname as active session
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
        let hashed = db::hash_password(password).expect("hash should succeed");

        // Create admin first
        test_ctx
            .db
            .users
            .create_first_user_if_none_exist("admin", &hashed)
            .await
            .expect("admin creation should succeed");

        // Create shared account
        test_ctx
            .db
            .users
            .create_user(
                "shared_acct",
                &hashed,
                false,
                true,
                true,
                &db::Permissions::new(),
            )
            .await
            .expect("shared account creation should succeed");

        // First login with nickname "Alice"
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

        // Read the login response
        let _response1 = read_server_message(&mut test_ctx.client).await;

        // Second login with different nickname "Bob"
        let mut session_id2 = None;
        let request2 = LoginRequest {
            username: "shared_acct".to_string(),
            password: password.to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: Some("Bob".to_string()), // Different nickname
            handshake_complete,
        };
        let result2 =
            handle_login(request2, &mut session_id2, &mut test_ctx.handler_context()).await;

        assert!(
            result2.is_ok(),
            "Second login with different nickname should succeed"
        );
        assert!(session_id2.is_some(), "Session ID should be set");

        // Verify both sessions exist with their nicknames
        let session1 = test_ctx
            .user_manager
            .get_user_by_session_id(session_id1.unwrap())
            .await
            .expect("session 1 should exist");
        assert_eq!(session1.nickname, Some("Alice".to_string()));

        let session2 = test_ctx
            .user_manager
            .get_user_by_session_id(session_id2.unwrap())
            .await
            .expect("session 2 should exist");
        assert_eq!(session2.nickname, Some("Bob".to_string()));
    }

    #[tokio::test]
    async fn test_login_shared_account_nickname_validation_empty() {
        let mut test_ctx = create_test_context().await;
        let handshake_complete = true;

        let password = "password123";
        let hashed = db::hash_password(password).expect("hash should succeed");

        test_ctx
            .db
            .users
            .create_first_user_if_none_exist("admin", &hashed)
            .await
            .expect("admin creation should succeed");

        test_ctx
            .db
            .users
            .create_user(
                "shared_acct",
                &hashed,
                false,
                true,
                true,
                &db::Permissions::new(),
            )
            .await
            .expect("shared account creation should succeed");

        // Try to login with empty nickname
        let mut session_id = None;
        let request = LoginRequest {
            username: "shared_acct".to_string(),
            password: password.to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: Some("".to_string()), // Empty nickname
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
        let hashed = db::hash_password(password).expect("hash should succeed");

        test_ctx
            .db
            .users
            .create_first_user_if_none_exist("admin", &hashed)
            .await
            .expect("admin creation should succeed");

        test_ctx
            .db
            .users
            .create_user(
                "shared_acct",
                &hashed,
                false,
                true,
                true,
                &db::Permissions::new(),
            )
            .await
            .expect("shared account creation should succeed");

        // Try to login with nickname that exceeds max length
        let mut session_id = None;
        let request = LoginRequest {
            username: "shared_acct".to_string(),
            password: password.to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: Some("a".repeat(validators::MAX_NICKNAME_LENGTH + 1)), // Too long
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
        let hashed = db::hash_password(password).expect("hash should succeed");

        test_ctx
            .db
            .users
            .create_first_user_if_none_exist("admin", &hashed)
            .await
            .expect("admin creation should succeed");

        test_ctx
            .db
            .users
            .create_user(
                "shared_acct",
                &hashed,
                false,
                true,
                true,
                &db::Permissions::new(),
            )
            .await
            .expect("shared account creation should succeed");

        // Try to login with nickname containing invalid characters (spaces)
        let mut session_id = None;
        let request = LoginRequest {
            username: "shared_acct".to_string(),
            password: password.to_string(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: Some("Alice Smith".to_string()), // Space not allowed
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
        let hashed = db::hash_password(password).expect("hash should succeed");

        // Create regular user "alice" and login
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

        // Read alice's login response
        let _response = read_server_message(&mut test_ctx.client).await;

        // Create shared account
        test_ctx
            .db
            .users
            .create_user(
                "shared_acct",
                &hashed,
                false,
                true,
                true,
                &db::Permissions::new(),
            )
            .await
            .expect("shared account creation should succeed");

        // Try to login to shared account with nickname "alice" - should fail
        // because alice is logged in (nickname would conflict with her username)
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
}
