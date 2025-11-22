//! Login message handler

use super::{
    ERR_ALREADY_LOGGED_IN, ERR_AUTHENTICATION, ERR_DATABASE, ERR_FAILED_TO_CREATE_USER,
    ERR_HANDSHAKE_REQUIRED, ERR_INVALID_CREDENTIALS,
};
use super::{HandlerContext, current_timestamp};
use crate::db;
use nexus_common::protocol::{ServerMessage, UserInfo};
use std::io;

/// Handle a login request from the client
pub async fn handle_login(
    username: String,
    password: String,
    features: Vec<String>,
    handshake_complete: bool,
    session_id: &mut Option<u32>,
    ctx: &mut HandlerContext<'_>,
) -> io::Result<()> {
    if !handshake_complete {
        eprintln!("Login attempt from {} without handshake", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(ERR_HANDSHAKE_REQUIRED, Some("Login"))
            .await;
    }

    if session_id.is_some() {
        eprintln!("Duplicate login attempt from {}", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(ERR_ALREADY_LOGGED_IN, Some("Login"))
            .await;
    }

    // Check if this is the first user (will become admin)
    let is_first_user = match ctx.user_db.has_any_users().await {
        Ok(has_users) => !has_users,
        Err(e) => {
            eprintln!("Database error checking for users: {}", e);
            return ctx
                .send_error_and_disconnect(ERR_DATABASE, Some("Login"))
                .await;
        }
    };

    // Check if user exists
    let account = match ctx.user_db.get_user_by_username(&username).await {
        Ok(acc) => acc,
        Err(e) => {
            eprintln!("Database error looking up user {}: {}", username, e);
            return ctx
                .send_error_and_disconnect(ERR_DATABASE, Some("Login"))
                .await;
        }
    };

    // Verify password or create first user
    let authenticated_account = if let Some(account) = account {
        // User exists - verify password
        match db::verify_password(&password, &account.hashed_password) {
            Ok(true) => {
                println!("User '{}' logged in from {}", username, ctx.peer_addr);
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
    } else if is_first_user {
        // First user - create as admin
        let hashed_password = match db::hash_password(&password) {
            Ok(hash) => hash,
            Err(e) => {
                eprintln!("Failed to hash password for {}: {}", username, e);
                return ctx
                    .send_error_and_disconnect(ERR_FAILED_TO_CREATE_USER, Some("Login"))
                    .await;
            }
        };

        // Admin gets all permissions automatically (no need to store in table)
        match ctx
            .user_db
            .create_user(&username, &hashed_password, true, &db::Permissions::new())
            .await
        {
            Ok(account) => {
                println!(
                    "Created first user (admin): '{}' from {}",
                    username, ctx.peer_addr
                );
                account
            }
            Err(e) => {
                eprintln!("Failed to create admin user {}: {}", username, e);
                return ctx
                    .send_error_and_disconnect(ERR_FAILED_TO_CREATE_USER, Some("Login"))
                    .await;
            }
        }
    } else {
        // User doesn't exist and not first user
        eprintln!("User {} does not exist", username);
        return ctx
            .send_error_and_disconnect(ERR_INVALID_CREDENTIALS, Some("Login"))
            .await;
    };

    // User authenticated successfully - create session
    // Note: Features are client preferences (what they want to subscribe to)
    // Permissions are checked when executing commands, not at login
    let id = ctx
        .user_manager
        .add_user(
            authenticated_account.id,
            authenticated_account.username,
            ctx.peer_addr,
            authenticated_account.created_at,
            ctx.tx.clone(),
            features,
        )
        .await;
    *session_id = Some(id);

    let response = ServerMessage::LoginResponse {
        success: true,
        session_id: Some(id.to_string()),
        error: None,
    };
    ctx.send_message(&response).await?;

    // Broadcast user connected to all other users
    let user_info = UserInfo {
        session_id: id,
        username,
        login_time: current_timestamp(),
    };
    ctx.user_manager
        .broadcast_except(id, ServerMessage::UserConnected { user: user_info })
        .await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::handlers::testing::create_test_context;
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

        let response_msg: ServerMessage = serde_json::from_str(&response.trim()).unwrap();

        // Verify successful login response
        match response_msg {
            ServerMessage::LoginResponse {
                success,
                session_id,
                error,
            } => {
                assert!(success, "Login should indicate success");
                assert!(session_id.is_some(), "Should return session ID");
                assert!(error.is_none(), "Should have no error");
            }
            _ => panic!("Expected LoginResponse"),
        }

        // Verify user was created as admin in database
        let user = test_ctx
            .user_db
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
        let password = "mypassword";
        let hashed = db::hash_password(password).unwrap();
        test_ctx
            .user_db
            .create_user("bob", &hashed, false, &db::Permissions::new())
            .await
            .unwrap();

        let mut session_id = None;
        let handshake_complete = true;

        // Login with correct password
        let result = handle_login(
            "bob".to_string(),
            password.to_string(),
            vec![],
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

        let response_msg: ServerMessage = serde_json::from_str(&response.trim()).unwrap();

        // Verify successful login response
        match response_msg {
            ServerMessage::LoginResponse {
                success,
                session_id,
                error,
            } => {
                assert!(success, "Login should succeed");
                assert!(session_id.is_some(), "Should return session ID");
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
            .user_db
            .create_user("bob", &hashed, false, &db::Permissions::new())
            .await
            .unwrap();

        let mut session_id = None;
        let handshake_complete = true;

        // Login with wrong password
        let result = handle_login(
            "bob".to_string(),
            "wrongpassword".to_string(),
            vec![],
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
            .user_db
            .create_user("existing", &hashed, true, &db::Permissions::new())
            .await
            .unwrap();

        let mut session_id = None;
        let handshake_complete = true;

        // Try to login as non-existent user
        let result = handle_login(
            "nonexistent".to_string(),
            "password".to_string(),
            vec![],
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
    async fn test_duplicate_login_same_connection() {
        let mut test_ctx = create_test_context().await;

        // Create user first
        let password = "password";
        let hashed = db::hash_password(password).unwrap();
        test_ctx
            .user_db
            .create_user("alice", &hashed, false, &db::Permissions::new())
            .await
            .unwrap();

        let mut session_id = None;
        let handshake_complete = true;

        // First login
        let result1 = handle_login(
            "alice".to_string(),
            password.to_string(),
            vec![],
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
            password.to_string(),
            vec![],
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
}
