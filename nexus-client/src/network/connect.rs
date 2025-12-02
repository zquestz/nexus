//! Server connection, handshake, and login

use tokio::io::{AsyncBufReadExt, BufReader};

use nexus_common::PROTOCOL_VERSION;
use nexus_common::io::send_client_message;
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
    connection_id: usize,
) -> Result<NetworkConnection, String> {
    // Establish TCP connection and get certificate fingerprint
    let (tls_stream, fingerprint) = establish_connection(&server_address, port).await?;

    let (reader, mut writer) = tokio::io::split(tls_stream);
    let mut reader = BufReader::new(reader);

    // Perform handshake and login
    perform_handshake(&mut reader, &mut writer).await?;
    let login_info = perform_login(&mut reader, &mut writer, username, password, locale).await?;

    // Set up bidirectional communication
    setup_communication_channels(reader, writer, login_info, connection_id, fingerprint).await
}

/// Perform protocol handshake with the server
async fn perform_handshake(reader: &mut Reader, writer: &mut Writer) -> Result<(), String> {
    let handshake = ClientMessage::Handshake {
        version: PROTOCOL_VERSION.to_string(),
    };
    send_client_message(writer, &handshake)
        .await
        .map_err(|e| t_args("err-failed-send-handshake", &[("error", &e.to_string())]))?;

    let mut line = String::new();
    reader
        .read_line(&mut line)
        .await
        .map_err(|e| t_args("err-failed-read-handshake", &[("error", &e.to_string())]))?;

    match serde_json::from_str::<ServerMessage>(line.trim()) {
        Ok(ServerMessage::HandshakeResponse { success: true, .. }) => Ok(()),
        Ok(ServerMessage::HandshakeResponse {
            success: false,
            error,
            ..
        }) => Err(t_args(
            "err-handshake-failed",
            &[("error", &error.unwrap_or_default())],
        )),
        Ok(_) => Err(t("err-unexpected-handshake-response")),
        Err(e) => Err(t_args(
            "err-failed-parse-handshake",
            &[("error", &e.to_string())],
        )),
    }
}

/// Perform login and return login info (session ID, admin status, permissions, locale)
async fn perform_login(
    reader: &mut Reader,
    writer: &mut Writer,
    username: String,
    password: String,
    locale: String,
) -> Result<LoginInfo, String> {
    let login = ClientMessage::Login {
        username,
        password,
        features: DEFAULT_FEATURES.iter().map(|s| s.to_string()).collect(),
        locale,
    };
    send_client_message(writer, &login)
        .await
        .map_err(|e| t_args("err-failed-send-login", &[("error", &e.to_string())]))?;

    let mut line = String::new();
    reader
        .read_line(&mut line)
        .await
        .map_err(|e| t_args("err-failed-read-login", &[("error", &e.to_string())]))?;

    match serde_json::from_str::<ServerMessage>(line.trim()) {
        Ok(ServerMessage::LoginResponse {
            success: true,
            session_id: Some(id),
            is_admin,
            permissions,
            server_info,
            locale,
            ..
        }) => Ok(LoginInfo {
            session_id: id,
            is_admin: is_admin.unwrap_or(false),
            permissions: permissions.unwrap_or_default(),
            chat_topic: server_info.as_ref().map(|info| info.chat_topic.clone()),
            chat_topic_set_by: server_info.map(|info| info.chat_topic_set_by),
            locale: locale.unwrap_or_else(|| DEFAULT_LOCALE.to_string()),
        }),
        Ok(ServerMessage::LoginResponse {
            success: true,
            session_id: None,
            ..
        }) => Err(t("err-no-session-id")),
        Ok(ServerMessage::LoginResponse {
            success: false,
            error: Some(msg),
            ..
        }) => Err(msg),
        Ok(ServerMessage::LoginResponse {
            success: false,
            error: None,
            ..
        }) => Err(t("err-login-failed")),
        Ok(ServerMessage::Error { message, .. }) => Err(message),
        Ok(_) => Err(t("err-unexpected-login-response")),
        Err(e) => Err(t_args(
            "err-failed-parse-login",
            &[("error", &e.to_string())],
        )),
    }
}
