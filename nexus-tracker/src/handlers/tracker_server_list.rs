//! `TrackerServerList` handler.
//!
//! A client connection sends one `TrackerServerList` request and the tracker
//! replies with `TrackerServerListResponse` (success or typed failure) and
//! closes the connection. There is no follow-up: client connections
//! are short-lived by design.
//!
//! Responsibilities of this handler:
//!
//! 1. Validate the `password` and `locale` field lengths.
//! 2. If listing is gated, verify the password.
//! 3. Take a snapshot from the registry and send it.
//!
//! Disconnect-on-failure is the connection task's responsibility — this
//! handler returns once the response (success or failure) has been
//! written to the wire.

use std::io;
use std::net::SocketAddr;

use tokio::io::AsyncWrite;
use tracing::{info, warn};

use nexus_common::framing::FrameWriter;
use nexus_common::io::send_tracker_server_message;
use nexus_common::tracker_protocol::TrackerServerMessage;
use nexus_common::validators::{MAX_LOCALE_LENGTH, MAX_PASSWORD_LENGTH};
use nexus_common::{ERROR_KIND_INVALID, ERROR_KIND_UNAUTHORIZED};

use crate::auth::check_password;
use crate::constants::{DEFAULT_LOCALE, LOG_LIST_REJECTED, LOG_LIST_RESPONSE};
use crate::errors::{
    err_tracker_locale_too_long, err_tracker_password_too_long, err_tracker_unauthorized,
};
use crate::state::TrackerState;

/// Decoded `TrackerServerList` request fields.
pub struct ListParams {
    pub password: Option<String>,
    pub locale: String,
}

/// Drive the `TrackerServerList` flow. Always sends exactly one
/// `TrackerServerListResponse` to the wire.
pub async fn handle_tracker_server_list<W>(
    params: ListParams,
    state: &TrackerState,
    writer: &mut FrameWriter<W>,
    peer_addr: SocketAddr,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let ListParams { password, locale } = params;

    // Length validation. Use the request's locale for translation when
    // it's within bounds; fall back to DEFAULT_LOCALE when the locale
    // field itself is what's too long (we can't trust it for translation).
    if locale.len() > MAX_LOCALE_LENGTH {
        warn!(ip = %peer_addr.ip(), reason = "locale_too_long", "{}", LOG_LIST_REJECTED);
        return send_failure(
            writer,
            ERROR_KIND_INVALID,
            err_tracker_locale_too_long(DEFAULT_LOCALE, MAX_LOCALE_LENGTH),
        )
        .await;
    }
    if let Some(p) = &password
        && p.len() > MAX_PASSWORD_LENGTH
    {
        warn!(ip = %peer_addr.ip(), reason = "password_too_long", "{}", LOG_LIST_REJECTED);
        return send_failure(
            writer,
            ERROR_KIND_INVALID,
            err_tracker_password_too_long(&locale, MAX_PASSWORD_LENGTH),
        )
        .await;
    }

    // Password verification (when listing is gated).
    if !check_password(password.as_deref(), state.listing_password_hash.as_deref()) {
        warn!(ip = %peer_addr.ip(), reason = "unauthorized", "{}", LOG_LIST_REJECTED);
        return send_failure(
            writer,
            ERROR_KIND_UNAUTHORIZED,
            err_tracker_unauthorized(&locale),
        )
        .await;
    }

    // Snapshot. The mutex is only held for the duration of the clone.
    let servers = state
        .registry
        .lock()
        .expect("registry mutex poisoned")
        .list();
    info!(ip = %peer_addr.ip(), count = servers.len(), "{}", LOG_LIST_RESPONSE);

    let response = TrackerServerMessage::TrackerServerListResponse {
        success: true,
        servers: Some(servers),
        error: None,
        error_kind: None,
    };
    send_tracker_server_message(writer, &response).await?;
    Ok(())
}

/// Build and send a failure-shaped `TrackerServerListResponse`.
async fn send_failure<W>(
    writer: &mut FrameWriter<W>,
    error_kind: &str,
    message: String,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let response = TrackerServerMessage::TrackerServerListResponse {
        success: false,
        servers: None,
        error: Some(message),
        error_kind: Some(error_kind.to_string()),
    };
    send_tracker_server_message(writer, &response).await?;
    Ok(())
}
