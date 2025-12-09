//! Server connection, handshake, and login

use tokio::io::BufReader;

use nexus_common::PROTOCOL_VERSION;
use nexus_common::framing::{FrameReader, FrameWriter};
use nexus_common::io::{read_server_message, send_client_message};
use nexus_common::protocol::{ClientMessage, ServerMessage};

use crate::i18n::{DEFAULT_LOCALE, t, t_args};
use crate::types::NetworkConnection;

use super::constants::DEFAULT_FEATURES;
use super::stream::setup_communication_channels;
use super::tls::establish_connection;
use super::types::{LoginInfo, Reader, Writer};

/// Connect to server, perform handshake and login
///
/// Establishes a TCP connection, performs protocol handshake and authentication,
/// then sets up bidirectional communication channels. Returns a NetworkConnection
/// handle for sending messages to the server.
pub async fn connect_to_server(
    server_address: String,
    port: u16,
    username: String,
    password: String,
    locale: String,
    avatar: Option<String>,
    connection_id: usize,
) -> Result<NetworkConnection, String> {
    // Establish TCP connection and get certificate fingerprint
    let (tls_stream, fingerprint) = establish_connection(&server_address, port).await?;

    let (reader, writer) = tokio::io::split(tls_stream);
    let buf_reader = BufReader::new(reader);
    let mut frame_reader = FrameReader::new(buf_reader);
    let mut frame_writer = FrameWriter::new(writer);

    // Perform handshake and login
    perform_handshake(&mut frame_reader, &mut frame_writer).await?;
    let login_info = perform_login(
        &mut frame_reader,
        &mut frame_writer,
        username,
        password,
        locale,
        avatar,
    )
    .await?;

    // Set up bidirectional communication
    setup_communication_channels(
        frame_reader,
        frame_writer,
        login_info,
        connection_id,
        fingerprint,
    )
    .await
}

/// Perform protocol handshake with the server
async fn perform_handshake(reader: &mut Reader, writer: &mut Writer) -> Result<(), String> {
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
        ServerMessage::HandshakeResponse { success: true, .. } => Ok(()),
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
    locale: String,
    avatar: Option<String>,
) -> Result<LoginInfo, String> {
    let login = ClientMessage::Login {
        username,
        password,
        features: DEFAULT_FEATURES.iter().map(|s| s.to_string()).collect(),
        locale,
        avatar,
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
            session_id: Some(id),
            is_admin,
            permissions,
            server_info,
            chat_info,
            locale,
            ..
        } => Ok(LoginInfo {
            session_id: id,
            is_admin: is_admin.unwrap_or(false),
            permissions: permissions.unwrap_or_default(),
            server_name: server_info.as_ref().map(|info| info.name.clone()),
            server_description: server_info.as_ref().map(|info| info.description.clone()),
            server_version: server_info.as_ref().map(|info| info.version.clone()),
            chat_topic: chat_info.as_ref().map(|info| info.topic.clone()),
            chat_topic_set_by: chat_info.as_ref().map(|info| info.topic_set_by.clone()),
            max_connections_per_ip: server_info.and_then(|info| info.max_connections_per_ip),
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
