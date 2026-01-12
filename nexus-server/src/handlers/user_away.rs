//! Handler for UserAway command - set away status

use std::io;

use tokio::io::AsyncWrite;

use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{self, StatusError};

use super::{
    HandlerContext, err_authentication, err_not_logged_in, err_status_contains_newlines,
    err_status_invalid_characters, err_status_too_long,
};
use crate::users::manager::UserManager;

/// Handle UserAway command - set away status for all sessions of this user
pub async fn handle_user_away<W>(
    message: Option<String>,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    // Verify authentication first
    let Some(session_id) = session_id else {
        eprintln!("UserAway request from {} without login", ctx.peer_addr);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("UserAway"))
            .await;
    };

    // Validate status message if provided
    if let Some(ref msg) = message
        && let Err(e) = validators::validate_status(msg)
    {
        let error_msg = match e {
            StatusError::TooLong => err_status_too_long(ctx.locale, validators::MAX_STATUS_LENGTH),
            StatusError::ContainsNewlines => err_status_contains_newlines(ctx.locale),
            StatusError::InvalidCharacters => err_status_invalid_characters(ctx.locale),
        };
        let response = ServerMessage::UserAwayResponse {
            success: false,
            error: Some(error_msg),
        };
        return ctx.send_message(&response).await;
    }

    // Update away status for this session
    let status = message.clone();
    let Some(session) = ctx
        .user_manager
        .set_status(session_id, true, status.clone())
        .await
    else {
        return ctx
            .send_error_and_disconnect(&err_authentication(ctx.locale), Some("UserAway"))
            .await;
    };

    // Send success response
    let response = ServerMessage::UserAwayResponse {
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

    #[tokio::test]
    async fn test_useraway_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_user_away(
            Some("grabbing lunch".to_string()),
            None,
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_err(), "Should disconnect unauthenticated user");
    }

    #[tokio::test]
    async fn test_useraway_with_message() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let result = handle_user_away(
            Some("grabbing lunch".to_string()),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserAwayResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected UserAwayResponse, got {:?}", response),
        }

        // Verify session was updated
        let user = test_ctx
            .user_manager
            .get_user_by_session_id(session_id)
            .await
            .unwrap();
        assert!(user.is_away);
        assert_eq!(user.status, Some("grabbing lunch".to_string()));
    }

    #[tokio::test]
    async fn test_useraway_without_message() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let result =
            handle_user_away(None, Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserAwayResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected UserAwayResponse, got {:?}", response),
        }

        // Verify session was updated
        let user = test_ctx
            .user_manager
            .get_user_by_session_id(session_id)
            .await
            .unwrap();
        assert!(user.is_away);
        assert!(user.status.is_none());
    }

    #[tokio::test]
    async fn test_userstatus_too_long() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let long_message = "x".repeat(validators::MAX_STATUS_LENGTH + 1);

        let result = handle_user_away(
            Some(long_message),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserAwayResponse { success, error } => {
                assert!(!success);
                assert!(error.is_some());
                assert!(error.unwrap().contains("long"));
            }
            _ => panic!("Expected UserAwayResponse, got {:?}", response),
        }

        // Verify session was NOT updated
        let user = test_ctx
            .user_manager
            .get_user_by_session_id(session_id)
            .await
            .unwrap();
        assert!(!user.is_away);
    }

    #[tokio::test]
    async fn test_userstatus_contains_newlines() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let result = handle_user_away(
            Some("line1\nline2".to_string()),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserAwayResponse { success, error } => {
                assert!(!success);
                assert!(error.is_some());
            }
            _ => panic!("Expected UserAwayResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_useraway_invalid_session() {
        let mut test_ctx = create_test_context().await;

        // Login to get a valid context, but use wrong session ID
        let _session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let result = handle_user_away(
            Some("away".to_string()),
            Some(999), // Invalid session ID
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_err(), "Should disconnect with invalid session");
    }
}
