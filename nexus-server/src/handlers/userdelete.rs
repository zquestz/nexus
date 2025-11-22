//! Handler for UserDelete command

use super::{HandlerContext, ERR_NOT_LOGGED_IN, ERR_DATABASE, ERR_ACCOUNT_DELETED, ERR_CANNOT_DELETE_LAST_ADMIN};
use crate::db::Permission;
use nexus_common::protocol::ServerMessage;
use std::io;

/// Handle UserDelete command
pub async fn handle_userdelete(
    target_username: String,
    requesting_user_id: Option<u32>,
    ctx: &mut HandlerContext<'_>,
) -> io::Result<()> {
    // User must be logged in
    let Some(requesting_user_id) = requesting_user_id else {
        return ctx
            .send_error_and_disconnect(ERR_NOT_LOGGED_IN, Some("UserDelete"))
            .await;
    };

    // Get requesting user info from database to check permissions
    let requesting_user = match ctx.user_db.get_user_by_id(requesting_user_id as i64).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            return ctx
                .send_error_and_disconnect("Your user account was not found", Some("UserDelete"))
                .await;
        }
        Err(e) => {
            eprintln!("Database error getting requesting user: {}", e);
            return ctx
                .send_error_and_disconnect(ERR_DATABASE, Some("UserDelete"))
                .await;
        }
    };

    // Check if requesting user is admin OR has UserDelete permission
    let has_permission = requesting_user.is_admin
        || match ctx
            .user_db
            .has_permission(requesting_user_id as i64, Permission::UserDelete)
            .await
        {
            Ok(has_perm) => has_perm,
            Err(e) => {
                eprintln!("Database error checking permissions: {}", e);
                return ctx
                    .send_error_and_disconnect(ERR_DATABASE, Some("UserDelete"))
                    .await;
            }
        };

    if !has_permission {
        let response = ServerMessage::UserDeleteResponse {
            success: false,
            error: Some("You don't have permission to delete users".to_string()),
        };
        return ctx.send_message(&response).await;
    }

    // Get the target user to check if they exist and if they're an admin
    let target_user = match ctx.user_db.get_user_by_username(&target_username).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            let response = ServerMessage::UserDeleteResponse {
                success: false,
                error: Some("User not found".to_string()),
            };
            return ctx.send_message(&response).await;
        }
        Err(e) => {
            eprintln!("Database error getting target user: {}", e);
            return ctx
                .send_error_and_disconnect(ERR_DATABASE, Some("UserDelete"))
                .await;
        }
    };

    // If the user is currently connected, notify and disconnect them first
    let all_users = ctx.user_manager.get_all_users().await;
    let online_user = all_users.iter().find(|u| u.db_user_id == target_user.id);
    
    if let Some(online_user) = online_user {
        // Send error message to the user being deleted
        let disconnect_msg = ServerMessage::Error {
            message: ERR_ACCOUNT_DELETED.to_string(),
            command: None,
        };
        let _ = online_user.tx.send(disconnect_msg);
        
        // Remove them from UserManager
        let session_id = online_user.id;
        if let Some(removed_user) = ctx.user_manager.remove_user(session_id).await {
            // Broadcast disconnection to all other users
            ctx.user_manager
                .broadcast(ServerMessage::UserDisconnected {
                    user_id: session_id,
                    username: removed_user.username.clone(),
                })
                .await;
        }
    }

    // Delete the user from database (atomic operation that prevents deleting last admin)
    match ctx.user_db.delete_user(target_user.id).await {
        Ok(deleted) => {
            if deleted {
                // Send success response to the admin who deleted the user
                let response = ServerMessage::UserDeleteResponse {
                    success: true,
                    error: None,
                };
                ctx.send_message(&response).await
            } else {
                // Deletion was blocked (likely because they're the last admin)
                let response = ServerMessage::UserDeleteResponse {
                    success: false,
                    error: Some(ERR_CANNOT_DELETE_LAST_ADMIN.to_string()),
                };
                ctx.send_message(&response).await
            }
        }
        Err(e) => {
            eprintln!("Database error deleting user: {}", e);
            ctx.send_error_and_disconnect(ERR_DATABASE, Some("UserDelete"))
                .await
        }
    }
}