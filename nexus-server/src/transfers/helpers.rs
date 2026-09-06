//! Helper utilities for file transfer handling: error responses, transfer-ID
//! generation, shared validation, and path resolution.

use std::io;
use std::path::{Path, PathBuf};

use rand::RngExt;
use tokio::io::{AsyncRead, AsyncWriteExt};
use tokio::time::timeout;

use nexus_common::framing::{FrameError, FrameReader, FrameWriter, MessageId};
use nexus_common::io::{
    ReceivedClientMessage, read_transfer_client_message_with_full_timeout,
    send_server_message_with_id, server_message_to_frame_bytes,
};
use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{self, FilePathError};
use nexus_common::{
    ERROR_KIND_CAPACITY, ERROR_KIND_CONFLICT, ERROR_KIND_EXISTS, ERROR_KIND_HASH_MISMATCH,
    ERROR_KIND_INVALID, ERROR_KIND_IO_ERROR, ERROR_KIND_NOT_FOUND, ERROR_KIND_PERMISSION,
    ERROR_KIND_PROTOCOL_ERROR, TRANSFER_CONTROL_FRAME_TIMEOUT, TRANSFER_IO_PROGRESS_TIMEOUT,
};

use crate::constants::{
    ERR_TRANSFER_WRITE_PROGRESS_TIMEOUT, TRANSFER_SETUP_WRITE_TIMEOUT, TRANSFER_SHUTDOWN_TIMEOUT,
};
use crate::db::Permission;
use crate::files::path::{PathError, build_and_validate_candidate_path};
use crate::handlers::{
    err_file_area_not_accessible, err_permission_denied, err_transfer_path_invalid,
    err_transfer_path_not_found, err_transfer_path_too_long,
};

use super::types::AuthenticatedUser;

/// Structured transfer-response error: a translated message plus a
/// machine-readable kind the client branches on.
#[derive(Debug, Clone)]
pub struct TransferError {
    pub message: String,
    /// Machine-readable error kind (e.g. "exists", "permission").
    pub kind: &'static str,
}

impl TransferError {
    pub fn new(message: impl Into<String>, kind: &'static str) -> Self {
        Self {
            message: message.into(),
            kind,
        }
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(message, ERROR_KIND_INVALID)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(message, ERROR_KIND_NOT_FOUND)
    }

    pub fn permission(message: impl Into<String>) -> Self {
        Self::new(message, ERROR_KIND_PERMISSION)
    }

    pub fn io_error(message: impl Into<String>) -> Self {
        Self::new(message, ERROR_KIND_IO_ERROR)
    }

    pub fn protocol_error(message: impl Into<String>) -> Self {
        Self::new(message, ERROR_KIND_PROTOCOL_ERROR)
    }

    pub fn exists(message: impl Into<String>) -> Self {
        Self::new(message, ERROR_KIND_EXISTS)
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(message, ERROR_KIND_CONFLICT)
    }

    pub fn hash_mismatch(message: impl Into<String>) -> Self {
        Self::new(message, ERROR_KIND_HASH_MISMATCH)
    }

    pub fn capacity(message: impl Into<String>) -> Self {
        Self::new(message, ERROR_KIND_CAPACITY)
    }
}

impl std::fmt::Display for TransferError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.kind)
    }
}

impl std::error::Error for TransferError {}

/// Validate a transfer path, converting `FilePathError` to a translated error.
pub(crate) fn validate_transfer_path(path: &str, locale: &str) -> Result<(), TransferError> {
    if let Err(e) = validators::validate_file_path(path) {
        let error_msg = match e {
            FilePathError::TooLong => err_transfer_path_too_long(locale),
            FilePathError::ContainsNull
            | FilePathError::InvalidCharacters
            | FilePathError::ContainsWindowsDrive => err_transfer_path_invalid(locale),
        };
        return Err(TransferError::invalid(error_msg));
    }
    Ok(())
}

/// `Ok(())` if the user is admin or holds `permission`, else a permission error.
pub(crate) fn check_permission(
    user: &AuthenticatedUser,
    permission: Permission,
    locale: &str,
) -> Result<(), TransferError> {
    if !user.is_admin && !user.permissions.contains(&permission) {
        return Err(TransferError::permission(err_permission_denied(locale)));
    }
    Ok(())
}

/// `Ok(())` if the user is admin or holds at least one of `permissions`, else
/// a permission error.
pub(crate) fn check_any_permission(
    user: &AuthenticatedUser,
    permissions: &[Permission],
    locale: &str,
) -> Result<(), TransferError> {
    if user.is_admin || permissions.iter().any(|p| user.permissions.contains(p)) {
        return Ok(());
    }
    Err(TransferError::permission(err_permission_denied(locale)))
}

/// Require `file_root` only when root mode is requested.
pub(crate) fn check_root_permission(
    user: &AuthenticatedUser,
    use_root: bool,
    locale: &str,
) -> Result<(), TransferError> {
    if use_root {
        check_permission(user, Permission::FileRoot, locale)?;
    }
    Ok(())
}

/// Resolve and canonicalize the area root: the file root when `use_root`,
/// otherwise the user's personal (or shared) area.
pub(crate) async fn resolve_area_root(
    file_root: &Path,
    user_area_root: Option<&Path>,
    use_root: bool,
    locale: &str,
) -> Result<PathBuf, TransferError> {
    let area_root = if use_root {
        file_root.to_path_buf()
    } else {
        user_area_root
            .ok_or_else(|| TransferError::not_found(err_file_area_not_accessible(locale)))?
            .to_path_buf()
    };

    tokio::fs::canonicalize(&area_root)
        .await
        .map_err(|_| TransferError::not_found(err_file_area_not_accessible(locale)))
}

/// Build and validate a candidate path within an area root.
pub(crate) async fn build_validated_path(
    area_root: &Path,
    client_path: &str,
    locale: &str,
) -> Result<PathBuf, TransferError> {
    build_and_validate_candidate_path(area_root, client_path)
        .await
        .map_err(|_| TransferError::invalid(err_transfer_path_invalid(locale)))
}

pub(crate) fn path_error_to_transfer_error(e: PathError, locale: &str) -> TransferError {
    match e {
        PathError::NotFound => TransferError::not_found(err_transfer_path_not_found(locale)),
        PathError::AccessDenied => TransferError::permission(err_transfer_path_invalid(locale)),
        _ => TransferError::invalid(err_transfer_path_invalid(locale)),
    }
}

/// Build a failure `LoginResponse` (simplified for the transfer port).
pub(crate) fn login_error_response(error: String) -> ServerMessage {
    ServerMessage::LoginResponse {
        success: false,
        error: Some(error),
        session_id: None,
        user_id: None,
        is_admin: None,
        permissions: None,
        features: None,
        server_info: None,
        locale: None,
        channels: None,
        nickname: None,
        group_id: None,
        group_name: None,
    }
}

/// Send a download error, shut down the writer, and return `Ok(())` so the
/// handler can early-exit.
pub(crate) async fn send_download_error_and_close<W>(
    frame_writer: &mut FrameWriter<W>,
    error: &str,
    error_kind: Option<&str>,
) -> io::Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    let response = ServerMessage::FileDownloadResponse {
        success: false,
        error: Some(error.to_string()),
        error_kind: error_kind.map(String::from),
        size: None,
        file_count: None,
        transfer_id: None,
    };
    let _ = send_transfer_server_message_with_progress_timeout(
        frame_writer,
        &response,
        MessageId::new(),
    )
    .await;
    let _ = shutdown_transfer_writer(frame_writer).await;
    Ok(())
}

pub(crate) async fn send_download_transfer_error<W>(
    frame_writer: &mut FrameWriter<W>,
    error: &TransferError,
) -> io::Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    send_download_error_and_close(frame_writer, &error.message, Some(error.kind)).await
}

/// Send an upload error, shut down the writer, and return `Ok(())` so the
/// handler can early-exit.
pub(crate) async fn send_upload_error_and_close<W>(
    frame_writer: &mut FrameWriter<W>,
    error: &str,
    error_kind: Option<&str>,
) -> io::Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    let response = ServerMessage::FileUploadResponse {
        success: false,
        error: Some(error.to_string()),
        error_kind: error_kind.map(String::from),
        transfer_id: None,
    };
    let _ = send_transfer_server_message_with_progress_timeout(
        frame_writer,
        &response,
        MessageId::new(),
    )
    .await;
    let _ = shutdown_transfer_writer(frame_writer).await;
    Ok(())
}

pub(crate) async fn send_upload_transfer_error<W>(
    frame_writer: &mut FrameWriter<W>,
    error: &TransferError,
) -> io::Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    send_upload_error_and_close(frame_writer, &error.message, Some(error.kind)).await
}

/// Send a generic error and close. Used when the client's message type is
/// unexpected, so no specific `File*Response` fits the intent.
pub(crate) async fn send_error_and_close<W>(
    frame_writer: &mut FrameWriter<W>,
    error: &str,
) -> io::Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    let response = ServerMessage::Error {
        message: error.to_string(),
        command: None,
        disconnect: true,
    };
    let _ = send_transfer_server_message_with_progress_timeout(
        frame_writer,
        &response,
        MessageId::new(),
    )
    .await;
    let _ = shutdown_transfer_writer(frame_writer).await;
    Ok(())
}

/// Read a validated post-login control message within one total deadline.
pub(crate) async fn read_transfer_control_message<R>(
    frame_reader: &mut FrameReader<R>,
) -> Result<Option<ReceivedClientMessage>, FrameError>
where
    R: AsyncRead + Unpin,
{
    // Override the inner idle limit so it cannot expire before the total budget.
    // The outer timeout prevents idle time and frame completion from adding up.
    timeout(
        TRANSFER_CONTROL_FRAME_TIMEOUT,
        read_transfer_client_message_with_full_timeout(
            frame_reader,
            Some(TRANSFER_CONTROL_FRAME_TIMEOUT),
            Some(TRANSFER_CONTROL_FRAME_TIMEOUT),
        ),
    )
    .await
    .unwrap_or(Err(FrameError::FrameTimeout))
}

/// Send a server message with a 60-second deadline for the whole write and flush.
/// Individual writes do not extend the deadline.
pub(crate) async fn send_transfer_server_message<W>(
    frame_writer: &mut FrameWriter<W>,
    response: &ServerMessage,
    message_id: MessageId,
) -> io::Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    match timeout(
        TRANSFER_SETUP_WRITE_TIMEOUT,
        send_server_message_with_id(frame_writer, response, message_id),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => Err(io::Error::other(ERR_TRANSFER_WRITE_PROGRESS_TIMEOUT)),
    }
}

/// Send a server message with a fresh 60-second timeout for each write and flush.
pub(crate) async fn send_transfer_server_message_with_progress_timeout<W>(
    frame_writer: &mut FrameWriter<W>,
    response: &ServerMessage,
    message_id: MessageId,
) -> io::Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    let frame = server_message_to_frame_bytes(response, message_id)?;
    let writer = frame_writer.get_mut();
    let mut remaining = frame.as_ref();
    while !remaining.is_empty() {
        let written = timeout(TRANSFER_IO_PROGRESS_TIMEOUT, writer.write(remaining))
            .await
            .map_err(|_| io::Error::other(ERR_TRANSFER_WRITE_PROGRESS_TIMEOUT))??;
        if written == 0 {
            return Err(io::ErrorKind::WriteZero.into());
        }
        remaining = &remaining[written..];
    }
    timeout(TRANSFER_IO_PROGRESS_TIMEOUT, writer.flush())
        .await
        .map_err(|_| io::Error::other(ERR_TRANSFER_WRITE_PROGRESS_TIMEOUT))?
}

/// Attempt graceful transport shutdown within one 30-second total deadline.
/// The caller must drop the connection afterward, even if shutdown fails.
pub(crate) async fn shutdown_transfer_writer<W>(frame_writer: &mut FrameWriter<W>) -> io::Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    match timeout(TRANSFER_SHUTDOWN_TIMEOUT, frame_writer.get_mut().shutdown()).await {
        Ok(result) => result,
        Err(_) => Err(io::Error::other(ERR_TRANSFER_WRITE_PROGRESS_TIMEOUT)),
    }
}

/// Random 8-hex-char (32-bit) transfer ID for log correlation. NOT
/// cryptographically secure; never use for auth or anything security-sensitive.
pub(crate) fn generate_transfer_id() -> String {
    let bytes: [u8; 4] = rand::rng().random();
    hex::encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::super::test_helpers::make_authenticated_user;
    use super::*;
    use std::future::Future;
    use std::pin::Pin;
    use std::task::{Context, Poll, ready};
    use std::time::Duration;

    use crate::connection_io::test_helpers::{
        ShutdownBehavior, TestStream, TestTransport, WriteFailure, WriteState,
    };
    use nexus_common::framing::RawFrame;
    use nexus_common::io::client_message_to_frame_bytes;
    use nexus_common::protocol::ClientMessage;
    use tokio::io::{AsyncWrite, BufReader, duplex};
    use tokio::time::{Instant, Sleep, sleep};

    #[tokio::test(start_paused = true)]
    async fn encrypted_transfer_shutdown_has_one_total_deadline() {
        for transport in [TestTransport::Tls, TestTransport::WebSocket] {
            for failure in [None, Some(WriteFailure::Error), Some(WriteFailure::Stall)] {
                let pair = transport.pair(8192).await;
                *pair.state.lock().unwrap() = match failure {
                    Some(failure) => WriteState::failing(failure, 0),
                    None => WriteState::default(),
                };
                let mut writer = FrameWriter::new(pair.server);
                let started = Instant::now();
                let result = timeout(
                    Duration::from_secs(120),
                    shutdown_transfer_writer(&mut writer),
                )
                .await
                .expect("encrypted shutdown must observe its total deadline");
                assert_eq!(
                    started.elapsed(),
                    Duration::from_secs(if matches!(failure, Some(WriteFailure::Stall)) {
                        30
                    } else {
                        0
                    }),
                    "{transport:?} / {failure:?}",
                );
                assert_eq!(
                    result.is_err(),
                    failure.is_some(),
                    "{transport:?} / {failure:?}"
                );
                {
                    let state = pair.state.lock().unwrap();
                    if failure.is_some() {
                        assert!(state.failure_at.is_some(), "close must reach the transport");
                        assert!(state.bytes.is_empty());
                    } else {
                        assert!(
                            !state.bytes.is_empty(),
                            "successful close must send transport bytes"
                        );
                    }
                }
                drop(writer);
                assert!(pair.state.lock().unwrap().dropped);
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn test_transfer_shutdown_has_one_thirty_second_deadline() {
        for (behavior, elapsed) in [
            (ShutdownBehavior::Immediate, Duration::ZERO),
            (ShutdownBehavior::Error, Duration::ZERO),
            (
                ShutdownBehavior::Delayed(Duration::from_secs(10)),
                Duration::from_secs(10),
            ),
            (
                ShutdownBehavior::Delayed(Duration::from_secs(45)),
                Duration::from_secs(30),
            ),
            (ShutdownBehavior::Stall, Duration::from_secs(30)),
        ] {
            let (stream, state) = TestStream::new(Vec::new());
            state.lock().unwrap().shutdown_behavior = behavior;
            let mut writer = FrameWriter::new(stream);
            let started = Instant::now();
            let result = timeout(
                Duration::from_secs(120),
                shutdown_transfer_writer(&mut writer),
            )
            .await
            .expect("shutdown must finish within its own deadline");
            assert_eq!(started.elapsed(), elapsed, "{behavior:?}");
            match behavior {
                ShutdownBehavior::Immediate => result.unwrap(),
                ShutdownBehavior::Delayed(duration) if duration < Duration::from_secs(30) => {
                    result.unwrap()
                }
                ShutdownBehavior::Error => {
                    assert_eq!(result.unwrap_err().to_string(), "injected shutdown failure")
                }
                _ => assert_eq!(
                    result.unwrap_err().to_string(),
                    ERR_TRANSFER_WRITE_PROGRESS_TIMEOUT
                ),
            }
            let state = state.lock().unwrap();
            assert!(state.bytes.is_empty());
            assert_eq!(state.flushes, 0);
            assert!(state.shutdowns > 0);
        }
    }

    struct DelayedWriter {
        bytes: Vec<u8>,
        chunk_size: usize,
        write_delay: Duration,
        flush_delay: Duration,
        delay: Option<Pin<Box<Sleep>>>,
    }

    impl DelayedWriter {
        fn new(chunk_size: usize, write_delay: Duration, flush_delay: Duration) -> Self {
            Self {
                bytes: Vec::new(),
                chunk_size,
                write_delay,
                flush_delay,
                delay: None,
            }
        }

        fn poll_delay(&mut self, cx: &mut Context<'_>, duration: Duration) -> Poll<()> {
            let delay = self.delay.get_or_insert_with(|| Box::pin(sleep(duration)));
            ready!(delay.as_mut().poll(cx));
            self.delay = None;
            Poll::Ready(())
        }
    }

    impl AsyncWrite for DelayedWriter {
        fn poll_write(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            let this = self.get_mut();
            ready!(this.poll_delay(cx, this.write_delay));
            let len = buf.len().min(this.chunk_size);
            this.bytes.extend_from_slice(&buf[..len]);
            Poll::Ready(Ok(len))
        }

        fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            let this = self.get_mut();
            ready!(this.poll_delay(cx, this.flush_delay));
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    #[tokio::test(start_paused = true)]
    async fn test_control_read_stalls_time_out() {
        let messages = [
            ClientMessage::FileDownload {
                path: "test.txt".into(),
                root: false,
            },
            ClientMessage::FileUpload {
                destination: "uploads".into(),
                file_count: 1,
                total_size: 0,
                root: false,
            },
            ClientMessage::FileStart {
                path: "test.txt".into(),
                size: 0,
            },
            ClientMessage::FileStartResponse {
                size: 0,
                blake3: None,
            },
            ClientMessage::FileHashing {
                file: "test.txt".into(),
            },
        ];
        for message in messages {
            let frame = client_message_to_frame_bytes(&message, MessageId::new()).unwrap();
            let payload_len = serde_json::to_vec(&message).unwrap().len();
            let header_len = frame.len() - payload_len - 1;
            for sent_len in [
                0,
                1,
                header_len,
                header_len + payload_len / 2,
                frame.len() - 1,
            ] {
                let (mut sender, receiver) = duplex(8192);
                sender.write_all(&frame[..sent_len]).await.unwrap();
                let mut reader = FrameReader::new(BufReader::new(receiver));
                let started = Instant::now();
                let err = timeout(
                    TRANSFER_CONTROL_FRAME_TIMEOUT * 2,
                    read_transfer_control_message(&mut reader),
                )
                .await
                .expect("the open connection must not keep a partial control frame alive")
                .unwrap_err();
                assert!(matches!(
                    err,
                    FrameError::FrameTimeout | FrameError::IdleTimeout
                ));
                assert!(started.elapsed() >= TRANSFER_CONTROL_FRAME_TIMEOUT);
                assert!(
                    started.elapsed() < TRANSFER_CONTROL_FRAME_TIMEOUT + Duration::from_secs(1)
                );
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn test_control_read_idle_and_payload_share_one_deadline() {
        let message = ClientMessage::FileStart {
            path: "test.txt".into(),
            size: 0,
        };
        let frame = client_message_to_frame_bytes(&message, MessageId::new()).unwrap();
        let header_len = frame.len() - serde_json::to_vec(&message).unwrap().len() - 1;
        let (mut sender, receiver) = duplex(8192);
        let mut reader = FrameReader::new(BufReader::new(receiver));
        let read = async {
            let started = Instant::now();
            let result = timeout(
                TRANSFER_CONTROL_FRAME_TIMEOUT * 2,
                read_transfer_control_message(&mut reader),
            )
            .await
            .expect("partial progress must not extend the total deadline");
            (result, started.elapsed())
        };
        let send = async {
            sleep(TRANSFER_CONTROL_FRAME_TIMEOUT * 3 / 4).await;
            sender.write_all(&frame[..header_len]).await.unwrap();
            sleep(TRANSFER_CONTROL_FRAME_TIMEOUT / 8).await;
            sender
                .write_all(&frame[header_len..header_len + 1])
                .await
                .unwrap();
            sleep(TRANSFER_CONTROL_FRAME_TIMEOUT / 2).await;
            sender.write_all(&frame[header_len + 1..]).await.unwrap();
        };
        let ((result, elapsed), ()) = tokio::join!(read, send);
        assert!(matches!(result, Err(FrameError::FrameTimeout)));
        assert!(elapsed >= TRANSFER_CONTROL_FRAME_TIMEOUT);
        assert!(elapsed < TRANSFER_CONTROL_FRAME_TIMEOUT + Duration::from_secs(1));
    }

    #[tokio::test]
    async fn test_control_read_preserves_frame_validation_and_eof() {
        let handshake = ClientMessage::Handshake {
            version: nexus_common::PROTOCOL_VERSION.into(),
        };
        let frame = client_message_to_frame_bytes(&handshake, MessageId::new()).unwrap();
        let mut reader = FrameReader::new(frame.as_ref());
        assert!(matches!(
            read_transfer_control_message(&mut reader).await,
            Err(FrameError::UnexpectedMessageType(_))
        ));

        let payload = serde_json::to_vec(&ClientMessage::FileHashing {
            file: "test.txt".into(),
        })
        .unwrap();
        let frame = RawFrame::new(MessageId::new(), "FileStart", payload).to_bytes();
        let mut reader = FrameReader::new(frame.as_slice());
        assert!(matches!(
            read_transfer_control_message(&mut reader).await,
            Err(FrameError::InvalidJson(_))
        ));

        let mut reader = FrameReader::new(tokio::io::empty());
        assert!(
            read_transfer_control_message(&mut reader)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn test_control_write_renews_deadline_after_each_short_write() {
        let message = ServerMessage::FileHashing {
            file: "test.txt".into(),
        };
        let id = MessageId::new();
        let expected = server_message_to_frame_bytes(&message, id).unwrap();
        let delay = TRANSFER_IO_PROGRESS_TIMEOUT * 3 / 4;
        let mut writer = FrameWriter::new(DelayedWriter::new(expected.len() / 2, delay, delay));
        let started = Instant::now();
        timeout(
            TRANSFER_IO_PROGRESS_TIMEOUT * 5,
            send_transfer_server_message_with_progress_timeout(&mut writer, &message, id),
        )
        .await
        .expect("short writes and flush must receive separate progress windows")
        .unwrap();
        assert!(started.elapsed() > TRANSFER_IO_PROGRESS_TIMEOUT);
        assert_eq!(writer.get_mut().bytes, expected.as_ref());
    }

    #[tokio::test(start_paused = true)]
    async fn test_control_write_and_flush_stalls_time_out() {
        let message = ServerMessage::FileHashing {
            file: "test.txt".into(),
        };
        for flush_stalls in [false, true] {
            let stall = TRANSFER_IO_PROGRESS_TIMEOUT * 2;
            let (write_delay, flush_delay) = if flush_stalls {
                (Duration::ZERO, stall)
            } else {
                (stall, Duration::ZERO)
            };
            let mut writer = FrameWriter::new(DelayedWriter::new(8192, write_delay, flush_delay));
            let started = Instant::now();
            let err = timeout(
                TRANSFER_IO_PROGRESS_TIMEOUT * 2,
                send_transfer_server_message_with_progress_timeout(
                    &mut writer,
                    &message,
                    MessageId::new(),
                ),
            )
            .await
            .expect("stalled writes and flushes must time out")
            .unwrap_err();
            assert_eq!(err.to_string(), ERR_TRANSFER_WRITE_PROGRESS_TIMEOUT);
            assert!(started.elapsed() >= TRANSFER_IO_PROGRESS_TIMEOUT);
            assert!(started.elapsed() < TRANSFER_IO_PROGRESS_TIMEOUT + Duration::from_secs(1));
            assert_eq!(writer.get_mut().bytes.is_empty(), !flush_stalls);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn test_control_write_zero_does_not_retry() {
        let mut writer = FrameWriter::new(DelayedWriter::new(0, Duration::ZERO, Duration::ZERO));
        let err = timeout(
            TRANSFER_IO_PROGRESS_TIMEOUT,
            send_transfer_server_message_with_progress_timeout(
                &mut writer,
                &ServerMessage::FileHashing {
                    file: "test.txt".into(),
                },
                MessageId::new(),
            ),
        )
        .await
        .expect("zero-byte writes must fail without retrying")
        .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::WriteZero);
    }

    #[tokio::test(start_paused = true)]
    async fn test_setup_write_retains_whole_operation_deadline() {
        let mut writer = FrameWriter::new(DelayedWriter::new(
            1,
            Duration::from_secs(1),
            Duration::ZERO,
        ));
        let started = Instant::now();
        let message = ServerMessage::HandshakeResponse {
            success: true,
            version: Some(nexus_common::PROTOCOL_VERSION.into()),
            fingerprint: "a".repeat(64),
            error: None,
        };
        let expected = server_message_to_frame_bytes(&message, MessageId::new()).unwrap();
        let err = timeout(
            TRANSFER_SETUP_WRITE_TIMEOUT * 2,
            send_transfer_server_message(&mut writer, &message, MessageId::new()),
        )
        .await
        .expect("setup writes must keep their total deadline")
        .unwrap_err();
        assert_eq!(err.to_string(), ERR_TRANSFER_WRITE_PROGRESS_TIMEOUT);
        assert!(started.elapsed() >= TRANSFER_SETUP_WRITE_TIMEOUT);
        assert!(started.elapsed() < TRANSFER_SETUP_WRITE_TIMEOUT + Duration::from_secs(1));
        assert!(!writer.get_mut().bytes.is_empty());
        assert!(writer.get_mut().bytes.len() < expected.len());
    }

    #[test]
    fn test_transfer_error_display() {
        let err = TransferError::permission("Access denied".to_string());
        assert_eq!(format!("{err}"), "Access denied (permission)");
    }

    #[test]
    fn test_transfer_error_kinds() {
        assert_eq!(TransferError::invalid("x").kind, ERROR_KIND_INVALID);
        assert_eq!(TransferError::not_found("x").kind, ERROR_KIND_NOT_FOUND);
        assert_eq!(TransferError::permission("x").kind, ERROR_KIND_PERMISSION);
        assert_eq!(TransferError::io_error("x").kind, ERROR_KIND_IO_ERROR);
        assert_eq!(
            TransferError::protocol_error("x").kind,
            ERROR_KIND_PROTOCOL_ERROR
        );
        assert_eq!(TransferError::exists("x").kind, ERROR_KIND_EXISTS);
        assert_eq!(TransferError::conflict("x").kind, ERROR_KIND_CONFLICT);
        assert_eq!(
            TransferError::hash_mismatch("x").kind,
            ERROR_KIND_HASH_MISMATCH
        );
        assert_eq!(TransferError::capacity("x").kind, ERROR_KIND_CAPACITY);
    }

    #[test]
    fn test_generate_transfer_id_format() {
        let id = generate_transfer_id();
        assert_eq!(id.len(), 8);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_generate_transfer_id_uniqueness() {
        let ids: Vec<_> = (0..100).map(|_| generate_transfer_id()).collect();
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        // 32 bits of randomness makes collisions in 100 samples extremely unlikely.
        assert!(unique.len() >= 99);
    }

    #[test]
    fn test_check_any_permission_admin_always_ok() {
        let user = make_authenticated_user(true, &[]);
        assert!(check_any_permission(&user, &[Permission::FileUpload], "en").is_ok());
    }

    #[test]
    fn test_check_any_permission_admin_with_empty_list_ok() {
        // Admin passes even against an empty permission list.
        let user = make_authenticated_user(true, &[]);
        assert!(check_any_permission(&user, &[], "en").is_ok());
    }

    #[test]
    fn test_check_any_permission_has_first() {
        let user = make_authenticated_user(false, &[Permission::FileUpload]);
        let result = check_any_permission(
            &user,
            &[Permission::FileUpload, Permission::FileUploadAnywhere],
            "en",
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_any_permission_has_second() {
        // Motivating case: holding only FileUploadAnywhere (no base FileUpload)
        // still grants upload.
        let user = make_authenticated_user(false, &[Permission::FileUploadAnywhere]);
        let result = check_any_permission(
            &user,
            &[Permission::FileUpload, Permission::FileUploadAnywhere],
            "en",
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_any_permission_has_neither() {
        let user = make_authenticated_user(false, &[Permission::FileList]);
        let result = check_any_permission(
            &user,
            &[Permission::FileUpload, Permission::FileUploadAnywhere],
            "en",
        );
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind, ERROR_KIND_PERMISSION);
    }

    #[test]
    fn test_check_any_permission_empty_list_rejects_non_admin() {
        let user = make_authenticated_user(false, &[Permission::FileUpload]);
        assert!(check_any_permission(&user, &[], "en").is_err());
    }
}
