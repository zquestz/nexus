//! UserList message handler

use super::{
    ERR_AUTHENTICATION, ERR_DATABASE, ERR_NOT_LOGGED_IN, ERR_PERMISSION_DENIED, HandlerContext,
};
use crate::db::Permission;
use nexus_common::protocol::{ServerMessage, UserInfo};
use std::io;

/// Handle a userlist request from the client
pub async fn handle_userlist(
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_>,
) -> io::Result<()> {
    let id = match session_id {
        Some(id) => id,
        None => {
            eprintln!("UserList request from {} without login", ctx.peer_addr);
            return ctx
                .send_error_and_disconnect(ERR_NOT_LOGGED_IN, Some("UserList"))
                .await;
        }
    };

    // Get user and check permission
    let user = match ctx.user_manager.get_user(id).await {
        Some(u) => u,
        None => {
            eprintln!("UserList request from unknown user {}", ctx.peer_addr);
            return ctx
                .send_error_and_disconnect(ERR_AUTHENTICATION, Some("UserList"))
                .await;
        }
    };

    let has_perm = match ctx
        .user_db
        .has_permission(user.db_user_id, Permission::UserList)
        .await
    {
        Ok(has) => has,
        Err(e) => {
            eprintln!("UserList permission check error: {}", e);
            return ctx
                .send_error_and_disconnect(ERR_DATABASE, Some("UserList"))
                .await;
        }
    };

    if !has_perm {
        eprintln!("UserList request from {} without permission", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(ERR_PERMISSION_DENIED, Some("UserList"))
            .await;
    }

    // Get all users from the manager
    let all_users = ctx.user_manager.get_all_users().await;
    let user_infos: Vec<UserInfo> = all_users
        .into_iter()
        .map(|u| UserInfo {
            session_id: u.session_id,
            username: u.username,
            login_time: u.login_time,
        })
        .collect();

    let response = ServerMessage::UserListResponse { users: user_infos };
    ctx.send_message(&response).await?;

    Ok(())
}
