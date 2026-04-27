//! Server connection, handshake, and login

use tokio::io::BufReader;

use nexus_common::framing::{FrameReader, FrameWriter};
use nexus_common::io::{read_server_message, send_client_message};
use nexus_common::protocol::{ClientMessage, ServerMessage};
use nexus_common::{DEFAULT_TRANSFER_PORT, PROTOCOL_VERSION};

use crate::i18n::{DEFAULT_LOCALE, t, t_args};
use crate::types::{ConnectionInfo, NetworkConnection};

use super::constants::DEFAULT_FEATURES;
use super::stream::setup_communication_channels;
use super::tls::establish_connection;
use super::types::{
    ConnectError, ConnectionParams, FingerprintInterception, FingerprintMismatchDetails, LoginInfo,
    Reader, Writer,
};

/// Connect to server, perform staged fingerprint verification, handshake, and login.
///
/// Two-stage fingerprint verification protects credentials from active TLS
/// interception:
///
/// 1. **Stage 1 (post-TLS, pre-handshake):** if `params.expected_fingerprint`
///    is `Some`, the TLS-observed fingerprint must match. Catches cert
///    rotation / wrong-server scenarios entirely offline (no protocol bytes
///    exchanged). On mismatch, the user is shown an accept/reject dialog by
///    the caller.
/// 2. **Stage 2 (post-handshake, pre-login):** the server's self-reported
///    fingerprint (from `HandshakeResponse`) must match the TLS-observed
///    fingerprint. Catches naive MITMs that don't bother to rewrite the
///    protocol response. No accept path — a mismatch here means active
///    interception is in progress.
///
/// Credentials are sent only after both stages pass.
pub async fn connect_to_server(
    params: ConnectionParams,
) -> Result<NetworkConnection, ConnectError> {
    // Establish TCP+TLS connection and observe the server certificate fingerprint.
    let (tls_stream, tls_fingerprint) =
        establish_connection(&params.server_address, params.port, params.proxy.as_ref())
            .await
            .map_err(ConnectError::Other)?;

    // Stage 1: TOFU check against bookmark's stored fingerprint, if any.
    // Both sides come from `nexus_common::fingerprint::format_certificate_fingerprint`
    // (single canonical producer), so direct `!=` is correct — no normalization needed.
    if let Some(expected) = &params.expected_fingerprint
        && expected != &tls_fingerprint
    {
        return Err(ConnectError::FingerprintMismatch(Box::new(
            FingerprintMismatchDetails {
                expected: expected.clone(),
                received: tls_fingerprint.clone(),
                server_address: params.server_address.clone(),
                server_port: params.port.to_string(),
            },
        )));
    }

    let (reader, writer) = tokio::io::split(tls_stream);
    let buf_reader = BufReader::new(reader);
    let mut frame_reader = FrameReader::new(buf_reader);
    let mut frame_writer = FrameWriter::new(writer);

    // Handshake — server self-reports its certificate fingerprint here.
    let server_fingerprint = perform_handshake(&mut frame_reader, &mut frame_writer)
        .await
        .map_err(ConnectError::Other)?;

    // Stage 2: server-reported fingerprint must match TLS-observed.
    // Both produced by the same canonical formatter — direct `!=` is correct.
    // Mismatch here = active TLS interception. No accept path; bail before
    // sending credentials.
    if server_fingerprint != tls_fingerprint {
        return Err(ConnectError::FingerprintInterception(Box::new(
            FingerprintInterception {
                // Left empty — connect.rs has no friendly name. Result handlers
                // populate this with the user-typed form name, bookmark name,
                // or matched-URI bookmark name as appropriate.
                server_name: String::new(),
                server_address: params.server_address.clone(),
                server_port: params.port.to_string(),
                tls_fingerprint: tls_fingerprint.clone(),
                server_fingerprint,
            },
        )));
    }

    // Both stages passed — safe to send credentials.
    let login_info = perform_login(
        &mut frame_reader,
        &mut frame_writer,
        params.username.clone(),
        params.password.clone(),
        params.nickname.clone(),
        params.locale,
        params.avatar,
    )
    .await
    .map_err(ConnectError::Other)?;

    // Build connection info from connection params and login response.
    // Resolve server_name: prefer server-provided name, fall back to address.
    let server_name = login_info
        .server_name
        .clone()
        .unwrap_or_else(|| params.server_address.clone());

    let connection_info = ConnectionInfo {
        server_name,
        address: params.server_address,
        port: params.port,
        transfer_port: login_info.transfer_port,
        certificate_fingerprint: tls_fingerprint,
        username: params.username,
        password: params.password,
        nickname: params.nickname.unwrap_or_default(),
    };

    // Set up bidirectional communication
    setup_communication_channels(
        frame_reader,
        frame_writer,
        login_info,
        connection_info,
        params.connection_id,
    )
    .await
    .map_err(ConnectError::Other)
}

/// Perform protocol handshake with the server.
///
/// Returns the server's self-reported certificate fingerprint, which the
/// caller compares against the TLS-observed fingerprint before login.
async fn perform_handshake(reader: &mut Reader, writer: &mut Writer) -> Result<String, String> {
    let handshake = ClientMessage::Handshake {
        version: PROTOCOL_VERSION.to_string(),
    };
    send_client_message(writer, &handshake)
        .await
        .map_err(|e| t_args("err-failed-send-handshake", &[("error", &e.to_string())]))?;

    let received = read_server_message(reader)
        .await
        .map_err(|e| t_args("err-failed-read-handshake", &[("error", &e.to_string())]))?
        .ok_or_else(|| t("err-connection-closed"))?;

    match received.message {
        ServerMessage::HandshakeResponse {
            success: true,
            fingerprint,
            ..
        } => Ok(fingerprint),
        ServerMessage::HandshakeResponse {
            success: false,
            error,
            ..
        } => Err(t_args(
            "err-handshake-failed",
            &[("error", &error.unwrap_or_default())],
        )),
        _ => Err(t("err-unexpected-handshake-response")),
    }
}

/// Perform login and return login info (session ID, admin status, permissions, locale)
async fn perform_login(
    reader: &mut Reader,
    writer: &mut Writer,
    username: String,
    password: String,
    nickname: Option<String>,
    locale: String,
    avatar: Option<String>,
) -> Result<LoginInfo, String> {
    let login = ClientMessage::Login {
        username,
        password,
        features: DEFAULT_FEATURES.iter().map(|s| s.to_string()).collect(),
        locale,
        avatar,
        nickname,
    };
    send_client_message(writer, &login)
        .await
        .map_err(|e| t_args("err-failed-send-login", &[("error", &e.to_string())]))?;

    let received = read_server_message(reader)
        .await
        .map_err(|e| t_args("err-failed-read-login", &[("error", &e.to_string())]))?
        .ok_or_else(|| t("err-connection-closed"))?;

    match received.message {
        ServerMessage::LoginResponse {
            success: true,
            session_id: Some(_),
            user_id,
            is_admin,
            permissions,
            server_info,
            channels,
            locale,
            nickname,
            ..
        } => Ok(LoginInfo {
            is_admin: is_admin.unwrap_or(false),
            user_id,
            // The protocol guarantees `nickname` is set on every successful
            // LoginResponse (since v0.5.2). The handshake compatibility check
            // already rejects pre-0.5 servers, so a missing nickname here
            // means the server is buggy or malicious — refuse to proceed.
            nickname: nickname.ok_or_else(|| t("err-server-omitted-nickname"))?,
            permissions: permissions.unwrap_or_default(),
            server_name: server_info.as_ref().and_then(|info| info.name.clone()),
            server_description: server_info
                .as_ref()
                .and_then(|info| info.description.clone()),
            public_address: server_info
                .as_ref()
                .and_then(|info| info.public_address.clone())
                .filter(|s| !s.is_empty()),
            server_version: server_info.as_ref().and_then(|info| info.version.clone()),
            server_image: server_info
                .as_ref()
                .and_then(|info| info.image.clone())
                .unwrap_or_default(),
            channels: channels.unwrap_or_default(),
            chat_burst_limit: server_info.as_ref().and_then(|info| info.chat_burst_limit),
            chat_rate_limit: server_info.as_ref().and_then(|info| info.chat_rate_limit),
            max_connections_per_ip: server_info
                .as_ref()
                .and_then(|info| info.max_connections_per_ip),
            max_transfers_per_ip: server_info
                .as_ref()
                .and_then(|info| info.max_transfers_per_ip),
            file_reindex_interval: server_info
                .as_ref()
                .and_then(|info| info.file_reindex_interval),
            persistent_channels: server_info
                .as_ref()
                .and_then(|info| info.persistent_channels.clone()),
            auto_join_channels: server_info
                .as_ref()
                .and_then(|info| info.auto_join_channels.clone()),
            min_password_strength: server_info
                .as_ref()
                .and_then(|info| info.min_password_strength)
                .map(nexus_common::validators::PasswordStrength::from)
                .unwrap_or(nexus_common::validators::PasswordStrength::Good),
            log_level: server_info.as_ref().and_then(|info| info.log_level.clone()),
            transfer_port: server_info
                .map(|info| info.transfer_port)
                .unwrap_or(DEFAULT_TRANSFER_PORT),
            locale: locale.unwrap_or_else(|| DEFAULT_LOCALE.to_string()),
        }),
        ServerMessage::LoginResponse {
            success: true,
            session_id: None,
            ..
        } => Err(t("err-no-session-id")),
        ServerMessage::LoginResponse {
            success: false,
            error: Some(msg),
            ..
        } => Err(msg),
        ServerMessage::LoginResponse {
            success: false,
            error: None,
            ..
        } => Err(t("err-login-failed")),
        ServerMessage::Error { message, .. } => Err(message),
        _ => Err(t("err-unexpected-login-response")),
    }
}
