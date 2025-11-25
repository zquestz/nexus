//! UserEdit message handler - Returns user details for editing

use super::{
    ERR_CANNOT_EDIT_SELF, ERR_DATABASE, ERR_NOT_LOGGED_IN, ERR_PERMISSION_DENIED,
    ERR_USER_NOT_FOUND, HandlerContext,
};
use crate::db::Permission;
use nexus_common::protocol::ServerMessage;
use std::io;

/// Handle a user edit request (returns user details for editing)
pub async fn handle_useredit(
    username: String,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_>,
) -> io::Result<()> {
    // Verify authentication
    let requesting_session_id = match session_id {
        Some(id) => id,
        None => {
            eprintln!("UserEdit request from {} without login", ctx.peer_addr);
            return ctx
                .send_error_and_disconnect(ERR_NOT_LOGGED_IN, Some("UserEdit"))
                .await;
        }
    };

    // Get requesting user from session
    let requesting_user = match ctx
        .user_manager
        .get_user_by_session_id(requesting_session_id)
        .await
    {
        Some(u) => u,
        None => {
            eprintln!("UserEdit request from unknown user {}", ctx.peer_addr);
            return ctx
                .send_error_and_disconnect(ERR_DATABASE, Some("UserEdit"))
                .await;
        }
    };

    // Check UserEdit permission
    let has_permission = match ctx
        .db
        .users
        .has_permission(requesting_user.db_user_id, Permission::UserEdit)
        .await
    {
        Ok(has) => has,
        Err(e) => {
            eprintln!("UserEdit permission check error: {}", e);
            return ctx
                .send_error_and_disconnect(ERR_DATABASE, Some("UserEdit"))
                .await;
        }
    };

    if !has_permission {
        eprintln!(
            "UserEdit from {} (user: {}) without permission",
            ctx.peer_addr, requesting_user.username
        );
        return ctx
            .send_error(ERR_PERMISSION_DENIED, Some("UserEdit"))
            .await;
    }

    // Prevent self-editing
    if requesting_user.username.eq_ignore_ascii_case(&username) {
        eprintln!(
            "UserEdit from {} (user: {}) attempting to edit themselves",
            ctx.peer_addr, requesting_user.username
        );
        return ctx.send_error(ERR_CANNOT_EDIT_SELF, Some("UserEdit")).await;
    }

    // Look up target user in database
    let target_user = match ctx.db.users.get_user_by_username(&username).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            eprintln!("UserEdit request for non-existent user: {}", username);
            return ctx.send_error(ERR_USER_NOT_FOUND, Some("UserEdit")).await;
        }
        Err(e) => {
            eprintln!("Database error getting user: {}", e);
            return ctx
                .send_error_and_disconnect(ERR_DATABASE, Some("UserEdit"))
                .await;
        }
    };

    // Fetch user permissions for response
    let user_permissions = match ctx.db.users.get_user_permissions(target_user.id).await {
        Ok(perms) => perms,
        Err(e) => {
            eprintln!("Database error getting permissions: {}", e);
            return ctx
                .send_error_and_disconnect(ERR_DATABASE, Some("UserEdit"))
                .await;
        }
    };

    // Convert permissions to protocol format
    let permissions: Vec<String> = user_permissions
        .to_vec()
        .iter()
        .map(|p| p.as_str().to_string())
        .collect();

    // Send user details for editing
    let response = ServerMessage::UserEditResponse {
        username: target_user.username,
        is_admin: target_user.is_admin,
        permissions,
    };

    ctx.send_message(&response).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::handlers::testing::{create_test_context, login_user, read_server_message};

    #[tokio::test]
    async fn test_useredit_get_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_useredit(
            "alice".to_string(),
            None, // Not logged in
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_useredit_get_requires_permission() {
        let mut test_ctx = create_test_context().await;

        // Login as user without UserEdit permission
        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        // Create another user to edit
        test_ctx
            .db
            .users
            .create_user("bob", "hash", false, &db::Permissions::new())
            .await
            .unwrap();

        let result = handle_useredit(
            "bob".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::Error { message, .. } => {
                assert_eq!(message, ERR_PERMISSION_DENIED);
            }
            _ => panic!("Expected Error message"),
        }
    }

    #[tokio::test]
    async fn test_useredit_get_user_not_found() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result = handle_useredit(
            "nonexistent".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::Error { message, .. } => {
                assert_eq!(message, ERR_USER_NOT_FOUND);
            }
            _ => panic!("Expected Error message"),
        }
    }

    #[tokio::test]
    async fn test_useredit_get_returns_user_details() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create a user with specific permissions
        let mut perms = db::Permissions::new();
        perms.permissions.insert(db::Permission::UserList);
        perms.permissions.insert(db::Permission::ChatSend);

        test_ctx
            .db
            .users
            .create_user("bob", "hash", false, &perms)
            .await
            .unwrap();

        let result = handle_useredit(
            "bob".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserEditResponse {
                username,
                is_admin,
                permissions,
            } => {
                assert_eq!(username, "bob");
                assert!(!is_admin);
                assert!(permissions.contains(&"user_list".to_string()));
                assert!(permissions.contains(&"chat_send".to_string()));
                assert_eq!(permissions.len(), 2);
            }
            _ => panic!("Expected UserEditResponse"),
        }
    }

    #[tokio::test]
    async fn test_useredit_get_admin_user() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Create another admin
        test_ctx
            .db
            .users
            .create_user("admin2", "hash", true, &db::Permissions::new())
            .await
            .unwrap();

        let result = handle_useredit(
            "admin2".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserEditResponse {
                username,
                is_admin,
                permissions,
            } => {
                assert_eq!(username, "admin2");
                assert!(is_admin);
                // Admins have no stored permissions (they get all automatically)
                assert_eq!(permissions.len(), 0);
            }
            _ => panic!("Expected UserEditResponse"),
        }
    }

    #[tokio::test]
    async fn test_useredit_get_with_permission() {
        let mut test_ctx = create_test_context().await;

        // Login as user with UserEdit permission
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::UserEdit],
            false,
        )
        .await;

        // Create another user
        test_ctx
            .db
            .users
            .create_user("bob", "hash", false, &db::Permissions::new())
            .await
            .unwrap();

        let result = handle_useredit(
            "bob".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::UserEditResponse { username, .. } => {
                assert_eq!(username, "bob");
            }
            _ => panic!("Expected UserEditResponse"),
        }
    }

    #[tokio::test]
    async fn test_useredit_cannot_edit_self() {
        let mut test_ctx = create_test_context().await;

        // Login as admin
        let session_id = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        // Try to edit self
        let result = handle_useredit(
            "admin".to_string(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let response = read_server_message(&mut test_ctx.client).await;
        match response {
            ServerMessage::Error { message, .. } => {
                assert_eq!(message, ERR_CANNOT_EDIT_SELF);
            }
            _ => panic!("Expected Error message"),
        }
    }
}
