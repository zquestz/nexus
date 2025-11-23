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
        eprintln!(
            "UserList from {} (user: {}) without permission",
            ctx.peer_addr, user.username
        );
        return ctx
            .send_error(ERR_PERMISSION_DENIED, Some("UserList"))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::handlers::testing::create_test_context;

    #[tokio::test]
    async fn test_userlist_requires_permission() {
        let mut test_ctx = create_test_context().await;

        // Create user WITHOUT userlist permission
        let password = "password";
        let hashed = db::hash_password(password).unwrap();
        let user = test_ctx
            .user_db
            .create_user("alice", &hashed, false, &db::Permissions::new())
            .await
            .unwrap();

        // Add user to UserManager
        let session_id = test_ctx
            .user_manager
            .add_user(
                user.id,
                "alice".to_string(),
                test_ctx.peer_addr,
                user.created_at,
                test_ctx.tx.clone(),
                vec![],
            )
            .await;

        // Try to get user list without permission
        let result = handle_userlist(Some(session_id), &mut test_ctx.handler_context()).await;

        // Should succeed (send error but not disconnect)
        assert!(result.is_ok(), "Should send error message but not disconnect");
    }

    #[tokio::test]
    async fn test_userlist_with_permission() {
        let mut test_ctx = create_test_context().await;

        // Create user WITH userlist permission
        let password = "password";
        let hashed = db::hash_password(password).unwrap();
        let mut perms = db::Permissions::new();
        use std::collections::HashSet;
        perms.permissions = {
            let mut set = HashSet::new();
            set.insert(db::Permission::UserList);
            set
        };
        let user = test_ctx
            .user_db
            .create_user("alice", &hashed, false, &perms)
            .await
            .unwrap();

        // Add user to UserManager
        let session_id = test_ctx
            .user_manager
            .add_user(
                user.id,
                "alice".to_string(),
                test_ctx.peer_addr,
                user.created_at,
                test_ctx.tx.clone(),
                vec![],
            )
            .await;

        // Get user list with permission
        let result = handle_userlist(Some(session_id), &mut test_ctx.handler_context()).await;

        // Should succeed
        assert!(result.is_ok(), "Valid userlist request should succeed");
    }
}
