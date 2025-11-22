//! Chat message handler

use super::HandlerContext;
use crate::db::Permission;
use nexus_common::protocol::ServerMessage;
use std::io;

/// Handle a chat send request from the client
pub async fn handle_chat_send(
    message: String,
    user_id: Option<u32>,
    ctx: &mut HandlerContext<'_>,
) -> io::Result<()> {
    if user_id.is_none() {
        eprintln!("ChatSend from {} without login", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect("Not logged in", Some("ChatSend"))
            .await;
    }

    // Check message length limit (1024 characters)
    if message.len() > 1024 {
        eprintln!(
            "ChatSend from {} exceeds length limit: {} chars",
            ctx.peer_addr,
            message.len()
        );
        return ctx
            .send_error_and_disconnect("Message too long (max 1024 characters)", Some("ChatSend"))
            .await;
    }

    let id = user_id.unwrap();

    // Get the user and check permissions
    let user = match ctx.user_manager.get_user(id).await {
        Some(u) => u,
        None => {
            return ctx
                .send_error_and_disconnect("Authentication error", Some("ChatSend"))
                .await;
        }
    };

    let has_perm = match ctx
        .user_db
        .has_permission(user.db_user_id, Permission::SendChat)
        .await
    {
        Ok(has) => has,
        Err(e) => {
            eprintln!("ChatSend permission check error: {}", e);
            return ctx
                .send_error_and_disconnect("Database error", Some("ChatSend"))
                .await;
        }
    };

    if !has_perm {
        eprintln!("ChatSend from {} without permission", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect("Permission denied", Some("ChatSend"))
            .await;
    }

    if !user.has_feature("chat") {
        eprintln!("ChatSend from {} without chat feature enabled", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect("Chat feature not enabled", Some("ChatSend"))
            .await;
    }

    // Broadcast to all users with chat feature
    ctx.user_manager
        .broadcast_to_feature(
            "chat",
            ServerMessage::ChatMessage {
                user_id: id,
                username: user.username.clone(),
                message: message.clone(),
            },
        )
        .await;

    Ok(())
}