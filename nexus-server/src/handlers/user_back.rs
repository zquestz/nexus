//! Handler for UserBack command - clear away status

use std::io;

use tokio::io::AsyncWrite;

use nexus_common::protocol::ServerMessage;

use super::{HandlerContext, err_authentication, err_not_logged_in};
use crate::users::manager::UserManager;

/// Handle UserBack command - clear away status for all sessions of this user
pub async fn handle_user_back<W>(
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    // Verify authentication first
    let Some(session_id) = session_id else {
        eprintln!("UserBack request from {} without login", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("UserBack"))
            .await;
    };

    // Update away status for this session
    let Some(session) = ctx.user_manager.set_status(session_id, false, None).await else {
        return ctx
            .send_error_and_disconnect(&err_authentication(ctx.locale), Some("UserBack"))
            .await;
    };

    // Send success response
    let response = ServerMessage::UserBackResponse {
        success: true,
        error: None,
    };
    ctx.send_message(&response).await?;

    // Broadcast UserUpdated
    // For regular accounts with multiple sessions, use aggregated data with "latest login wins"
    // For shared accounts, each session is separate (use single session data)
    let user_info = if session.is_shared {
        // Shared account: use this session's data directly
        UserManager::build_user_info_from_session(&session)
    } else {
        // Regular account: aggregate all sessions, using "latest login wins" for avatar/away/status
        let all_sessions = ctx
            .user_manager
            .get_sessions_by_username(&session.username)
            .await;
        UserManager::build_aggregated_user_info(&all_sessions).expect("at least one session exists")
    };

    let user_updated = ServerMessage::UserUpdated {
        previous_username: session.username.clone(),
        user: user_info,
    };

    ctx.user_manager
        .broadcast_user_event(user_updated, None)
        .await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::testing::{create_test_context, login_user, read_server_message};
    use crate::handlers::user_away::handle_user_away;

    #[tokio::test]
    async fn test_userback_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_user_back(None, &mut test_ctx.handler_context()).await;

        assert!(result.is_err(), "Should disconnect unauthenticated user");
    }

    #[tokio::test]
    async fn test_userback_clears_away_status() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        // First set away
        let _ = handle_user_away(
            Some("grabbing lunch".to_string()),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_server_message(&mut test_ctx).await; // consume UserAwayResponse

        // Verify away is set
        let user = test_ctx
            .user_manager
            .get_user_by_session_id(session_id)
            .await
            .unwrap();
        assert!(user.is_away);
        assert_eq!(user.status, Some("grabbing lunch".to_string()));

        // Now clear with back
        let result = handle_user_back(Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserBackResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected UserBackResponse, got {:?}", response),
        }

        // Verify session was updated
        let user = test_ctx
            .user_manager
            .get_user_by_session_id(session_id)
            .await
            .unwrap();
        assert!(!user.is_away);
        assert!(user.status.is_none());
    }

    #[tokio::test]
    async fn test_userback_when_not_away() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        // Call back without being away (should still succeed)
        let result = handle_user_back(Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserBackResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected UserBackResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_userback_invalid_session() {
        let mut test_ctx = create_test_context().await;

        // Login to get a valid context, but use wrong session ID
        let _session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let result = handle_user_back(
            Some(999), // Invalid session ID
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_err(), "Should disconnect with invalid session");
    }
}
