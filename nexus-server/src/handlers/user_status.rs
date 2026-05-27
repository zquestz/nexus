//! Handler for UserStatus command

use std::io;

use tokio::io::AsyncWrite;
use tracing::warn;

use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{self, StatusError};

use super::{
    HandlerContext, err_not_logged_in, err_status_contains_newlines, err_status_invalid_characters,
    err_status_too_long,
};
use crate::constants::{HANDLER_USER_STATUS, LOG_USER_STATUS_NOT_LOGGED_IN};
use crate::users::manager::UserManager;

/// Set status message and always clear the away flag (unlike `/away`).
/// `/status` with no message clears status too (same as `/back`).
pub async fn handle_user_status<W>(
    status: Option<String>,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let Some(session_id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_USER_STATUS_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_USER_STATUS))
            .await;
    };

    if let Some(ref msg) = status
        && let Err(e) = validators::validate_status(msg)
    {
        let error_msg = match e {
            StatusError::TooLong => err_status_too_long(ctx.locale, validators::MAX_STATUS_LENGTH),
            StatusError::ContainsNewlines => err_status_contains_newlines(ctx.locale),
            StatusError::InvalidCharacters => err_status_invalid_characters(ctx.locale),
        };
        let response = ServerMessage::UserStatusResponse {
            success: false,
            error: Some(error_msg),
        };
        return ctx.send_message(&response).await;
    }

    let Some(_) = ctx
        .user_manager
        .set_status(session_id, false, status.clone())
        .await
    else {
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(HANDLER_USER_STATUS))
            .await;
    };

    let response = ServerMessage::UserStatusResponse {
        success: true,
        error: None,
    };
    ctx.send_message(&response).await?;

    // Re-fetch under user-state read lock so a concurrent rename cannot cause us
    // to broadcast stale identity fields after it has committed. The lock is acquired
    // after the socket write so no slow-client back-pressure holds the lock.
    let _user_state = ctx.user_manager.read_user_state().await;
    let Some(session) = ctx.user_manager.get_user_by_session_id(session_id).await else {
        return Ok(()); // concurrent disconnect
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
            return Ok(());
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

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::testing::{create_test_context, login_user, read_server_message};
    use crate::handlers::user_away::handle_user_away;

    #[tokio::test]
    async fn test_userstatus_requires_login() {
        let mut test_ctx = create_test_context().await;

        let result = handle_user_status(
            Some("working on project".to_string()),
            None,
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_err(), "Should disconnect unauthenticated user");
    }

    #[tokio::test]
    async fn test_userstatus_sets_status() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let result = handle_user_status(
            Some("working on project".to_string()),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserStatusResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected UserStatusResponse, got {:?}", response),
        }

        // Verify session was updated
        let user = test_ctx
            .user_manager
            .get_user_by_session_id(session_id)
            .await
            .unwrap();
        assert!(!user.is_away); // Should not change away status
        assert_eq!(user.status, Some("working on project".to_string()));
    }

    #[tokio::test]
    async fn test_userstatus_clears_status() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        // First set a status
        let _ = handle_user_status(
            Some("working".to_string()),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        let _ = read_server_message(&mut test_ctx).await; // consume response

        // Now clear it
        let result =
            handle_user_status(None, Some(session_id), &mut test_ctx.handler_context()).await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserStatusResponse { success, error } => {
                assert!(success);
                assert!(error.is_none());
            }
            _ => panic!("Expected UserStatusResponse, got {:?}", response),
        }

        // Verify status was cleared
        let user = test_ctx
            .user_manager
            .get_user_by_session_id(session_id)
            .await
            .unwrap();
        assert!(user.status.is_none());
    }

    #[tokio::test]
    async fn test_userstatus_clears_away_status() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        // First set away
        let _ = handle_user_away(
            Some("lunch".to_string()),
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
        assert_eq!(user.status, Some("lunch".to_string()));

        // Now set status (should clear away flag and change status)
        let result = handle_user_status(
            Some("in a meeting".to_string()),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());
        let _ = read_server_message(&mut test_ctx).await; // consume response

        // Verify away status is cleared and status changed
        let user = test_ctx
            .user_manager
            .get_user_by_session_id(session_id)
            .await
            .unwrap();
        assert!(!user.is_away, "Away status should be cleared");
        assert_eq!(user.status, Some("in a meeting".to_string()));
    }

    #[tokio::test]
    async fn test_userstatus_message_too_long() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let long_message = "x".repeat(validators::MAX_STATUS_LENGTH + 1);

        let result = handle_user_status(
            Some(long_message),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserStatusResponse { success, error } => {
                assert!(!success);
                assert!(error.is_some());
                assert!(error.unwrap().contains("long"));
            }
            _ => panic!("Expected UserStatusResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_userstatus_message_contains_newlines() {
        let mut test_ctx = create_test_context().await;

        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let result = handle_user_status(
            Some("line1\nline2".to_string()),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_ok());

        let response = read_server_message(&mut test_ctx).await;
        match response {
            ServerMessage::UserStatusResponse { success, error } => {
                assert!(!success);
                assert!(error.is_some());
            }
            _ => panic!("Expected UserStatusResponse, got {:?}", response),
        }
    }

    #[tokio::test]
    async fn test_userstatus_invalid_session() {
        let mut test_ctx = create_test_context().await;

        // Login to get a valid context, but use wrong session ID
        let _session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let result = handle_user_status(
            Some("status".to_string()),
            Some(999), // Invalid session ID
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(result.is_err(), "Should disconnect with invalid session");
    }
}
