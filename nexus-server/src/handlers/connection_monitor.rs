//! Handler for ConnectionMonitor command

use std::io;

use tokio::io::AsyncWrite;
use tracing::warn;

use nexus_common::protocol::{ConnectionInfo, ServerMessage};

use super::{HandlerContext, err_authentication, err_not_logged_in, err_permission_denied};
use crate::constants::{
    HANDLER_CONNECTION_MONITOR, LOG_CONN_MONITOR_NOT_LOGGED_IN, LOG_CONN_MONITOR_PERMISSION_DENIED,
};
use crate::db::Permission;

/// Handle ConnectionMonitor command
///
/// Returns a list of all active connections with their session info.
pub async fn handle_connection_monitor<W>(
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    // Verify authentication
    let Some(session_id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_CONN_MONITOR_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(
                &err_not_logged_in(ctx.locale),
                Some(HANDLER_CONNECTION_MONITOR),
            )
            .await;
    };

    // Get requesting user from session
    let requesting_user = match ctx.user_manager.get_user_by_session_id(session_id).await {
        Some(user) => user,
        None => {
            return ctx
                .send_error_and_disconnect(
                    &err_authentication(ctx.locale),
                    Some(HANDLER_CONNECTION_MONITOR),
                )
                .await;
        }
    };

    // Check connection_monitor permission
    if !requesting_user.has_permission(Permission::ConnectionMonitor) {
        warn!(user = %requesting_user.username, ip = %ctx.peer_addr, "{}", LOG_CONN_MONITOR_PERMISSION_DENIED);
        let response = ServerMessage::ConnectionMonitorResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            connections: None,
            transfers: None,
        };
        return ctx.send_message(&response).await;
    }

    // Get all active sessions from user manager
    let sessions = ctx.user_manager.get_all_users().await;

    let mut connections: Vec<ConnectionInfo> = sessions
        .into_iter()
        .map(|s| ConnectionInfo {
            nickname: s.nickname,
            username: s.username,
            ip: s.address.ip().to_string(),
            port: s.address.port(),
            login_time: s.login_time,
            is_admin: s.is_admin,
            is_shared: s.is_shared,
        })
        .collect();

    // Sort alphabetically by nickname
    connections.sort_by_key(|c| c.nickname.to_lowercase());

    // Get active transfers from registry
    let mut transfers: Vec<_> = ctx
        .transfer_registry
        .snapshot()
        .iter()
        .map(|t| t.to_transfer_info())
        .collect();

    // Sort transfers by nickname
    transfers.sort_by_key(|t| t.nickname.to_lowercase());

    let response = ServerMessage::ConnectionMonitorResponse {
        success: true,
        error: None,
        connections: Some(connections),
        transfers: Some(transfers),
    };
    ctx.send_message(&response).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::testing::{create_test_context, login_user, read_server_message};

    #[tokio::test]
    async fn test_connection_monitor_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_connection_monitor(None, &mut test_ctx.handler_context()).await;

        // Should fail with disconnect
        assert!(result.is_err(), "ConnectionMonitor should require login");
    }

    #[tokio::test]
    async fn test_connection_monitor_requires_permission() {
        let mut test_ctx = create_test_context().await;

        // Create non-admin user without connection_monitor permission
        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let result =
            handle_connection_monitor(Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok(), "Should send error response, not disconnect");

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::ConnectionMonitorResponse { success, error, .. } = response {
            assert!(!success);
            assert!(error.is_some());
        } else {
            panic!("Expected ConnectionMonitorResponse, got: {:?}", response);
        }
    }

    #[tokio::test]
    async fn test_connection_monitor_admin_can_view() {
        let mut test_ctx = create_test_context().await;

        // Create admin user
        let session_id = login_user(&mut test_ctx, "admin", "adminpass", &[], true).await;

        let result =
            handle_connection_monitor(Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::ConnectionMonitorResponse {
            success,
            connections,
            error,
            ..
        } = response
        {
            assert!(success, "Expected success, got error: {:?}", error);
            let connections = connections.unwrap();
            // Should have at least our admin session
            assert!(!connections.is_empty());

            // Find our admin connection
            let admin_conn = connections.iter().find(|c| c.username == "admin");
            assert!(admin_conn.is_some());
            let admin_conn = admin_conn.unwrap();
            assert_eq!(admin_conn.nickname, "admin");
            assert!(admin_conn.is_admin);
            assert!(!admin_conn.is_shared);
        } else {
            panic!("Expected ConnectionMonitorResponse, got: {:?}", response);
        }
    }

    #[tokio::test]
    async fn test_connection_monitor_with_permission() {
        let mut test_ctx = create_test_context().await;

        // Create user with only connection_monitor permission
        let session_id = login_user(
            &mut test_ctx,
            "moderator",
            "password",
            &[Permission::ConnectionMonitor],
            false,
        )
        .await;

        let result =
            handle_connection_monitor(Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::ConnectionMonitorResponse { success, .. } = response {
            assert!(success);
        } else {
            panic!("Expected ConnectionMonitorResponse, got: {:?}", response);
        }
    }

    #[tokio::test]
    async fn test_connection_monitor_shows_multiple_sessions() {
        let mut test_ctx = create_test_context().await;

        // Login multiple users
        login_user(&mut test_ctx, "alice", "password", &[], false).await;
        login_user(&mut test_ctx, "bob", "password", &[], false).await;
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result =
            handle_connection_monitor(Some(admin_session), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::ConnectionMonitorResponse {
            success,
            connections,
            error,
            ..
        } = response
        {
            assert!(success, "Expected success, got error: {:?}", error);
            let connections = connections.unwrap();
            assert_eq!(connections.len(), 3);

            // Verify all users are present
            assert!(connections.iter().any(|c| c.username == "alice"));
            assert!(connections.iter().any(|c| c.username == "bob"));
            assert!(connections.iter().any(|c| c.username == "admin"));
        } else {
            panic!("Expected ConnectionMonitorResponse, got: {:?}", response);
        }
    }

    #[tokio::test]
    async fn test_connection_monitor_sorted_alphabetically() {
        let mut test_ctx = create_test_context().await;

        // Login users in non-alphabetical order
        login_user(&mut test_ctx, "zach", "password", &[], false).await;
        login_user(&mut test_ctx, "alice", "password", &[], false).await;
        login_user(&mut test_ctx, "mike", "password", &[], false).await;
        let admin_session = login_user(&mut test_ctx, "admin", "password", &[], true).await;

        let result =
            handle_connection_monitor(Some(admin_session), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        if let ServerMessage::ConnectionMonitorResponse {
            success,
            connections,
            ..
        } = response
        {
            assert!(success);
            let connections = connections.unwrap();

            // Verify alphabetical order
            let nicknames: Vec<&str> = connections.iter().map(|c| c.nickname.as_str()).collect();
            assert_eq!(nicknames, vec!["admin", "alice", "mike", "zach"]);
        } else {
            panic!("Expected ConnectionMonitorResponse, got: {:?}", response);
        }
    }
}
