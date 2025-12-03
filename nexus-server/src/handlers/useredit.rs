//! UserEdit message handler - Returns user details for editing

use std::io;

use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{self, UsernameError};

#[cfg(test)]
use super::testing::DEFAULT_TEST_LOCALE;
use super::{
    HandlerContext, err_authentication, err_cannot_edit_self, err_database, err_not_logged_in,
    err_permission_denied, err_user_not_found, err_username_empty, err_username_invalid,
    err_username_too_long,
};
use crate::db::Permission;

/// Handle a user edit request (returns user details for editing)
pub async fn handle_useredit(
    username: String,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_>,
) -> io::Result<()> {
    // Verify authentication first (before revealing validation errors to unauthenticated users)
    let Some(requesting_session_id) = session_id else {
        eprintln!("UserEdit request from {} without login", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("UserEdit"))
            .await;
    };

    // Validate username format
    if let Err(e) = validators::validate_username(&username) {
        let error_msg = match e {
            UsernameError::Empty => err_username_empty(ctx.locale),
            UsernameError::TooLong => {
                err_username_too_long(ctx.locale, validators::MAX_USERNAME_LENGTH)
            }
            UsernameError::InvalidCharacters => err_username_invalid(ctx.locale),
        };
        let response = ServerMessage::UserEditResponse {
            success: false,
            error: Some(error_msg),
            username: None,
            is_admin: None,
            enabled: None,
            permissions: None,
        };
        return ctx.send_message(&response).await;
    }

    // Get requesting user from session
    let requesting_user = match ctx
        .user_manager
        .get_user_by_session_id(requesting_session_id)
        .await
    {
        Some(u) => u,
        None => {
            return ctx
                .send_error_and_disconnect(&err_authentication(ctx.locale), Some("UserEdit"))
                .await;
        }
    };

    // Prevent self-editing (cheap check before DB query)
    if requesting_user.username.eq_ignore_ascii_case(&username) {
        let response = ServerMessage::UserEditResponse {
            success: false,
            error: Some(err_cannot_edit_self(ctx.locale)),
            username: None,
            is_admin: None,
            enabled: None,
            permissions: None,
        };
        return ctx.send_message(&response).await;
    }

    // Check UserEdit permission (uses cached permissions, admin bypass built-in)
    if !requesting_user.has_permission(Permission::UserEdit) {
        eprintln!(
            "UserEdit from {} (user: {}) without permission",
            ctx.peer_addr, requesting_user.username
        );
        let response = ServerMessage::UserEditResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            username: None,
            is_admin: None,
            enabled: None,
            permissions: None,
        };
        return ctx.send_message(&response).await;
    }

    // Look up target user in database
    let target_user = match ctx.db.users.get_user_by_username(&username).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            let response = ServerMessage::UserEditResponse {
                success: false,
                error: Some(err_user_not_found(ctx.locale, &username)),
                username: None,
                is_admin: None,
                enabled: None,
                permissions: None,
            };
            return ctx.send_message(&response).await;
        }
        Err(e) => {
            eprintln!("Database error getting user: {}", e);
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some("UserEdit"))
                .await;
        }
    };

    // Fetch user permissions for response
    let user_permissions = match ctx.db.users.get_user_permissions(target_user.id).await {
        Ok(perms) => perms,
        Err(e) => {
            eprintln!("Database error getting permissions: {}", e);
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some("UserEdit"))
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
        success: true,
        error: None,
        username: Some(target_user.username),
        is_admin: Some(target_user.is_admin),
        enabled: Some(target_user.enabled),
        permissions: Some(permissions),
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
            .create_user("bob", "hash", false, true, &db::Permissions::new())
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
            ServerMessage::UserEditResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_permission_denied(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected UserEditResponse with error"),
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
            ServerMessage::UserEditResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(
                    error,
                    Some(err_user_not_found(DEFAULT_TEST_LOCALE, "nonexistent"))
                );
            }
            _ => panic!("Expected UserEditResponse with error"),
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
            .create_user("bob", "hash", false, true, &perms)
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
                success,
                error,
                username,
                is_admin,
                enabled: _,
                permissions,
            } => {
                assert!(success);
                assert!(error.is_none());
                assert_eq!(username.as_deref(), Some("bob"));
                assert_eq!(is_admin, Some(false));
                assert!(
                    permissions
                        .as_ref()
                        .unwrap()
                        .contains(&"user_list".to_string())
                );
                assert!(
                    permissions
                        .as_ref()
                        .unwrap()
                        .contains(&"chat_send".to_string())
                );
                assert_eq!(permissions.as_ref().unwrap().len(), 2);
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
            .create_user("admin2", "hash", true, true, &db::Permissions::new())
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
                success,
                error,
                username,
                is_admin,
                enabled,
                permissions,
            } => {
                assert!(success);
                assert!(error.is_none());
                assert_eq!(username.as_deref(), Some("admin2"));
                assert_eq!(is_admin, Some(true));
                assert_eq!(enabled, Some(true));
                // Admins have no stored permissions (they get all automatically)
                assert_eq!(permissions.as_ref().unwrap().len(), 0);
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
            .create_user("bob", "hash", false, true, &db::Permissions::new())
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
                success, username, ..
            } => {
                assert!(success);
                assert_eq!(username.as_deref(), Some("bob"));
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
            ServerMessage::UserEditResponse { success, error, .. } => {
                assert!(!success);
                assert_eq!(error, Some(err_cannot_edit_self(DEFAULT_TEST_LOCALE)));
            }
            _ => panic!("Expected UserEditResponse with error"),
        }
    }
}
