//! TrackerUpdate message handler — replaces a tracker's configuration
//! row and re-spawns its publisher task.

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
    err_tracker_name_too_long, err_tracker_not_found, err_tracker_password_too_long,
    err_tracker_port_invalid,
};
use crate::constants::{
    LOG_TRACKER_UPDATE_DB_ERROR, LOG_TRACKER_UPDATE_NOT_LOGGED_IN,
    LOG_TRACKER_UPDATE_PERMISSION_DENIED, LOG_TRACKER_UPDATE_SUCCESS,
};
use crate::db::{Permission, TrackerDbError, UpdateTrackerParams};

/// Fields for `handle_tracker_update`, mirroring the
/// `ClientMessage::TrackerUpdate` variant. Bundled into a struct so
/// the handler signature stays under the workspace-wide
/// too-many-arguments limit.
pub struct TrackerUpdateRequest {
    pub id: i64,
    pub address: String,
    pub port: u16,
    pub fingerprint: Option<String>,
    pub password: Option<String>,
    pub name: String,
    pub enabled: bool,
}

/// Handle a `TrackerUpdate { ... }` request. Requires the
/// `tracker_edit` permission. Validates inputs, replaces the row, and
/// asks the manager to abort the existing task and spawn a fresh one
/// (or just abort, if the row was disabled).
pub async fn handle_tracker_update<W>(
    request: TrackerUpdateRequest,
    session_id: Option<u32>,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let TrackerUpdateRequest {
        id,
        address,
        port,
        fingerprint,
        password,
        name,
        enabled,
    } = request;

    let Some(session_id) = session_id else {
        warn!(ip = %ctx.peer_addr, "{}", LOG_TRACKER_UPDATE_NOT_LOGGED_IN);
        return ctx
            .send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some("TrackerUpdate"))
            .await;
    };

    let requesting_user = match ctx.user_manager.get_user_by_session_id(session_id).await {
        Some(u) => u,
        None => {
            return ctx
                .send_error_and_disconnect(&err_authentication(ctx.locale), Some("TrackerUpdate"))
                .await;
        }
    };

    if !requesting_user.has_permission(Permission::TrackerEdit) {
        warn!(
            user = %requesting_user.username,
            ip = %ctx.peer_addr,
            "{}", LOG_TRACKER_UPDATE_PERMISSION_DENIED
        );
        let response = ServerMessage::TrackerUpdateResponse {
            success: false,
            error: Some(err_permission_denied(ctx.locale)),
            id: None,
            name: None,
        };
        return ctx.send_message(&response).await;
    }

    // Normalize empty-string password to None at the protocol boundary.
    let password = password.filter(|s| !s.is_empty());

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

    let result = ctx
        .db
        .trackers
        .update(
            id,
            UpdateTrackerParams {
                address: &address,
                port,
                fingerprint: fingerprint.as_deref(),
                password: password.as_deref(),
                name: &name,
                enabled,
            },
        )
        .await;

    let record = match result {
        Ok(Some(r)) => r,
        Ok(None) => {
            let response = ServerMessage::TrackerUpdateResponse {
                success: false,
                error: Some(err_tracker_not_found(ctx.locale)),
                id: None,
                name: None,
            };
            return ctx.send_message(&response).await;
        }
        Err(TrackerDbError::EndpointDuplicate) => {
            let response = ServerMessage::TrackerUpdateResponse {
                success: false,
                error: Some(err_tracker_endpoint_duplicate(ctx.locale)),
                id: None,
                name: None,
            };
            return ctx.send_message(&response).await;
        }
        Err(TrackerDbError::NameDuplicate) => {
            let response = ServerMessage::TrackerUpdateResponse {
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
                id = id,
                err = %e,
                "{}", LOG_TRACKER_UPDATE_DB_ERROR
            );
            return ctx
                .send_error_and_disconnect(&err_database(ctx.locale), Some("TrackerUpdate"))
                .await;
        }
    };

    info!(
        user = %requesting_user.username,
        ip = %ctx.peer_addr,
        id = record.id,
        name = %record.name,
        enabled = record.enabled,
        "{}", LOG_TRACKER_UPDATE_SUCCESS
    );

    // Abort the old task and spawn a fresh one (or just abort, if the
    // record is now disabled). The manager handles both cases.
    ctx.tracker_manager.replace(record.clone());

    let response = ServerMessage::TrackerUpdateResponse {
        success: true,
        error: None,
        id: Some(record.id),
        name: Some(record.name),
    };
    ctx.send_message(&response).await
}

/// Validate per-field inputs. Returns `Some(response)` with the first
/// failing message, or `None` if everything passes.
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
        return Some(reject_update(error));
    }

    if port == 0 {
        return Some(reject_update(err_tracker_port_invalid(ctx.locale)));
    }

    if let Some(fp) = fingerprint
        && !is_canonical_fingerprint(fp)
    {
        return Some(reject_update(err_tracker_fingerprint_invalid(ctx.locale)));
    }

    if let Some(pw) = password
        && pw.len() > MAX_PASSWORD_LENGTH
    {
        return Some(reject_update(err_tracker_password_too_long(
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
        return Some(reject_update(error));
    }

    None
}

fn reject_update(error: String) -> ServerMessage {
    ServerMessage::TrackerUpdateResponse {
        success: false,
        error: Some(error),
        id: None,
        name: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{self, CreateTrackerParams};
    use crate::handlers::testing::{create_test_context, login_user, read_server_message};

    async fn seed_tracker(test_ctx: &mut crate::handlers::testing::TestContext) -> i64 {
        test_ctx
            .db
            .trackers
            .create(CreateTrackerParams {
                address: "tracker.example.com",
                port: 7510,
                fingerprint: None,
                password: None,
                name: "Original",
                enabled: true,
            })
            .await
            .expect("create")
            .id
    }

    fn valid_request(id: i64) -> TrackerUpdateRequest {
        TrackerUpdateRequest {
            id,
            address: "tracker2.example.com".to_string(),
            port: 7520,
            fingerprint: None,
            password: None,
            name: "Renamed".to_string(),
            enabled: true,
        }
    }

    #[tokio::test]
    async fn requires_login() {
        let mut test_ctx = create_test_context().await;
        let result =
            handle_tracker_update(valid_request(1), None, &mut test_ctx.handler_context()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn requires_permission() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(&mut test_ctx, "alice", "password", &[], false).await;

        let result = handle_tracker_update(
            valid_request(1),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerUpdateResponse { success, .. } => assert!(!success),
            other => panic!("Expected TrackerUpdateResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn happy_path_replaces_row() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerEdit],
            false,
        )
        .await;
        let id = seed_tracker(&mut test_ctx).await;

        let request = valid_request(id);
        let expected_name = request.name.clone();
        let result =
            handle_tracker_update(request, Some(session_id), &mut test_ctx.handler_context()).await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerUpdateResponse {
                success,
                id: response_id,
                name,
                error,
            } => {
                assert!(success, "expected success, got error: {error:?}");
                assert_eq!(response_id, Some(id));
                assert_eq!(name.as_deref(), Some(expected_name.as_str()));
            }
            other => panic!("Expected TrackerUpdateResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn unknown_id_returns_not_found() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerEdit],
            false,
        )
        .await;

        let result = handle_tracker_update(
            valid_request(99_999),
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerUpdateResponse {
                success,
                id: response_id,
                ..
            } => {
                assert!(!success);
                assert!(response_id.is_none());
            }
            other => panic!("Expected TrackerUpdateResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn rejects_bad_address() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerEdit],
            false,
        )
        .await;
        let id = seed_tracker(&mut test_ctx).await;

        let result = handle_tracker_update(
            TrackerUpdateRequest {
                address: "tracker.example.com:7500".to_string(),
                ..valid_request(id)
            },
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerUpdateResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            other => panic!("Expected TrackerUpdateResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn rejects_duplicate_endpoint() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerEdit],
            false,
        )
        .await;

        // Seed two rows; try to update row #2 to take row #1's endpoint.
        let _first = test_ctx
            .db
            .trackers
            .create(CreateTrackerParams {
                address: "first.example.com",
                port: 7510,
                fingerprint: None,
                password: None,
                name: "First",
                enabled: true,
            })
            .await
            .expect("create first");
        let second = test_ctx
            .db
            .trackers
            .create(CreateTrackerParams {
                address: "second.example.com",
                port: 7510,
                fingerprint: None,
                password: None,
                name: "Second",
                enabled: true,
            })
            .await
            .expect("create second");

        let result = handle_tracker_update(
            TrackerUpdateRequest {
                id: second.id,
                address: "first.example.com".to_string(),
                port: 7510,
                fingerprint: None,
                password: None,
                name: "Second".to_string(),
                enabled: true,
            },
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerUpdateResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            other => panic!("Expected TrackerUpdateResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn rejects_duplicate_name() {
        let mut test_ctx = create_test_context().await;
        let session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[db::Permission::TrackerEdit],
            false,
        )
        .await;

        let _first = test_ctx
            .db
            .trackers
            .create(CreateTrackerParams {
                address: "first.example.com",
                port: 7510,
                fingerprint: None,
                password: None,
                name: "First",
                enabled: true,
            })
            .await
            .expect("create first");
        let second = test_ctx
            .db
            .trackers
            .create(CreateTrackerParams {
                address: "second.example.com",
                port: 7510,
                fingerprint: None,
                password: None,
                name: "Second",
                enabled: true,
            })
            .await
            .expect("create second");

        // Update second to take first's name (case-insensitive collision).
        let result = handle_tracker_update(
            TrackerUpdateRequest {
                id: second.id,
                address: "second.example.com".to_string(),
                port: 7510,
                fingerprint: None,
                password: None,
                name: "first".to_string(),
                enabled: true,
            },
            Some(session_id),
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result.is_ok());
        match read_server_message(&mut test_ctx).await {
            ServerMessage::TrackerUpdateResponse { success, error, .. } => {
                assert!(!success);
                assert!(error.is_some());
            }
            other => panic!("Expected TrackerUpdateResponse, got {other:?}"),
        }
    }
}
