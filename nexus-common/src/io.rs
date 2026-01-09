//! I/O utilities for sending and receiving protocol messages
//!
//! This module provides the interface between the protocol message types
//! (`ClientMessage`, `ServerMessage`) and the wire format (framing).

use std::io;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::framing::{
    DEFAULT_FRAME_TIMEOUT, DEFAULT_IDLE_TIMEOUT, FrameError, FrameReader, FrameWriter, MessageId,
    RawFrame,
};
use crate::protocol::{ClientMessage, ServerMessage};

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
            other => io::Error::other(other.to_string()),
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

    let frame = RawFrame::new(message_id, message_type.to_string(), payload);
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
    let message_type = server_message_type(message);
    let payload =
        serde_json::to_vec(message).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let frame = RawFrame::new(message_id, message_type.to_string(), payload);
    writer.write_frame(&frame).await.map_err(Into::into)
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
    let Some(frame) = reader.read_frame().await? else {
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
        .read_frame_with_timeout(DEFAULT_FRAME_TIMEOUT)
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
    let idle = idle_timeout.unwrap_or(DEFAULT_IDLE_TIMEOUT);
    let frame_time = frame_timeout.unwrap_or(DEFAULT_FRAME_TIMEOUT);

    let Some(frame) = reader
        .read_frame_with_full_timeout(idle, frame_time)
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
    let Some(frame) = reader.read_frame().await? else {
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
        ClientMessage::BanCreate { .. } => "BanCreate",
        ClientMessage::BanDelete { .. } => "BanDelete",
        ClientMessage::BanList => "BanList",
    }
}

/// Get the type name for a server message (matches enum variant name)
#[must_use]
pub fn server_message_type(message: &ServerMessage) -> &'static str {
    match message {
        ServerMessage::ChatMessage { .. } => "ChatMessage",
        ServerMessage::ChatTopicUpdated { .. } => "ChatTopicUpdated",
        ServerMessage::ChatTopicUpdateResponse { .. } => "ChatTopicUpdateResponse",
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
        ServerMessage::BanCreateResponse { .. } => "BanCreateResponse",
        ServerMessage::BanDeleteResponse { .. } => "BanDeleteResponse",
        ServerMessage::BanListResponse { .. } => "BanListResponse",
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::ChatAction;
    use std::io::Cursor;
    use tokio::io::BufReader;

    #[test]
    fn test_client_message_type() {
        assert_eq!(
            client_message_type(&ClientMessage::ChatSend {
                message: "hi".to_string(),
                action: ChatAction::Normal,
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
            }),
            "ChatMessage"
        );
        assert_eq!(
            server_message_type(&ServerMessage::Error {
                message: "error".to_string(),
                command: None,
            }),
            "Error"
        );
    }

    #[tokio::test]
    async fn test_send_and_receive_client_message() {
        let message = ClientMessage::ChatSend {
            message: "Hello, world!".to_string(),
            action: ChatAction::Normal,
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
        let message = ServerMessage::ChatMessage {
            session_id: 42,
            nickname: "alice".to_string(),
            is_admin: false,
            is_shared: false,
            message: "Hi there!".to_string(),
            action: ChatAction::Normal,
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

        let received = read_client_message(&mut reader).await.unwrap().unwrap();
        assert_eq!(received.message_id, sent_id);
    }

    #[tokio::test]
    async fn test_send_with_specific_id() {
        let message = ServerMessage::HandshakeResponse {
            success: true,
            version: Some("0.4.0".to_string()),
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

        let received = read_server_message(&mut reader).await.unwrap().unwrap();
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
}
