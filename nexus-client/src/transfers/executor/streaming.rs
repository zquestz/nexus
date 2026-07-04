//! Streaming utilities for the transfer executor
//!
//! Provides helpers for reading server messages with timeout, streaming
//! file data directly to disk (downloads), and streaming file data to
//! the server (uploads) with progress tracking and cancellation support.

use nexus_common::hash::StreamingHasher;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::time::timeout;

use nexus_common::framing::{FrameError, FrameHeader, FrameReader, FrameWriter, MessageId};
use nexus_common::io::read_transfer_server_message;
use nexus_common::protocol::{ClientMessage, ServerMessage};

use super::file_utils::is_cancelled;
use super::{
    BUFFER_SIZE, ERR_TRANSFER_CONTROL_WRITE_PROGRESS_TIMEOUT, IDLE_TIMEOUT,
    TRANSFER_CONTROL_WRITE_PROGRESS_TIMEOUT, TransferError,
};
use crate::network::write_timeout::send_client_message_with_progress_timeout;

/// Minimum interval between progress updates (250ms = 4 updates/second)
const PROGRESS_UPDATE_INTERVAL: Duration = Duration::from_millis(250);

pub(super) async fn send_transfer_control_message<W>(
    writer: &mut FrameWriter<W>,
    message: &ClientMessage,
) -> Result<(), TransferError>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    send_client_message_with_progress_timeout(
        writer,
        message,
        MessageId::new(),
        TRANSFER_CONTROL_WRITE_PROGRESS_TIMEOUT,
        ERR_TRANSFER_CONTROL_WRITE_PROGRESS_TIMEOUT,
    )
    .await
    .map_err(|_| TransferError::ConnectionError)
}

/// Error type for streaming operations
#[derive(Debug)]
pub enum StreamError {
    /// Transfer was cancelled by user
    Cancelled,
    /// Frame/protocol error
    Frame(FrameError),
    /// IO error writing to file
    Io,
}

/// Read a server message with timeout
///
/// Automatically skips FileHashing keepalive messages and continues waiting
/// for the next message.
pub async fn read_message_with_timeout<R>(
    reader: &mut FrameReader<R>,
    idle_timeout: Duration,
) -> Result<ServerMessage, TransferError>
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    // Loop to skip any FileHashing keepalive messages
    loop {
        let result = timeout(idle_timeout, read_transfer_server_message(reader)).await;

        match result {
            Ok(Ok(Some(received))) => {
                // Skip FileHashing keepalives
                if matches!(received.message, ServerMessage::FileHashing { .. }) {
                    continue;
                }
                return Ok(received.message);
            }
            Ok(Ok(None)) => return Err(TransferError::ConnectionError),
            Ok(Err(_)) => return Err(TransferError::ProtocolError),
            Err(_) => return Err(TransferError::ConnectionError),
        }
    }
}

/// Dispatch result from reading the next server frame after FileStartResponse
#[derive(Debug)]
pub enum ServerFileFrame {
    /// Server is sending file data
    FileData(FrameHeader),
    /// Server says file is already complete (or zero-byte) — no FileData
    FileHash { blake3: String },
    /// Server terminated the transfer early (error during resume verification, etc.)
    TransferComplete {
        error: Option<String>,
        error_kind: Option<String>,
    },
}

/// Read the next server frame: FileData, FileHash, or TransferComplete
///
/// After the FileStartResponse exchange, the server sends one of:
/// - `FileData` then `FileHash` — data transfer with post-hash verification
/// - `FileHash` alone — file was already complete or zero-byte
/// - `TransferComplete` — transfer terminated early (e.g., resume hash mismatch)
///
/// Automatically skips `FileHashing` keepalive messages.
pub async fn read_file_data_or_file_hash<R>(
    reader: &mut FrameReader<R>,
) -> Result<ServerFileFrame, TransferError>
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    loop {
        let header = match timeout(IDLE_TIMEOUT, reader.read_frame_header()).await {
            Ok(Ok(Some(h))) => h,
            Ok(Ok(None)) => return Err(TransferError::ConnectionError),
            Ok(Err(_)) => return Err(TransferError::ProtocolError),
            Err(_) => return Err(TransferError::ConnectionError),
        };

        match header.message_type.as_str() {
            "FileHashing" => {
                // Keepalive — consume payload and continue
                if reader.read_payload_into_vec(&header).await.is_err() {
                    return Err(TransferError::ProtocolError);
                }
                continue;
            }
            "FileData" => {
                return Ok(ServerFileFrame::FileData(header));
            }
            "FileHash" => {
                let payload = reader
                    .read_payload_into_vec(&header)
                    .await
                    .map_err(|_| TransferError::ProtocolError)?;
                let msg: ServerMessage =
                    serde_json::from_slice(&payload).map_err(|_| TransferError::ProtocolError)?;
                match msg {
                    ServerMessage::FileHash { blake3 } => {
                        return Ok(ServerFileFrame::FileHash { blake3 });
                    }
                    _ => {
                        return Err(TransferError::ProtocolError);
                    }
                }
            }
            "TransferComplete" => {
                let payload = reader
                    .read_payload_into_vec(&header)
                    .await
                    .map_err(|_| TransferError::ProtocolError)?;
                let msg: ServerMessage =
                    serde_json::from_slice(&payload).map_err(|_| TransferError::ProtocolError)?;
                match msg {
                    ServerMessage::TransferComplete {
                        error, error_kind, ..
                    } => {
                        return Ok(ServerFileFrame::TransferComplete { error, error_kind });
                    }
                    _ => {
                        return Err(TransferError::ProtocolError);
                    }
                }
            }
            _ => {
                return Err(TransferError::ProtocolError);
            }
        }
    }
}

/// Stream FileData payload directly to a file with progress-based timeout and cancellation
///
/// This function streams the payload bytes directly to the file without loading
/// the entire payload into memory. The timeout resets each time bytes are received.
/// Cancellation is checked between each chunk read.
///
/// The progress callback is called periodically with the total bytes written so far.
pub async fn stream_payload_to_file_with_progress<R, F>(
    reader: &mut FrameReader<R>,
    header: &FrameHeader,
    file: &mut File,
    progress_timeout: Duration,
    cancel_flag: &Option<Arc<AtomicBool>>,
    mut hasher: Option<&mut StreamingHasher>,
    mut on_progress: F,
) -> Result<u64, StreamError>
where
    R: tokio::io::AsyncBufRead + Unpin,
    F: FnMut(u64),
{
    let mut remaining = header.payload_length;
    let mut total_written: u64 = 0;
    let mut buffer = [0u8; BUFFER_SIZE];
    let mut last_progress_time = Instant::now();

    while remaining > 0 {
        // Check for cancellation before each read
        if is_cancelled(cancel_flag) {
            return Err(StreamError::Cancelled);
        }

        let to_read = (remaining as usize).min(buffer.len());

        // Read with progress timeout - resets on each successful read
        let bytes_read = match timeout(
            progress_timeout,
            reader.get_mut().read(&mut buffer[..to_read]),
        )
        .await
        {
            Ok(Ok(0)) => return Err(StreamError::Frame(FrameError::ConnectionClosed)),
            Ok(Ok(n)) => n,
            Ok(Err(e)) => return Err(StreamError::Frame(FrameError::Io(e.to_string()))),
            Err(_) => return Err(StreamError::Frame(FrameError::FrameTimeout)),
        };

        // Write to file
        file.write_all(&buffer[..bytes_read])
            .await
            .map_err(|_| StreamError::Io)?;

        if let Some(ref mut h) = hasher {
            h.update(&buffer[..bytes_read]);
        }

        remaining -= bytes_read as u64;
        total_written += bytes_read as u64;

        // Rate limit progress updates to reduce UI rebuilds
        if last_progress_time.elapsed() >= PROGRESS_UPDATE_INTERVAL {
            on_progress(total_written);
            last_progress_time = Instant::now();
        }
    }

    // Final progress update (always send to ensure UI shows 100%)
    on_progress(total_written);

    // Flush the file
    file.flush().await.map_err(|_| StreamError::Io)?;

    // Read terminator byte
    let mut terminator = [0u8; 1];
    match timeout(
        progress_timeout,
        reader.get_mut().read_exact(&mut terminator),
    )
    .await
    {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => return Err(StreamError::Frame(FrameError::Io(e.to_string()))),
        Err(_) => return Err(StreamError::Frame(FrameError::FrameTimeout)),
    }

    if terminator[0] != b'\n' {
        return Err(StreamError::Frame(FrameError::MissingTerminator));
    }

    Ok(total_written)
}

// =============================================================================
// Upload Streaming (Client -> Server)
// =============================================================================

/// Stream file data from disk to server with progress-based timeout and cancellation
///
/// This function streams file bytes from the local file to the server using the
/// FileData frame format. The timeout resets each time bytes are sent.
/// Cancellation is checked between each chunk write.
///
/// The progress callback is called periodically with the total bytes sent so far.
///
/// # Arguments
///
/// * `writer` - The frame writer to send data through
/// * `file` - The file to read from (should be positioned at the correct offset)
/// * `bytes_to_send` - Number of bytes to send from the current position
/// * `progress_timeout` - Timeout for progress (must make progress within this time)
/// * `cancel_flag` - Optional flag to check for cancellation
/// * `hasher` - Optional StreamingHasher to feed bytes for single-pass hashing
/// * `on_progress` - Callback called with bytes sent so far
///
/// # Returns
///
/// The total number of bytes sent, or an error.
pub async fn stream_file_to_server<W, F>(
    writer: &mut FrameWriter<W>,
    file: &mut File,
    bytes_to_send: u64,
    progress_timeout: Duration,
    cancel_flag: &Option<Arc<AtomicBool>>,
    mut hasher: Option<&mut StreamingHasher>,
    mut on_progress: F,
) -> Result<u64, StreamError>
where
    W: tokio::io::AsyncWrite + Unpin,
    F: FnMut(u64),
{
    // For zero-byte files, no FileData frame is sent per protocol spec:
    // "0-byte files: FileStart sent, receiver sends FileStartResponse, no FileData, proceed to next file"
    if bytes_to_send == 0 {
        return Ok(0);
    }

    // For non-zero files, we need to use write_streaming_frame to avoid loading
    // the entire file into memory. However, FrameWriter::write_streaming_frame
    // takes an AsyncRead, so we need to wrap our progress/cancellation logic.
    //
    // We'll read the file in chunks, checking for cancellation and sending progress
    // updates, then write each chunk. This is slightly less efficient than a single
    // streaming write, but allows for cancellation and progress tracking.

    let mut reader = BufReader::new(file);
    let mut total_sent: u64 = 0;
    let mut last_progress_time = Instant::now();

    // Build and send the frame header manually to allow chunked writing
    // Format: NX|<type_len>|FileData|<msg_id>|<payload_len>|<payload>\n
    let message_id = MessageId::new();
    let header = format!("NX|8|FileData|{}|{}|", message_id.as_str(), bytes_to_send);

    let header_result = timeout(
        progress_timeout,
        writer.get_mut().write_all(header.as_bytes()),
    )
    .await;

    match header_result {
        Ok(Ok(())) => {}
        Ok(Err(_)) => return Err(StreamError::Io),
        Err(_) => return Err(StreamError::Frame(FrameError::FrameTimeout)),
    }

    // Stream the file data
    let mut buffer = [0u8; BUFFER_SIZE];
    let mut remaining = bytes_to_send;

    while remaining > 0 {
        // Check for cancellation before each read
        if is_cancelled(cancel_flag) {
            return Err(StreamError::Cancelled);
        }

        let to_read = (remaining as usize).min(buffer.len());

        // Read from file
        let bytes_read = match timeout(progress_timeout, reader.read(&mut buffer[..to_read])).await
        {
            Ok(Ok(0)) => {
                // Unexpected EOF - file is shorter than expected
                return Err(StreamError::Io);
            }
            Ok(Ok(n)) => n,
            Ok(Err(_)) => return Err(StreamError::Io),
            Err(_) => return Err(StreamError::Frame(FrameError::FrameTimeout)),
        };

        if let Some(ref mut h) = hasher {
            h.update(&buffer[..bytes_read]);
        }

        // Write to server
        let write_result = timeout(
            progress_timeout,
            writer.get_mut().write_all(&buffer[..bytes_read]),
        )
        .await;

        match write_result {
            Ok(Ok(())) => {}
            Ok(Err(_)) => return Err(StreamError::Io),
            Err(_) => return Err(StreamError::Frame(FrameError::FrameTimeout)),
        }

        remaining -= bytes_read as u64;
        total_sent += bytes_read as u64;

        // Rate limit progress updates to reduce UI rebuilds
        if last_progress_time.elapsed() >= PROGRESS_UPDATE_INTERVAL {
            on_progress(total_sent);
            last_progress_time = Instant::now();
        }
    }

    // Write terminator
    let terminator_result = timeout(progress_timeout, writer.get_mut().write_all(b"\n")).await;

    match terminator_result {
        Ok(Ok(())) => {}
        Ok(Err(_)) => return Err(StreamError::Io),
        Err(_) => return Err(StreamError::Frame(FrameError::FrameTimeout)),
    }

    // Flush
    let flush_result = timeout(progress_timeout, writer.get_mut().flush()).await;

    match flush_result {
        Ok(Ok(())) => {}
        Ok(Err(_)) => return Err(StreamError::Io),
        Err(_) => return Err(StreamError::Frame(FrameError::FrameTimeout)),
    }

    // Final progress update
    on_progress(total_sent);

    Ok(total_sent)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_common::framing::FrameReader;

    /// Helper: build a raw protocol frame from type name and payload bytes
    fn build_frame(type_name: &str, payload: &[u8]) -> Vec<u8> {
        let header = format!(
            "NX|{}|{}|a1b2c3d4e5f6|{}|",
            type_name.len(),
            type_name,
            payload.len()
        );
        let mut frame = header.into_bytes();
        frame.extend_from_slice(payload);
        frame.push(b'\n');
        frame
    }

    #[tokio::test]
    async fn test_read_dispatches_file_data() {
        let frame = build_frame("FileData", b"hello");
        let cursor = std::io::Cursor::new(frame);
        let buf_reader = tokio::io::BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = read_file_data_or_file_hash(&mut reader).await;
        assert!(result.is_ok());
        match result.unwrap() {
            ServerFileFrame::FileData(header) => {
                assert_eq!(header.message_type, "FileData");
                assert_eq!(header.payload_length, 5);
            }
            _ => panic!("Expected FileData"),
        }
    }

    #[tokio::test]
    async fn test_read_dispatches_file_hash() {
        let payload = br#"{"type":"FileHash","blake3":"abc123def456"}"#;
        let frame = build_frame("FileHash", payload);
        let cursor = std::io::Cursor::new(frame);
        let buf_reader = tokio::io::BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = read_file_data_or_file_hash(&mut reader).await;
        assert!(result.is_ok());
        match result.unwrap() {
            ServerFileFrame::FileHash { blake3 } => {
                assert_eq!(blake3, "abc123def456");
            }
            _ => panic!("Expected FileHash"),
        }
    }

    #[tokio::test]
    async fn test_read_dispatches_transfer_complete() {
        let payload =
            br#"{"type":"TransferComplete","success":false,"error":"resume hash mismatch","error_kind":"hash_mismatch"}"#;
        let frame = build_frame("TransferComplete", payload);
        let cursor = std::io::Cursor::new(frame);
        let buf_reader = tokio::io::BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = read_file_data_or_file_hash(&mut reader).await;
        assert!(result.is_ok());
        match result.unwrap() {
            ServerFileFrame::TransferComplete { error, error_kind } => {
                assert_eq!(error.unwrap(), "resume hash mismatch");
                assert_eq!(error_kind.unwrap(), "hash_mismatch");
            }
            _ => panic!("Expected TransferComplete"),
        }
    }

    #[tokio::test]
    async fn test_read_skips_file_hashing_keepalive() {
        let hashing_payload = br#"{"type":"FileHashing","file":"test.txt"}"#;
        let mut data = build_frame("FileHashing", hashing_payload);
        data.extend_from_slice(&build_frame("FileData", b"world"));

        let cursor = std::io::Cursor::new(data);
        let buf_reader = tokio::io::BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = read_file_data_or_file_hash(&mut reader).await;
        assert!(result.is_ok());
        match result.unwrap() {
            ServerFileFrame::FileData(header) => {
                assert_eq!(header.message_type, "FileData");
                assert_eq!(header.payload_length, 5);
            }
            _ => panic!("Expected FileData after skip"),
        }
    }

    #[tokio::test]
    async fn test_read_skips_multiple_keepalives() {
        let hashing_payload = br#"{"type":"FileHashing","file":"big.zip"}"#;
        let hash_payload = br#"{"type":"FileHash","blake3":"deadbeef"}"#;
        let mut data = build_frame("FileHashing", hashing_payload);
        data.extend_from_slice(&build_frame("FileHashing", hashing_payload));
        data.extend_from_slice(&build_frame("FileHash", hash_payload));

        let cursor = std::io::Cursor::new(data);
        let buf_reader = tokio::io::BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = read_file_data_or_file_hash(&mut reader).await;
        assert!(result.is_ok());
        match result.unwrap() {
            ServerFileFrame::FileHash { blake3 } => {
                assert_eq!(blake3, "deadbeef");
            }
            _ => panic!("Expected FileHash"),
        }
    }

    #[tokio::test]
    async fn test_read_rejects_unexpected_message_type() {
        let payload = br#"{"type":"ChatSend","message":"hello"}"#;
        let frame = build_frame("ChatSend", payload);
        let cursor = std::io::Cursor::new(frame);
        let buf_reader = tokio::io::BufReader::new(cursor);
        let mut reader = FrameReader::new(buf_reader);

        let result = read_file_data_or_file_hash(&mut reader).await;
        assert!(result.is_err());
    }
}
