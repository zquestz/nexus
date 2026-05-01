//! TrackerCreate message handler — adds a new tracker to the
//! server's publisher list.

use std::io;

use nexus_common::fingerprint::is_canonical_fingerprint;
use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{
    MAX_PASSWORD_LENGTH, MAX_PUBLIC_ADDRESS_LENGTH, MAX_TRACKER_NAME_LENGTH, PublicAddressError,
    TrackerNameError, validate_public_address, validate_tracker_name,
};
use tokio::io::AsyncWrite;
use tracing::{error, info, warn};

use super::{
    HandlerContext, err_authentication, err_database, err_not_logged_in, err_permission_denied,
    err_tracker_address_invalid, err_tracker_address_too_long, err_tracker_endpoint_duplicate,
    err_tracker_fingerprint_invalid, err_tracker_name_duplicate, err_tracker_name_invalid,
    err_tracker_name_too_long, err_tracker_password_too_long, err_tracker_port_invalid,
};
use crate::constants::{
    LOG_TRACKER_CREATE_DB_ERROR, LOG_TRACKER_CREATE_NOT_LOGGED_IN,
    LOG_TRACKER_CREATE_PERMISSION_DENIED, LOG_TRACKER_CREATE_SUCCESS,
};
use crate::db::{CreateTrackerParams, Permission, TrackerDbError};

/// Fields for `handle_tracker_create`, mirroring the
/// `ClientMessage::TrackerCreate` variant. Bundled into a struct so
/// the handler signature stays under the workspace-wide
/// too-many-arguments limit.
pub struct TrackerCreateRequest {
    pub address: String,
    pub port: u16,
    pub fingerprint: Option<String>,
    pub password: Option<String>,
    pub name: String,
    pub enabled: bool,
}

/// Handle a `TrackerCreate { ... }` request. Requires the
/// `tracker_create` permission. Validates inputs, inserts the row,
/// and spawns a publisher task for it (or skips spawn if disabled).
pub async fn handle_tracker_create<W>(
    request: TrackerCreateRequest,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let TrackerCreateRequest {
        address,
        port,
        fingerprint,
        password,
        name,
        enabled,
    } = request;

    let Some(session_id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_TRACKER_CREATE_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("TrackerCreate"))
            .await;
    };

    let requesting_user = match ctx.user_manager.get_user_by_session_id(session_id).await {
        Some(u) => u,
        None => {
            return ctx
                .send_error_and_disconnect(&err_authentication(ctx.locale), Some("TrackerCreate"))
                .await;
        }
    };

    if !requesting_user.has_permission(Permission::TrackerCreate) {
        warn!(
            user = %requesting_user.username,
            ip = %ctx.peer_addr,
            "{}", LOG_TRACKER_CREATE_PERMISSION_DENIED
        );
        let response = ServerMessage::TrackerCreateResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            id: None,
            name: None,
        };
        return ctx.send_message(&response).await;
    }

    // Normalize empty-string password to None at the protocol boundary.
    let password = password.filter(|s| !s.is_empty());

    // Validate inputs. First failure produces a typed response.
    if let Some(err) = validate_inputs(
        ctx,
        &address,
        port,
        fingerprint.as_deref(),
        password.as_deref(),
        &name,
    ) {
        return ctx.send_message(&err).await;
    }

    // Insert into the DB.
    let result = ctx
        .db
        .trackers
        .create(CreateTrackerParams {
            address: &address,
            port,
            fingerprint: fingerprint.as_deref(),
            password: password.as_deref(),
            name: &name,
            enabled,
        })
        .await;

    let record = match result {
        Ok(r) => r,
        Err(TrackerDbError::EndpointDuplicate) => {
            let response = ServerMessage::TrackerCreateResponse {
                success: false,
                error: Some(err_tracker_endpoint_duplicate(ctx.locale)),
                id: None,
                name: None,
            };
            return ctx.send_message(&response).await;
        }
        Err(TrackerDbError::NameDuplicate) => {
            let response = ServerMessage::TrackerCreateResponse {
                success: false,
                error: Some(err_tracker_name_duplicate(ctx.locale)),
                id: None,
                name: None,
            };
            return ctx.send_message(&response).await;
        }
        Err(TrackerDbError::Other(e)) => {
            error!(
                user = %requesting_user.username,
                ip = %ctx.peer_addr,
                err = %e,
                "{}", LOG_TRACKER_CREATE_DB_ERROR
            );
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some("TrackerCreate"))
                .await;
        }
    };

    info!(
        user = %requesting_user.username,
        ip = %ctx.peer_addr,
        id = record.id,
        name = %record.name,
        "{}", LOG_TRACKER_CREATE_SUCCESS
    );

    // Spawn a publisher task for the new row. No-op if disabled.
    ctx.tracker_manager.spawn(record.clone());

    let response = ServerMessage::TrackerCreateResponse {
        success: true,
        error: None,
        id: Some(record.id),
        name: Some(record.name),
    };
    ctx.send_message(&response).await
}

/// Run all per-field validation. Returns `Some(response)` with the
/// first failing message, or `None` if everything passes.
fn validate_inputs<W>(
    ctx: &HandlerContext<'_, W>,
    address: &str,
    port: u16,
    fingerprint: Option<&str>,
    password: Option<&str>,
    name: &str,
) -> Option<ServerMessage>
where
    W: AsyncWrite + Unpin,
{
    if let Err(e) = validate_public_address(address) {
        let error = match e {
            PublicAddressError::TooLong => {
                err_tracker_address_too_long(ctx.locale, MAX_PUBLIC_ADDRESS_LENGTH)
            }
            _ => err_tracker_address_invalid(ctx.locale),
        };
        return Some(reject_create(error));
    }

    if port == 0 {
        return Some(reject_create(err_tracker_port_invalid(ctx.locale)));
    }

    if let Some(fp) = fingerprint
        && !is_canonical_fingerprint(fp)
    {
        return Some(reject_create(err_tracker_fingerprint_invalid(ctx.locale)));
    }

    if let Some(pw) = password
        && pw.len() > MAX_PASSWORD_LENGTH
    {
        return Some(reject_create(err_tracker_password_too_long(
            ctx.locale,
            MAX_PASSWORD_LENGTH,
        )));
    }

    if let Err(e) = validate_tracker_name(name) {
        let error = match e {
            TrackerNameError::TooLong => {
                err_tracker_name_too_long(ctx.locale, MAX_TRACKER_NAME_LENGTH)
            }
            _ => err_tracker_name_invalid(ctx.locale),
        };
        return Some(reject_create(error));
    }

    None
}

fn reject_create(error: String) -> ServerMessage {
    ServerMessage::TrackerCreateResponse {
        success: false,
        error: Some(error),
        id: None,
        name: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::handlers::testing::{create_test_context, login_user, read_server_message};

    fn valid_request() -> TrackerCreateRequest {
        TrackerCreateRequest {
            address: "tracker.example.com".to_string(),
            port: 7510,
            fingerprint: None,
            password: None,
            name: "Public".to_string(),
            enabled: true,
        }
    }

    #[tokio::test]
    async fn requires_login() {
        let mut test_ctx = create_test_context().await;
        let result =
            handle_tracker_create(valid_request(), None, &mut test_ctx.handler_context()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn requires_permission() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let result = handle_tracker_create(
            valid_request(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerCreateResponse { success, .. } => assert!(!success),
            other => panic!("Expected TrackerCreateResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn happy_path_inserts_and_returns_id() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerCreate],
            false,
        )
        .await;

        let request = valid_request();
        let expected_name = request.name.clone();
        let result =
            handle_tracker_create(request, Some(session_id), &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerCreateResponse {
                success,
                id,
                name,
                error,
            } => {
                assert!(success, "expected success, got error: {error:?}");
                assert!(id.is_some());
                assert_eq!(name.as_deref(), Some(expected_name.as_str()));
            }
            other => panic!("Expected TrackerCreateResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn rejects_bad_address() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerCreate],
            false,
        )
        .await;

        // Address with embedded port — rejected by validate_public_address.
        let result = handle_tracker_create(
            TrackerCreateRequest {
                address: "tracker.example.com:7500".to_string(),
                ..valid_request()
            },
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerCreateResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            other => panic!("Expected TrackerCreateResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn rejects_zero_port() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerCreate],
            false,
        )
        .await;

        let result = handle_tracker_create(
            TrackerCreateRequest {
                port: 0,
                ..valid_request()
            },
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerCreateResponse { success, .. } => assert!(!success),
            other => panic!("Expected TrackerCreateResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn rejects_invalid_fingerprint() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerCreate],
            false,
        )
        .await;

        let result = handle_tracker_create(
            TrackerCreateRequest {
                fingerprint: Some("not-a-fingerprint".to_string()),
                ..valid_request()
            },
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerCreateResponse { success, .. } => assert!(!success),
            other => panic!("Expected TrackerCreateResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn rejects_empty_name() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerCreate],
            false,
        )
        .await;

        let result = handle_tracker_create(
            TrackerCreateRequest {
                name: "   ".to_string(),
                ..valid_request()
            },
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerCreateResponse { success, .. } => assert!(!success),
            other => panic!("Expected TrackerCreateResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn rejects_duplicate_endpoint() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerCreate],
            false,
        )
        .await;

        // First create.
        handle_tracker_create(
            valid_request(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();
        let _ = read_server_message(&mut test_ctx).await;

        // Second create with same (address, port) but different name.
        let result = handle_tracker_create(
            TrackerCreateRequest {
                name: "Different Name".to_string(),
                ..valid_request()
            },
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerCreateResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            other => panic!("Expected TrackerCreateResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn rejects_duplicate_name() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerCreate],
            false,
        )
        .await;

        handle_tracker_create(
            valid_request(),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await
        .unwrap();
        let _ = read_server_message(&mut test_ctx).await;

        // Second create with same name (case-insensitive) but different address.
        let result = handle_tracker_create(
            TrackerCreateRequest {
                address: "other.example.com".to_string(),
                name: "public".to_string(), // case-insensitive collision with "Public"
                ..valid_request()
            },
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerCreateResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            other => panic!("Expected TrackerCreateResponse, got {other:?}"),
        }
    }
}
