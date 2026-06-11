//! I/O utilities for sending and receiving protocol messages
//!
//! This module provides the interface between the protocol message types
//! (`ClientMessage`, `ServerMessage`) and the wire format (framing).

use std::io;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::framing::{
    DEFAULT_FRAME_TIMEOUT, DEFAULT_IDLE_TIMEOUT, FrameContexts, FrameError, FrameReader,
    FrameWriter, MessageId, RawFrame,
};
use crate::protocol::{ClientMessage, ServerMessage};
use crate::tracker_protocol::{TrackerClientMessage, TrackerServerMessage};

// =============================================================================
// Error Conversion
// =============================================================================

impl From<FrameError> for io::Error {
    fn from(err: FrameError) -> Self {
        match err {
            FrameError::Io(msg) => io::Error::other(msg),
            FrameError::ConnectionClosed => {
                io::Error::new(io::ErrorKind::ConnectionReset, "connection closed")
            }
            other => io::Error::other(other),
        }
    }
}

// =============================================================================
// Message Sending
// =============================================================================

/// Send a `ClientMessage` to the server
///
/// Generates a new message ID for request-response correlation.
/// Returns the message ID that was used.
pub async fn send_client_message<W>(
    writer: &mut FrameWriter<W>,
    message: &ClientMessage,
) -> io::Result<MessageId>
where
    W: AsyncWriteExt + Unpin,
{
    let message_id = MessageId::new();
    send_client_message_with_id(writer, message, message_id).await?;
    Ok(message_id)
}

/// Send a `ClientMessage` to the server with a specific message ID
///
/// This is useful when you need to correlate the response with the request.
pub async fn send_client_message_with_id<W>(
    writer: &mut FrameWriter<W>,
    message: &ClientMessage,
    message_id: MessageId,
) -> io::Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    let message_type = client_message_type(message);
    let payload =
        serde_json::to_vec(message).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let frame = RawFrame::new(message_id, message_type, payload);
    writer.write_frame(&frame).await.map_err(Into::into)
}

/// Send a `ServerMessage` to a client
///
/// Generates a new message ID. For responses, use `send_server_message_with_id`
/// to echo the request's message ID.
pub async fn send_server_message<W>(
    writer: &mut FrameWriter<W>,
    message: &ServerMessage,
) -> io::Result<MessageId>
where
    W: AsyncWriteExt + Unpin,
{
    let message_id = MessageId::new();
    send_server_message_with_id(writer, message, message_id).await?;
    Ok(message_id)
}

/// Send a `ServerMessage` to a client with a specific message ID
///
/// Use this to echo the request's message ID in responses.
pub async fn send_server_message_with_id<W>(
    writer: &mut FrameWriter<W>,
    message: &ServerMessage,
    message_id: MessageId,
) -> io::Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    let frame = server_message_to_raw_frame(message, message_id)?;
    writer.write_frame(&frame).await.map_err(Into::into)
}

/// Serialize a `ServerMessage` into complete BBS frame bytes.
///
/// This uses the same message-type and JSON payload encoding as
/// [`send_server_message_with_id`].
pub fn server_message_to_frame_bytes(
    message: &ServerMessage,
    message_id: MessageId,
) -> io::Result<Arc<[u8]>> {
    let frame = server_message_to_raw_frame(message, message_id)?;
    Ok(Arc::from(frame.to_bytes()))
}

fn server_message_to_raw_frame(
    message: &ServerMessage,
    message_id: MessageId,
) -> io::Result<RawFrame> {
    let message_type = server_message_type(message);
    let payload =
        serde_json::to_vec(message).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    Ok(RawFrame::new(message_id, message_type, payload))
}

// =============================================================================
// Message Receiving
// =============================================================================

/// Received client message with its message ID
#[derive(Debug)]
pub struct ReceivedClientMessage {
    /// The message ID from the frame (for response correlation)
    pub message_id: MessageId,
    /// The parsed client message
    pub message: ClientMessage,
}

/// Received server message with its message ID
#[derive(Debug)]
pub struct ReceivedServerMessage {
    /// The message ID from the frame (for request correlation)
    pub message_id: MessageId,
    /// The parsed server message
    pub message: ServerMessage,
}

/// Read a `ClientMessage` from the stream
///
/// Returns `Ok(None)` if the connection was cleanly closed.
///
/// # Note
///
/// This method has no timeout - it will wait indefinitely for data.
/// For production use, prefer [`read_client_message_with_timeout`].
pub async fn read_client_message<R>(
    reader: &mut FrameReader<R>,
) -> io::Result<Option<ReceivedClientMessage>>
where
    R: AsyncReadExt + Unpin,
{
    let Some(frame) = reader
        .read_frame_in_context(FrameContexts::BBS_CLIENT)
        .await?
    else {
        return Ok(None);
    };

    parse_client_frame(frame).map(Some)
}

/// Read a `ClientMessage` from the stream with a timeout
///
/// This method waits indefinitely for the first byte (allowing idle connections),
/// but once the first byte is received, the entire frame must complete within
/// 60 seconds. This protects against slowloris-style attacks while still
/// allowing users to idle.
///
/// Returns `Ok(None)` if the connection was cleanly closed.
pub async fn read_client_message_with_timeout<R>(
    reader: &mut FrameReader<R>,
) -> Result<Option<ReceivedClientMessage>, FrameError>
where
    R: AsyncReadExt + Unpin,
{
    let Some(frame) = reader
        .read_frame_in_context_with_timeout(DEFAULT_FRAME_TIMEOUT, FrameContexts::BBS_CLIENT)
        .await?
    else {
        return Ok(None);
    };

    parse_client_frame(frame)
        .map(Some)
        .map_err(|e| FrameError::InvalidJson(e.to_string()))
}

/// Read a `ClientMessage` from the stream with full timeout (no idle allowed)
///
/// Unlike [`read_client_message_with_timeout`], this method applies a timeout
/// to the entire read operation, including waiting for the first byte.
/// This is appropriate for protocols where idle connections should be disconnected,
/// such as the file transfer port.
///
/// Returns `Ok(None)` if the connection was cleanly closed.
///
/// # Arguments
///
/// * `reader` - The frame reader to read from
/// * `idle_timeout` - Maximum time to wait for the first byte (defaults to 30 seconds)
/// * `frame_timeout` - Maximum time to complete the frame after the first byte (defaults to 60 seconds)
pub async fn read_client_message_with_full_timeout<R>(
    reader: &mut FrameReader<R>,
    idle_timeout: Option<Duration>,
    frame_timeout: Option<Duration>,
) -> Result<Option<ReceivedClientMessage>, FrameError>
where
    R: AsyncReadExt + Unpin,
{
    read_client_message_in_context_with_full_timeout(
        reader,
        idle_timeout,
        frame_timeout,
        FrameContexts::BBS_CLIENT,
    )
    .await
}

/// Read the pre-handshake client frame (`Handshake` only) with full timeout.
pub async fn read_client_handshake_message_with_full_timeout<R>(
    reader: &mut FrameReader<R>,
    idle_timeout: Option<Duration>,
    frame_timeout: Option<Duration>,
) -> Result<Option<ReceivedClientMessage>, FrameError>
where
    R: AsyncReadExt + Unpin,
{
    read_client_message_in_context_with_full_timeout(
        reader,
        idle_timeout,
        frame_timeout,
        FrameContexts::CLIENT_HANDSHAKE,
    )
    .await
}

/// Read the post-handshake, pre-login client frame (`Login` only) with full timeout.
pub async fn read_client_login_message_with_full_timeout<R>(
    reader: &mut FrameReader<R>,
    idle_timeout: Option<Duration>,
    frame_timeout: Option<Duration>,
) -> Result<Option<ReceivedClientMessage>, FrameError>
where
    R: AsyncReadExt + Unpin,
{
    read_client_message_in_context_with_full_timeout(
        reader,
        idle_timeout,
        frame_timeout,
        FrameContexts::CLIENT_LOGIN,
    )
    .await
}

/// Read a logged-in transfer-port JSON control frame with full timeout.
pub async fn read_transfer_client_message_with_full_timeout<R>(
    reader: &mut FrameReader<R>,
    idle_timeout: Option<Duration>,
    frame_timeout: Option<Duration>,
) -> Result<Option<ReceivedClientMessage>, FrameError>
where
    R: AsyncReadExt + Unpin,
{
    read_client_message_in_context_with_full_timeout(
        reader,
        idle_timeout,
        frame_timeout,
        FrameContexts::TRANSFER_CLIENT,
    )
    .await
}

/// Shared body for the full-timeout client-message readers; the public
/// readers differ only by the context they enforce.
async fn read_client_message_in_context_with_full_timeout<R>(
    reader: &mut FrameReader<R>,
    idle_timeout: Option<Duration>,
    frame_timeout: Option<Duration>,
    context: FrameContexts,
) -> Result<Option<ReceivedClientMessage>, FrameError>
where
    R: AsyncReadExt + Unpin,
{
    let idle = idle_timeout.unwrap_or(DEFAULT_IDLE_TIMEOUT);
    let frame_time = frame_timeout.unwrap_or(DEFAULT_FRAME_TIMEOUT);

    let Some(frame) = reader
        .read_frame_in_context_with_full_timeout(idle, frame_time, context)
        .await?
    else {
        return Ok(None);
    };

    parse_client_frame(frame)
        .map(Some)
        .map_err(|e| FrameError::InvalidJson(e.to_string()))
}

/// Parse a raw frame into a `ReceivedClientMessage`
fn parse_client_frame(frame: RawFrame) -> io::Result<ReceivedClientMessage> {
    // Parse the JSON payload
    let message: ClientMessage = serde_json::from_slice(&frame.payload)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("invalid JSON: {e}")))?;

    // Validate that the frame type matches the message type
    let expected_type = client_message_type(&message);
    if frame.message_type != expected_type {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "frame type mismatch: frame says '{}' but JSON is '{}'",
                frame.message_type, expected_type
            ),
        ));
    }

    Ok(ReceivedClientMessage {
        message_id: frame.message_id,
        message,
    })
}

/// Parse a raw frame into a `ReceivedServerMessage`
fn parse_server_frame(frame: RawFrame) -> io::Result<ReceivedServerMessage> {
    // Parse the JSON payload
    let message: ServerMessage = serde_json::from_slice(&frame.payload)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("invalid JSON: {e}")))?;

    // Validate that the frame type matches the message type
    let expected_type = server_message_type(&message);
    if frame.message_type != expected_type {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "frame type mismatch: frame says '{}' but JSON is '{}'",
                frame.message_type, expected_type
            ),
        ));
    }

    Ok(ReceivedServerMessage {
        message_id: frame.message_id,
        message,
    })
}

/// Read a `ServerMessage` from the stream
///
/// Returns `Ok(None)` if the connection was cleanly closed.
pub async fn read_server_message<R>(
    reader: &mut FrameReader<R>,
) -> io::Result<Option<ReceivedServerMessage>>
where
    R: AsyncReadExt + Unpin,
{
    read_server_message_in_context(reader, FrameContexts::BBS_SERVER).await
}

/// Read the handshake response (`HandshakeResponse` or `Error` only).
pub async fn read_server_handshake_response<R>(
    reader: &mut FrameReader<R>,
) -> io::Result<Option<ReceivedServerMessage>>
where
    R: AsyncReadExt + Unpin,
{
    read_server_message_in_context(reader, FrameContexts::SERVER_HANDSHAKE_RESPONSE).await
}

/// Read the login response (`LoginResponse` or `Error` only).
pub async fn read_server_login_response<R>(
    reader: &mut FrameReader<R>,
) -> io::Result<Option<ReceivedServerMessage>>
where
    R: AsyncReadExt + Unpin,
{
    read_server_message_in_context(reader, FrameContexts::SERVER_LOGIN_RESPONSE).await
}

/// Read a logged-in transfer-port JSON control frame from the server.
pub async fn read_transfer_server_message<R>(
    reader: &mut FrameReader<R>,
) -> io::Result<Option<ReceivedServerMessage>>
where
    R: AsyncReadExt + Unpin,
{
    read_server_message_in_context(reader, FrameContexts::TRANSFER_SERVER).await
}

/// Read a `ServerMessage` in an explicit context.
///
/// Shared body for the phase/port server-message readers above, which are
/// what production code should use. Public for callers that legitimately
/// span phases with one reader — e.g. test harnesses that read handshake,
/// login, and stream responses through a single helper with a union context.
pub async fn read_server_message_in_context<R>(
    reader: &mut FrameReader<R>,
    context: FrameContexts,
) -> io::Result<Option<ReceivedServerMessage>>
where
    R: AsyncReadExt + Unpin,
{
    let Some(frame) = reader.read_frame_in_context(context).await? else {
        return Ok(None);
    };

    parse_server_frame(frame).map(Some)
}

// =============================================================================
// Message Type Helpers
// =============================================================================

/// Get the type name for a client message (matches enum variant name)
#[must_use]
pub fn client_message_type(message: &ClientMessage) -> &'static str {
    match message {
        ClientMessage::ChatSend { .. } => "ChatSend",
        ClientMessage::ChatTopicUpdate { .. } => "ChatTopicUpdate",
        ClientMessage::ChatJoin { .. } => "ChatJoin",
        ClientMessage::ChatLeave { .. } => "ChatLeave",
        ClientMessage::ChatList { .. } => "ChatList",
        ClientMessage::ChatSecret { .. } => "ChatSecret",
        ClientMessage::Handshake { .. } => "Handshake",
        ClientMessage::Login { .. } => "Login",
        ClientMessage::UserBroadcast { .. } => "UserBroadcast",
        ClientMessage::UserCreate { .. } => "UserCreate",
        ClientMessage::UserDelete { .. } => "UserDelete",
        ClientMessage::UserEdit { .. } => "UserEdit",
        ClientMessage::UserInfo { .. } => "UserInfo",
        ClientMessage::UserKick { .. } => "UserKick",
        ClientMessage::UserList { .. } => "UserList",
        ClientMessage::UserMessage { .. } => "UserMessage",
        ClientMessage::UserUpdate { .. } => "UserUpdate",
        ClientMessage::UserAway { .. } => "UserAway",
        ClientMessage::UserBack => "UserBack",
        ClientMessage::UserStatus { .. } => "UserStatus",
        ClientMessage::ServerInfoUpdate { .. } => "ServerInfoUpdate",
        ClientMessage::TrackerAcceptFingerprint { .. } => "TrackerAcceptFingerprint",
        ClientMessage::TrackerAdd { .. } => "TrackerAdd",
        ClientMessage::TrackerEdit { .. } => "TrackerEdit",
        ClientMessage::TrackerList => "TrackerList",
        ClientMessage::TrackerRemove { .. } => "TrackerRemove",
        ClientMessage::TrackerUpdate { .. } => "TrackerUpdate",
        ClientMessage::NewsList => "NewsList",
        ClientMessage::NewsShow { .. } => "NewsShow",
        ClientMessage::NewsCreate { .. } => "NewsCreate",
        ClientMessage::NewsEdit { .. } => "NewsEdit",
        ClientMessage::NewsUpdate { .. } => "NewsUpdate",
        ClientMessage::NewsDelete { .. } => "NewsDelete",
        ClientMessage::FileList { .. } => "FileList",
        ClientMessage::FileCreateDir { .. } => "FileCreateDir",
        ClientMessage::FileDelete { .. } => "FileDelete",
        ClientMessage::FileInfo { .. } => "FileInfo",
        ClientMessage::FileRename { .. } => "FileRename",
        ClientMessage::FileMove { .. } => "FileMove",
        ClientMessage::FileCopy { .. } => "FileCopy",
        ClientMessage::FileDownload { .. } => "FileDownload",
        ClientMessage::FileStartResponse { .. } => "FileStartResponse",
        ClientMessage::FileUpload { .. } => "FileUpload",
        ClientMessage::FileStart { .. } => "FileStart",
        ClientMessage::FileData => "FileData",
        ClientMessage::FileHashing { .. } => "FileHashing",
        ClientMessage::FileHash { .. } => "FileHash",
        ClientMessage::BanCreate { .. } => "BanCreate",
        ClientMessage::BanDelete { .. } => "BanDelete",
        ClientMessage::BanList => "BanList",
        ClientMessage::TrustCreate { .. } => "TrustCreate",
        ClientMessage::TrustDelete { .. } => "TrustDelete",
        ClientMessage::TrustList => "TrustList",
        ClientMessage::ConnectionMonitor => "ConnectionMonitor",
        ClientMessage::GroupList => "GroupList",
        ClientMessage::GroupCreate { .. } => "GroupCreate",
        ClientMessage::GroupEdit { .. } => "GroupEdit",
        ClientMessage::GroupUpdate { .. } => "GroupUpdate",
        ClientMessage::GroupDelete { .. } => "GroupDelete",
        ClientMessage::FileSearch { .. } => "FileSearch",
        ClientMessage::FileReindex => "FileReindex",
        ClientMessage::VoiceJoin { .. } => "VoiceJoin",
        ClientMessage::VoiceLeave => "VoiceLeave",
        ClientMessage::Ping => "Ping",
    }
}

/// Get the type name for a server message (matches enum variant name)
#[must_use]
pub fn server_message_type(message: &ServerMessage) -> &'static str {
    match message {
        ServerMessage::ChatMessage { .. } => "ChatMessage",
        ServerMessage::ChatUpdated { .. } => "ChatUpdated",
        ServerMessage::ChatTopicUpdateResponse { .. } => "ChatTopicUpdateResponse",
        ServerMessage::ChatJoinResponse { .. } => "ChatJoinResponse",
        ServerMessage::ChatLeaveResponse { .. } => "ChatLeaveResponse",
        ServerMessage::ChatListResponse { .. } => "ChatListResponse",
        ServerMessage::ChatSecretResponse { .. } => "ChatSecretResponse",
        ServerMessage::ChatUserJoined { .. } => "ChatUserJoined",
        ServerMessage::ChatUserLeft { .. } => "ChatUserLeft",
        ServerMessage::ChatUserRenamed { .. } => "ChatUserRenamed",
        ServerMessage::Error { .. } => "Error",
        ServerMessage::HandshakeResponse { .. } => "HandshakeResponse",
        ServerMessage::LoginResponse { .. } => "LoginResponse",
        ServerMessage::PermissionsUpdated { .. } => "PermissionsUpdated",
        ServerMessage::ServerBroadcast { .. } => "ServerBroadcast",
        ServerMessage::UserBroadcastResponse { .. } => "UserBroadcastResponse",
        ServerMessage::UserConnected { .. } => "UserConnected",
        ServerMessage::UserCreateResponse { .. } => "UserCreateResponse",
        ServerMessage::UserDeleteResponse { .. } => "UserDeleteResponse",
        ServerMessage::UserDisconnected { .. } => "UserDisconnected",
        ServerMessage::UserEditResponse { .. } => "UserEditResponse",
        ServerMessage::UserInfoResponse { .. } => "UserInfoResponse",
        ServerMessage::UserKickResponse { .. } => "UserKickResponse",
        ServerMessage::UserListResponse { .. } => "UserListResponse",
        ServerMessage::UserMessage { .. } => "UserMessage",
        ServerMessage::UserMessageResponse { .. } => "UserMessageResponse",
        ServerMessage::UserUpdated { .. } => "UserUpdated",
        ServerMessage::UserAwayResponse { .. } => "UserAwayResponse",
        ServerMessage::UserBackResponse { .. } => "UserBackResponse",
        ServerMessage::UserStatusResponse { .. } => "UserStatusResponse",
        ServerMessage::UserUpdateResponse { .. } => "UserUpdateResponse",
        ServerMessage::ServerInfoUpdated { .. } => "ServerInfoUpdated",
        ServerMessage::ServerInfoUpdateResponse { .. } => "ServerInfoUpdateResponse",
        ServerMessage::TrackerAcceptFingerprintResponse { .. } => {
            "TrackerAcceptFingerprintResponse"
        }
        ServerMessage::TrackerAddResponse { .. } => "TrackerAddResponse",
        ServerMessage::TrackerEditResponse { .. } => "TrackerEditResponse",
        ServerMessage::TrackerListResponse { .. } => "TrackerListResponse",
        ServerMessage::TrackerRemoveResponse { .. } => "TrackerRemoveResponse",
        ServerMessage::TrackerUpdateResponse { .. } => "TrackerUpdateResponse",
        ServerMessage::NewsListResponse { .. } => "NewsListResponse",
        ServerMessage::NewsShowResponse { .. } => "NewsShowResponse",
        ServerMessage::NewsCreateResponse { .. } => "NewsCreateResponse",
        ServerMessage::NewsEditResponse { .. } => "NewsEditResponse",
        ServerMessage::NewsUpdateResponse { .. } => "NewsUpdateResponse",
        ServerMessage::NewsDeleteResponse { .. } => "NewsDeleteResponse",
        ServerMessage::NewsUpdated { .. } => "NewsUpdated",
        ServerMessage::FileListResponse { .. } => "FileListResponse",
        ServerMessage::FileCreateDirResponse { .. } => "FileCreateDirResponse",
        ServerMessage::FileDeleteResponse { .. } => "FileDeleteResponse",
        ServerMessage::FileInfoResponse { .. } => "FileInfoResponse",
        ServerMessage::FileRenameResponse { .. } => "FileRenameResponse",
        ServerMessage::FileMoveResponse { .. } => "FileMoveResponse",
        ServerMessage::FileCopyResponse { .. } => "FileCopyResponse",
        ServerMessage::FileDownloadResponse { .. } => "FileDownloadResponse",
        ServerMessage::FileStart { .. } => "FileStart",
        ServerMessage::FileData => "FileData",
        ServerMessage::FileUploadResponse { .. } => "FileUploadResponse",
        ServerMessage::FileStartResponse { .. } => "FileStartResponse",
        ServerMessage::TransferComplete { .. } => "TransferComplete",
        ServerMessage::FileHashing { .. } => "FileHashing",
        ServerMessage::FileHash { .. } => "FileHash",
        ServerMessage::BanCreateResponse { .. } => "BanCreateResponse",
        ServerMessage::BanDeleteResponse { .. } => "BanDeleteResponse",
        ServerMessage::BanListResponse { .. } => "BanListResponse",
        ServerMessage::TrustCreateResponse { .. } => "TrustCreateResponse",
        ServerMessage::TrustDeleteResponse { .. } => "TrustDeleteResponse",
        ServerMessage::TrustListResponse { .. } => "TrustListResponse",
        ServerMessage::ConnectionMonitorResponse { .. } => "ConnectionMonitorResponse",
        ServerMessage::GroupListResponse { .. } => "GroupListResponse",
        ServerMessage::GroupCreateResponse { .. } => "GroupCreateResponse",
        ServerMessage::GroupEditResponse { .. } => "GroupEditResponse",
        ServerMessage::GroupUpdateResponse { .. } => "GroupUpdateResponse",
        ServerMessage::GroupDeleteResponse { .. } => "GroupDeleteResponse",
        ServerMessage::FileSearchResponse { .. } => "FileSearchResponse",
        ServerMessage::FileReindexResponse { .. } => "FileReindexResponse",
        ServerMessage::VoiceJoinResponse { .. } => "VoiceJoinResponse",
        ServerMessage::VoiceLeaveResponse { .. } => "VoiceLeaveResponse",
        ServerMessage::VoiceUserJoined { .. } => "VoiceUserJoined",
        ServerMessage::VoiceUserLeft { .. } => "VoiceUserLeft",
        ServerMessage::Pong => "Pong",
    }
}

// =============================================================================
// Tracker protocol I/O
//
// Tracker connections share the BBS frame format and use BBS-protocol
// `Handshake` / `HandshakeResponse` for the initial handshake, but every
// post-handshake message is a `TrackerClientMessage` / `TrackerServerMessage`.
// These helpers mirror the BBS-protocol read/send/type helpers above.
// =============================================================================

/// Received tracker client message with its message ID.
#[derive(Debug)]
pub struct ReceivedTrackerClientMessage {
    pub message_id: MessageId,
    pub message: TrackerClientMessage,
}

/// Received tracker server message with its message ID.
#[derive(Debug)]
pub struct ReceivedTrackerServerMessage {
    pub message_id: MessageId,
    pub message: TrackerServerMessage,
}

/// Get the type name for a tracker client message (matches enum variant).
#[must_use]
pub fn tracker_client_message_type(message: &TrackerClientMessage) -> &'static str {
    match message {
        TrackerClientMessage::TrackerServerList { .. } => "TrackerServerList",
        TrackerClientMessage::TrackerServerRegister { .. } => "TrackerServerRegister",
    }
}

/// Get the type name for a tracker server message (matches enum variant).
#[must_use]
pub fn tracker_server_message_type(message: &TrackerServerMessage) -> &'static str {
    match message {
        TrackerServerMessage::Error { .. } => "Error",
        TrackerServerMessage::TrackerServerListResponse { .. } => "TrackerServerListResponse",
        TrackerServerMessage::TrackerServerRegisterResponse { .. } => {
            "TrackerServerRegisterResponse"
        }
    }
}

/// Send a `TrackerServerMessage` to the peer with a fresh message ID.
pub async fn send_tracker_server_message<W>(
    writer: &mut FrameWriter<W>,
    message: &TrackerServerMessage,
) -> io::Result<MessageId>
where
    W: AsyncWriteExt + Unpin,
{
    let message_id = MessageId::new();
    let message_type = tracker_server_message_type(message);
    let payload =
        serde_json::to_vec(message).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let frame = RawFrame::new(message_id, message_type, payload);
    writer.write_frame(&frame).await.map_err(io::Error::from)?;
    Ok(message_id)
}

/// Send a `TrackerClientMessage` to the tracker with a fresh message ID.
///
/// Used by the tracker task in `nexus-server` (the BBS server acting
/// as a tracker client) and by client-side code that queries trackers
/// for server listings.
pub async fn send_tracker_client_message<W>(
    writer: &mut FrameWriter<W>,
    message: &TrackerClientMessage,
) -> io::Result<MessageId>
where
    W: AsyncWriteExt + Unpin,
{
    let message_id = MessageId::new();
    let message_type = tracker_client_message_type(message);
    let payload =
        serde_json::to_vec(message).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let frame = RawFrame::new(message_id, message_type, payload);
    writer.write_frame(&frame).await.map_err(io::Error::from)?;
    Ok(message_id)
}

/// Read a `TrackerClientMessage` with idle + frame timeouts.
///
/// `idle_timeout` overrides the default wait-for-first-byte (30s);
/// `frame_timeout` overrides the default frame-completion timeout (60s).
/// Returns `Ok(None)` if the connection was cleanly closed.
pub async fn read_tracker_client_message_with_full_timeout<R>(
    reader: &mut FrameReader<R>,
    idle_timeout: Option<Duration>,
    frame_timeout: Option<Duration>,
) -> Result<Option<ReceivedTrackerClientMessage>, FrameError>
where
    R: AsyncReadExt + Unpin,
{
    let idle = idle_timeout.unwrap_or(DEFAULT_IDLE_TIMEOUT);
    let frame_time = frame_timeout.unwrap_or(DEFAULT_FRAME_TIMEOUT);

    let Some(frame) = reader
        .read_frame_in_context_with_full_timeout(idle, frame_time, FrameContexts::TRACKER_CLIENT)
        .await?
    else {
        return Ok(None);
    };

    parse_tracker_client_frame(frame)
        .map(Some)
        .map_err(|e| FrameError::InvalidJson(e.to_string()))
}

/// Parse a raw frame into a `ReceivedTrackerClientMessage`.
fn parse_tracker_client_frame(frame: RawFrame) -> io::Result<ReceivedTrackerClientMessage> {
    let message: TrackerClientMessage = serde_json::from_slice(&frame.payload)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("invalid JSON: {e}")))?;

    let expected_type = tracker_client_message_type(&message);
    if frame.message_type != expected_type {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "frame type mismatch: frame says '{}' but JSON is '{}'",
                frame.message_type, expected_type
            ),
        ));
    }

    Ok(ReceivedTrackerClientMessage {
        message_id: frame.message_id,
        message,
    })
}

/// Read a `TrackerServerMessage` with idle + frame timeouts.
///
/// `idle_timeout` overrides the default wait-for-first-byte (30s);
/// `frame_timeout` overrides the default frame-completion timeout (60s).
/// Returns `Ok(None)` if the connection was cleanly closed.
///
/// Used by the tracker task in `nexus-server` to read tracker
/// responses, and by client-side code that queries trackers.
pub async fn read_tracker_server_message_with_full_timeout<R>(
    reader: &mut FrameReader<R>,
    idle_timeout: Option<Duration>,
    frame_timeout: Option<Duration>,
) -> Result<Option<ReceivedTrackerServerMessage>, FrameError>
where
    R: AsyncReadExt + Unpin,
{
    let idle = idle_timeout.unwrap_or(DEFAULT_IDLE_TIMEOUT);
    let frame_time = frame_timeout.unwrap_or(DEFAULT_FRAME_TIMEOUT);

    let Some(frame) = reader
        .read_frame_in_context_with_full_timeout(idle, frame_time, FrameContexts::TRACKER_SERVER)
        .await?
    else {
        return Ok(None);
    };

    parse_tracker_server_frame(frame)
        .map(Some)
        .map_err(|e| FrameError::InvalidJson(e.to_string()))
}

/// Parse a raw frame into a `ReceivedTrackerServerMessage`.
fn parse_tracker_server_frame(frame: RawFrame) -> io::Result<ReceivedTrackerServerMessage> {
    let message: TrackerServerMessage = serde_json::from_slice(&frame.payload)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("invalid JSON: {e}")))?;

    let expected_type = tracker_server_message_type(&message);
    if frame.message_type != expected_type {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "frame type mismatch: frame says '{}' but JSON is '{}'",
                frame.message_type, expected_type
            ),
        ));
    }

    Ok(ReceivedTrackerServerMessage {
        message_id: frame.message_id,
        message,
    })
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::ChatAction;
    use crate::validators::DEFAULT_CHANNEL;
    use std::io::Cursor;
    use tokio::io::BufReader;

    fn header_only_frame(message_type: &str, payload_length: u64) -> Vec<u8> {
        format!(
            "NX|{}|{}|a1b2c3d4e5f6|{}|",
            message_type.len(),
            message_type,
            payload_length
        )
        .into_bytes()
    }

    fn assert_unexpected_message_type(result: Result<Option<ReceivedClientMessage>, FrameError>) {
        assert!(matches!(
            result,
            Err(FrameError::UnexpectedMessageType(message_type))
                if message_type == "UserListResponse"
        ));
    }

    #[test]
    fn test_client_message_type() {
        assert_eq!(
            client_message_type(&ClientMessage::ChatSend {
                message: "hi".to_string(),
                action: ChatAction::Normal,
                channel: DEFAULT_CHANNEL.to_string(),
            }),
            "ChatSend"
        );
        assert_eq!(
            client_message_type(&ClientMessage::Handshake {
                version: "0.4.0".to_string()
            }),
            "Handshake"
        );
        assert_eq!(
            client_message_type(&ClientMessage::UserList { all: false }),
            "UserList"
        );
    }

    #[test]
    fn test_server_message_type() {
        assert_eq!(
            server_message_type(&ServerMessage::ChatMessage {
                session_id: 1,
                nickname: "test".to_string(),
                is_admin: false,
                is_shared: false,
                message: "hi".to_string(),
                action: ChatAction::Normal,
                channel: DEFAULT_CHANNEL.to_string(),
                timestamp: 0,
            }),
            "ChatMessage"
        );
        assert_eq!(
            server_message_type(&ServerMessage::Error {
                message: "error".to_string(),
                command: None,
                disconnect: false,
            }),
            "Error"
        );
    }

    async fn sent_server_frame_bytes(message: &ServerMessage, message_id: MessageId) -> Vec<u8> {
        let mut buffer = Vec::new();
        {
            let cursor = Cursor::new(&mut buffer);
            let mut writer = FrameWriter::new(cursor);
            send_server_message_with_id(&mut writer, message, message_id)
                .await
                .unwrap();
        }
        buffer
    }

    async fn assert_server_frame_bytes_match_writer(message: ServerMessage, message_id: MessageId) {
        let serialized = server_message_to_frame_bytes(&message, message_id).unwrap();
        let sent = sent_server_frame_bytes(&message, message_id).await;
        assert_eq!(serialized.as_ref(), sent.as_slice());
    }

    #[tokio::test]
    async fn server_message_to_frame_bytes_matches_writer_for_pong() {
        let message_id = MessageId::from_bytes(b"000000000001").unwrap();

        assert_server_frame_bytes_match_writer(ServerMessage::Pong, message_id).await;
    }

    #[tokio::test]
    async fn server_message_to_frame_bytes_matches_writer_for_disconnect_error() {
        let message_id = MessageId::from_bytes(b"000000000002").unwrap();

        assert_server_frame_bytes_match_writer(
            ServerMessage::Error {
                message: "Disconnected".to_string(),
                command: Some("BanCreate".to_string()),
                disconnect: true,
            },
            message_id,
        )
        .await;
    }

    #[tokio::test]
    async fn server_message_to_frame_bytes_matches_writer_for_large_payload() {
        let message_id = MessageId::from_bytes(b"000000000003").unwrap();

        assert_server_frame_bytes_match_writer(
            ServerMessage::ChatMessage {
                session_id: 42,
                nickname: "alice".to_string(),
                is_admin: true,
                is_shared: false,
                message: "x".repeat(16 * 1024),
                action: ChatAction::Normal,
                channel: DEFAULT_CHANNEL.to_string(),
                timestamp: 1234,
            },
            message_id,
        )
        .await;
    }

    #[tokio::test]
    async fn test_send_and_receive_client_message() {
        let message = ClientMessage::ChatSend {
            message: "Hello, world!".to_string(),
            action: ChatAction::Normal,
            channel: DEFAULT_CHANNEL.to_string(),
        };

        // Write the message
        let mut buffer = Vec::new();
        {
            let cursor = Cursor::new(&mut buffer);
            let mut writer = FrameWriter::new(cursor);
            send_client_message(&mut writer, &message).await.unwrap();
        }

        // Read it back
        let cursor = Cursor::new(buffer);
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let received = read_client_message(&mut reader).await.unwrap().unwrap();
        match received.message {
            ClientMessage::ChatSend { message, .. } => {
                assert_eq!(message, "Hello, world!");
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[tokio::test]
    async fn test_send_and_receive_server_message() {
        use crate::validators::DEFAULT_CHANNEL;

        let message = ServerMessage::ChatMessage {
            session_id: 42,
            nickname: "alice".to_string(),
            is_admin: false,
            is_shared: false,
            message: "Hi there!".to_string(),
            action: ChatAction::Normal,
            channel: DEFAULT_CHANNEL.to_string(),
            timestamp: 0,
        };

        // Write the message
        let mut buffer = Vec::new();
        {
            let cursor = Cursor::new(&mut buffer);
            let mut writer = FrameWriter::new(cursor);
            send_server_message(&mut writer, &message).await.unwrap();
        }

        // Read it back
        let cursor = Cursor::new(buffer);
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let received = read_server_message(&mut reader).await.unwrap().unwrap();
        match received.message {
            ServerMessage::ChatMessage {
                session_id,
                nickname,
                is_admin,
                is_shared,
                message,
                ..
            } => {
                assert_eq!(session_id, 42);
                assert_eq!(nickname, "alice");
                assert!(!is_admin);
                assert!(!is_shared);
                assert_eq!(message, "Hi there!");
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[tokio::test]
    async fn test_message_id_correlation() {
        let message = ClientMessage::Handshake {
            version: "0.4.0".to_string(),
        };

        // Write the message and capture the ID
        let mut buffer = Vec::new();
        let sent_id;
        {
            let cursor = Cursor::new(&mut buffer);
            let mut writer = FrameWriter::new(cursor);
            sent_id = send_client_message(&mut writer, &message).await.unwrap();
        }

        // Read it back and verify the ID matches
        let cursor = Cursor::new(buffer);
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let received = read_client_handshake_message_with_full_timeout(&mut reader, None, None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(received.message_id, sent_id);
    }

    #[tokio::test]
    async fn test_send_with_specific_id() {
        let message = ServerMessage::HandshakeResponse {
            success: true,
            version: Some(crate::PROTOCOL_VERSION.to_string()),
            fingerprint: "00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF".to_string(),
            error: None,
        };
        let specific_id = MessageId::new();

        // Write with specific ID
        let mut buffer = Vec::new();
        {
            let cursor = Cursor::new(&mut buffer);
            let mut writer = FrameWriter::new(cursor);
            send_server_message_with_id(&mut writer, &message, specific_id)
                .await
                .unwrap();
        }

        // Verify the ID was used
        let cursor = Cursor::new(buffer);
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let received = read_server_handshake_response(&mut reader)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(received.message_id, specific_id);
    }

    #[tokio::test]
    async fn test_frame_type_mismatch_client_message() {
        // Frame header says "ChatSend" but JSON payload has type "Handshake"
        // serde uses the "type" field inside JSON to determine the variant
        let id = MessageId::new();
        let payload = r#"{"type":"Handshake","version":"0.4.0"}"#;
        let frame_data = format!("NX|8|ChatSend|{}|{}|{}\n", id, payload.len(), payload);
        let cursor = Cursor::new(frame_data.into_bytes());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = read_client_message(&mut reader).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("frame type mismatch"));
    }

    #[tokio::test]
    async fn test_frame_type_mismatch_server_message() {
        // Frame header says "ChatMessage" but JSON payload has type "Error"
        let id = MessageId::new();
        let payload = r#"{"type":"Error","message":"oops","command":null}"#;
        let frame_data = format!("NX|11|ChatMessage|{}|{}|{}\n", id, payload.len(), payload);
        let cursor = Cursor::new(frame_data.into_bytes());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = read_server_message(&mut reader).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("frame type mismatch"));
    }

    #[tokio::test]
    async fn test_invalid_json_payload() {
        // Valid frame structure but invalid JSON payload
        let id = MessageId::new();
        let payload = "{not valid}";
        let frame_data = format!("NX|8|ChatSend|{}|{}|{}\n", id, payload.len(), payload);
        let cursor = Cursor::new(frame_data.into_bytes());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = read_client_message(&mut reader).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("invalid JSON"));
    }

    #[tokio::test]
    async fn test_json_missing_required_field() {
        // Valid JSON but missing required field for ChatSend
        let id = MessageId::new();
        let payload = "{}";
        let frame_data = format!("NX|8|ChatSend|{}|{}|{}\n", id, payload.len(), payload);
        let cursor = Cursor::new(frame_data.into_bytes());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = read_client_message(&mut reader).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[tokio::test]
    async fn bbs_client_reader_rejects_server_response_before_payload() {
        let cursor = Cursor::new(header_only_frame("UserListResponse", u64::MAX));
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = read_client_message_with_full_timeout(&mut reader, None, None).await;
        assert_unexpected_message_type(result);
    }

    #[tokio::test]
    async fn bbs_client_reader_rejects_file_data() {
        let cursor = Cursor::new(header_only_frame("FileData", u64::MAX));
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = read_client_message_with_full_timeout(&mut reader, None, None).await;
        assert!(matches!(
            result,
            Err(FrameError::UnexpectedMessageType(message_type)) if message_type == "FileData"
        ));
    }

    #[tokio::test]
    async fn bbs_client_reader_rejects_transfer_control() {
        let cursor = Cursor::new(header_only_frame("FileUpload", 128));
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = read_client_message_with_full_timeout(&mut reader, None, None).await;
        assert!(matches!(
            result,
            Err(FrameError::UnexpectedMessageType(message_type)) if message_type == "FileUpload"
        ));
    }

    #[tokio::test]
    async fn bbs_client_reader_rejects_handshake_and_login_after_login() {
        for message_type in ["Handshake", "Login"] {
            let cursor = Cursor::new(header_only_frame(message_type, 64));
            let buf_reader = BufReader::new(cursor);
            let mut reader = FrameReader::new(buf_reader);

            let result = read_client_message_with_full_timeout(&mut reader, None, None).await;
            assert!(
                matches!(
                    result,
                    Err(FrameError::UnexpectedMessageType(ref unexpected))
                        if unexpected == message_type
                ),
                "{message_type} must not be accepted by the logged-in BBS reader"
            );
        }
    }

    #[tokio::test]
    async fn bbs_server_reader_rejects_handshake_and_login_responses() {
        for message_type in ["HandshakeResponse", "LoginResponse"] {
            let cursor = Cursor::new(header_only_frame(message_type, 64));
            let buf_reader = BufReader::new(cursor);
            let mut reader = FrameReader::new(buf_reader);

            let err = read_server_message(&mut reader).await.unwrap_err();
            assert!(
                err.to_string()
                    .contains(&format!("unexpected message type: '{message_type}'")),
                "{message_type} must not be accepted by the logged-in BBS stream reader"
            );
        }
    }

    #[tokio::test]
    async fn client_handshake_reader_rejects_login_before_handshake() {
        let payload =
            r#"{"type":"Login","username":"u","password":"p","features":[],"locale":"en"}"#;
        let frame_data = format!("NX|5|Login|a1b2c3d4e5f6|{}|{}\n", payload.len(), payload);
        let cursor = Cursor::new(frame_data.into_bytes());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = read_client_handshake_message_with_full_timeout(&mut reader, None, None).await;
        assert!(matches!(
            result,
            Err(FrameError::UnexpectedMessageType(message_type)) if message_type == "Login"
        ));
    }

    #[tokio::test]
    async fn client_login_reader_rejects_command_before_login() {
        let payload = r#"{"type":"ChatSend","message":"hi","action":null,"channel":null}"#;
        let frame_data = format!("NX|8|ChatSend|a1b2c3d4e5f6|{}|{}\n", payload.len(), payload);
        let cursor = Cursor::new(frame_data.into_bytes());
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = read_client_login_message_with_full_timeout(&mut reader, None, None).await;
        assert!(matches!(
            result,
            Err(FrameError::UnexpectedMessageType(message_type)) if message_type == "ChatSend"
        ));
    }

    #[tokio::test]
    async fn server_login_response_reader_rejects_non_login_response() {
        let cursor = Cursor::new(header_only_frame("UserListResponse", u64::MAX));
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let err = read_server_login_response(&mut reader).await.unwrap_err();
        let frame_error = err
            .get_ref()
            .and_then(|source| source.downcast_ref::<FrameError>())
            .expect("unexpected message type should be preserved as FrameError source");
        assert!(matches!(
            frame_error,
            FrameError::UnexpectedMessageType(message_type) if message_type == "UserListResponse"
        ));
    }

    #[tokio::test]
    async fn transfer_client_reader_accepts_transfer_control() {
        let mut buffer = Vec::new();
        {
            let cursor = Cursor::new(&mut buffer);
            let mut writer = FrameWriter::new(cursor);
            send_client_message(
                &mut writer,
                &ClientMessage::FileUpload {
                    destination: "/incoming".to_string(),
                    file_count: 1,
                    total_size: 42,
                    root: false,
                },
            )
            .await
            .unwrap();
            send_client_message(
                &mut writer,
                &ClientMessage::FileHashing {
                    file: "demo.bin".to_string(),
                },
            )
            .await
            .unwrap();
        }

        let cursor = Cursor::new(buffer);
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let first = read_transfer_client_message_with_full_timeout(&mut reader, None, None)
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(first.message, ClientMessage::FileUpload { .. }));

        let second = read_transfer_client_message_with_full_timeout(&mut reader, None, None)
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(second.message, ClientMessage::FileHashing { .. }));
    }

    #[tokio::test]
    async fn transfer_client_reader_rejects_file_data() {
        let cursor = Cursor::new(header_only_frame("FileData", u64::MAX));
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = read_transfer_client_message_with_full_timeout(&mut reader, None, None).await;
        assert!(matches!(
            result,
            Err(FrameError::UnexpectedMessageType(message_type)) if message_type == "FileData"
        ));
    }

    #[tokio::test]
    async fn bbs_server_reader_rejects_transfer_control() {
        let mut buffer = Vec::new();
        {
            let cursor = Cursor::new(&mut buffer);
            let mut writer = FrameWriter::new(cursor);
            send_server_message(
                &mut writer,
                &ServerMessage::FileStart {
                    path: "demo.bin".to_string(),
                    size: 42,
                },
            )
            .await
            .unwrap();
        }

        let cursor = Cursor::new(buffer);
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let err = read_server_message(&mut reader).await.unwrap_err();
        assert!(
            err.to_string()
                .contains("unexpected message type: 'FileStart'")
        );
    }

    #[tokio::test]
    async fn transfer_server_reader_accepts_transfer_control() {
        let mut buffer = Vec::new();
        {
            let cursor = Cursor::new(&mut buffer);
            let mut writer = FrameWriter::new(cursor);
            send_server_message(
                &mut writer,
                &ServerMessage::FileStart {
                    path: "demo.bin".to_string(),
                    size: 42,
                },
            )
            .await
            .unwrap();
            send_server_message(
                &mut writer,
                &ServerMessage::FileHashing {
                    file: "demo.bin".to_string(),
                },
            )
            .await
            .unwrap();
        }

        let cursor = Cursor::new(buffer);
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let first = read_transfer_server_message(&mut reader)
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(first.message, ServerMessage::FileStart { .. }));

        let second = read_transfer_server_message(&mut reader)
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(second.message, ServerMessage::FileHashing { .. }));
    }

    #[tokio::test]
    async fn transfer_server_reader_rejects_file_data() {
        let cursor = Cursor::new(header_only_frame("FileData", u64::MAX));
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let err = read_transfer_server_message(&mut reader).await.unwrap_err();
        assert!(
            err.to_string()
                .contains("unexpected message type: 'FileData'")
        );
    }

    #[tokio::test]
    async fn server_handshake_response_reader_rejects_unexpected_list_response() {
        let cursor = Cursor::new(header_only_frame("UserListResponse", u64::MAX));
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let err = read_server_handshake_response(&mut reader)
            .await
            .unwrap_err();
        assert!(
            err.to_string()
                .contains("unexpected message type: 'UserListResponse'")
        );
    }

    #[tokio::test]
    async fn tracker_client_reader_accepts_tracker_requests() {
        let mut buffer = Vec::new();
        {
            let cursor = Cursor::new(&mut buffer);
            let mut writer = FrameWriter::new(cursor);
            send_tracker_client_message(
                &mut writer,
                &TrackerClientMessage::TrackerServerList {
                    password: None,
                    locale: "en".to_string(),
                    version: crate::PROTOCOL_VERSION.to_string(),
                },
            )
            .await
            .unwrap();
        }

        let cursor = Cursor::new(buffer);
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let received = read_tracker_client_message_with_full_timeout(&mut reader, None, None)
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(
            received.message,
            TrackerClientMessage::TrackerServerList { .. }
        ));
    }

    #[tokio::test]
    async fn tracker_server_reader_accepts_tracker_responses() {
        let mut buffer = Vec::new();
        {
            let cursor = Cursor::new(&mut buffer);
            let mut writer = FrameWriter::new(cursor);
            send_tracker_server_message(
                &mut writer,
                &TrackerServerMessage::TrackerServerRegisterResponse {
                    success: true,
                    refresh_interval: Some(300),
                    error: None,
                    error_kind: None,
                },
            )
            .await
            .unwrap();
        }

        let cursor = Cursor::new(buffer);
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let received = read_tracker_server_message_with_full_timeout(&mut reader, None, None)
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(
            received.message,
            TrackerServerMessage::TrackerServerRegisterResponse { .. }
        ));
    }

    #[tokio::test]
    async fn tracker_server_reader_rejects_bbs_response_before_payload() {
        let cursor = Cursor::new(header_only_frame("UserListResponse", u64::MAX));
        let buf_reader = BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = read_tracker_server_message_with_full_timeout(&mut reader, None, None).await;
        assert!(matches!(
            result,
            Err(FrameError::UnexpectedMessageType(message_type))
                if message_type == "UserListResponse"
        ));
    }
}
