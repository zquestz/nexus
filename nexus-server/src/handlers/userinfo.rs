//! UserInfo message handler

use super::{HandlerContext, ERR_NOT_LOGGED_IN, ERR_AUTHENTICATION, ERR_DATABASE, ERR_PERMISSION_DENIED};
use crate::db::Permission;
use nexus_common::protocol::{ServerMessage, UserInfoDetailed};
use std::io;

/// Handle a userinfo request from the client
pub async fn handle_userinfo(
    requested_user_id: u32,
    user_id: Option<u32>,
    ctx: &mut HandlerContext<'_>,
) -> io::Result<()> {
    let id = match user_id {
        Some(id) => id,
        None => {
            eprintln!("UserInfo request from {} without login", ctx.peer_addr);
            return ctx
                .send_error_and_disconnect(ERR_NOT_LOGGED_IN, Some("UserInfo"))
                .await;
        }
    };

    // Get the requesting user
    let requesting_user = match ctx.user_manager.get_user(id).await {
        Some(u) => u,
        None => {
            eprintln!("UserInfo request from unknown user {}", ctx.peer_addr);
            return ctx
                .send_error_and_disconnect(ERR_AUTHENTICATION, Some("UserInfo"))
                .await;
        }
    };

    // Check if requesting user has permission to view user info
    let has_perm = match ctx
        .user_db
        .has_permission(requesting_user.db_user_id, Permission::UserInfo)
        .await
    {
        Ok(has) => has,
        Err(e) => {
            eprintln!("UserInfo permission check error: {}", e);
            return ctx
                .send_error_and_disconnect(ERR_DATABASE, Some("UserInfo"))
                .await;
        }
    };

    if !has_perm {
        eprintln!("UserInfo request from {} without permission", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(ERR_PERMISSION_DENIED, Some("UserInfo"))
            .await;
    }

    // Get the requested user
    let target_user = match ctx.user_manager.get_user(requested_user_id).await {
        Some(u) => u,
        None => {
            // User not found - send response with None
            let response = ServerMessage::UserInfoResponse {
                user: None,
                error: Some("User not found".to_string()),
            };
            ctx.send_message(&response).await?;
            return Ok(());
        }
    };

    // Check if requesting user is admin (for detailed info)
    let requesting_account = match ctx
        .user_db
        .get_user_by_username(&requesting_user.username)
        .await
    {
        Ok(Some(acc)) => acc,
        _ => {
            return ctx
                .send_error_and_disconnect(ERR_DATABASE, Some("UserInfo"))
                .await;
        }
    };

    // Get target user's account info to check if they are admin
    let target_account = match ctx
        .user_db
        .get_user_by_username(&target_user.username)
        .await
    {
        Ok(Some(acc)) => acc,
        _ => {
            return ctx
                .send_error_and_disconnect(ERR_DATABASE, Some("UserInfo"))
                .await;
        }
    };

    // Build response - admins get extra fields
    let user_info = if requesting_account.is_admin {
        // Admin gets all fields including target user's admin status
        UserInfoDetailed {
            id: target_user.id,
            username: target_user.username.clone(),
            session_id: target_user.session_id.clone(),
            login_time: target_user.login_time,
            features: target_user.features.clone(),
            created_at: target_user.created_at,
            is_admin: Some(target_account.is_admin),
            address: Some(target_user.address.to_string()),
        }
    } else {
        // Non-admin gets filtered fields
        UserInfoDetailed {
            id: target_user.id,
            username: target_user.username.clone(),
            session_id: target_user.session_id.clone(),
            login_time: target_user.login_time,
            features: target_user.features.clone(),
            created_at: target_user.created_at,
            is_admin: None,
            address: None,
        }
    };

    let response = ServerMessage::UserInfoResponse {
        user: Some(user_info),
        error: None,
    };
    ctx.send_message(&response).await?;

    Ok(())
}
