//! Login message handler

use super::{current_timestamp, rand_session_id, HandlerContext};
use crate::db;
use nexus_common::protocol::{ServerMessage, UserInfo};
use std::io;

/// Handle a login request from the client
pub async fn handle_login(
    username: String,
    password: String,
    features: Vec<String>,
    handshake_complete: bool,
    user_id: &mut Option<u32>,
    ctx: &mut HandlerContext<'_>,
) -> io::Result<()> {
    if !handshake_complete {
        eprintln!("Login attempt from {} without handshake", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect("Handshake required", Some("Login"))
            .await;
    }

    if user_id.is_some() {
        eprintln!("Duplicate login attempt from {}", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect("Already logged in", Some("Login"))
            .await;
    }

    // Check if this is the first user (will become admin)
    let is_first_user = match ctx.user_db.has_any_users().await {
        Ok(has_users) => !has_users,
        Err(e) => {
            eprintln!("Database error checking for users: {}", e);
            return ctx
                .send_error_and_disconnect("Database error", Some("Login"))
                .await;
        }
    };

    // Check if user exists
    let account = match ctx.user_db.get_user_by_username(&username).await {
        Ok(acc) => acc,
        Err(e) => {
            eprintln!("Database error looking up user {}: {}", username, e);
            return ctx
                .send_error_and_disconnect("Database error", Some("Login"))
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
                    .send_error_and_disconnect("Invalid username or password", Some("Login"))
                    .await;
            }
            Err(e) => {
                eprintln!("Password verification error for {}: {}", username, e);
                return ctx
                    .send_error_and_disconnect("Authentication error", Some("Login"))
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
                    .send_error_and_disconnect("Failed to create user", Some("Login"))
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
                    .send_error_and_disconnect("Failed to create user", Some("Login"))
                    .await;
            }
        }
    } else {
        // User doesn't exist and not first user
        eprintln!("User {} does not exist", username);
        return ctx
            .send_error_and_disconnect("Invalid username or password", Some("Login"))
            .await;
    };

    // User authenticated successfully - create session
    // Note: Features are client preferences (what they want to subscribe to)
    // Permissions are checked when executing commands, not at login
    let session_id = format!("{}-{}", username, rand_session_id());
    let id = ctx
        .user_manager
        .add_user(
            authenticated_account.id,
            username.clone(),
            session_id.clone(),
            ctx.peer_addr,
            ctx.tx.clone(),
            features,
        )
        .await;
    *user_id = Some(id);

    let response = ServerMessage::LoginResponse {
        success: true,
        session_id: Some(session_id),
        error: None,
    };
    ctx.send_message(&response).await?;

    // Broadcast user connected to all other users
    let user_info = UserInfo {
        id,
        username: username.clone(),
        login_time: current_timestamp(),
    };
    ctx.user_manager
        .broadcast_except(id, ServerMessage::UserConnected { user: user_info })
        .await;

    Ok(())
}