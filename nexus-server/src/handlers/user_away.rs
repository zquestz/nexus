//! Handler for UserAway command

use std::io;

use tokio::io::AsyncWrite;
use tracing::warn;

use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{self, StatusError};

use super::{
    HandlerContext, err_not_logged_in, err_status_contains_newlines, err_status_invalid_characters,
    err_status_too_long,
};
use crate::constants::{HANDLER_USER_AWAY, LOG_USER_AWAY_NOT_LOGGED_IN};
use crate::users::manager::UserManager;

enum AwayOutcome {
    Disconnect,
    Send(Box<ServerMessage>),
}

/// Set away status for this session.
pub async fn handle_user_away<W>(
    message: Option<String>,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let Some(session_id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_USER_AWAY_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_USER_AWAY))
            .await;
    };

    let message = message.filter(|msg| !msg.trim().is_empty());

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

    let outcome = 'locked: {
        let _user_state = ctx.user_manager.read_user_state().await;

        let Some(session) = ctx
            .user_manager
            .set_status(session_id, true, message.clone())
            .await
        else {
            break 'locked AwayOutcome::Disconnect;
        };

        // Broadcast UserUpdated. Shared accounts broadcast this session directly;
        // regular accounts aggregate across all their sessions.
        let user_info = if session.is_shared {
            UserManager::build_user_info_from_session(&session)
        } else {
            let all_sessions = ctx
                .user_manager
                .get_sessions_by_username(&session.username)
                .await;
            let Some(user_info) = UserManager::build_aggregated_user_info(&all_sessions) else {
                break 'locked AwayOutcome::Send(Box::new(ServerMessage::UserAwayResponse {
                    success: true,
                    error: None,
                }));
            };
            user_info
        };

        let user_updated = ServerMessage::UserUpdated {
            previous_username: session.username.clone(),
            user: user_info,
        };

        ctx.user_manager
            .broadcast_user_event(user_updated, None)
            .await;

        AwayOutcome::Send(Box::new(ServerMessage::UserAwayResponse {
            success: true,
            error: None,
        }))
    };

    match outcome {
        AwayOutcome::Disconnect => {
            ctx.send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_USER_AWAY))
                .await
        }
        AwayOutcome::Send(response) => ctx.send_message(&response).await,
    }
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
    async fn test_useraway_empty_string_sets_away_without_message() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let result = handle_user_away(
            Some("   ".to_string()),
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
