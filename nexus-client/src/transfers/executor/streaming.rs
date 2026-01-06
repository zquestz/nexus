//! Streaming utilities for the transfer executor
//!
//! Provides helpers for reading server messages with timeout, streaming
//! file data directly to disk (downloads), and streaming file data to
//! the server (uploads) with progress tracking and cancellation support.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use tokio::fs::File;
use tokio::io::{AsyncReadExt, BufReader};
use tokio::time::timeout;

use nexus_common::framing::{FrameError, FrameHeader, FrameReader, FrameWriter, MessageId};
use nexus_common::io::read_server_message;
use nexus_common::protocol::ServerMessage;

use super::file_utils::is_cancelled;
use super::{BUFFER_SIZE, TransferError};

/// Minimum interval between progress updates (100ms = 10 updates/second)
const PROGRESS_UPDATE_INTERVAL: Duration = Duration::from_millis(100);

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
        let result = timeout(idle_timeout, read_server_message(reader)).await;

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
    mut on_progress: F,
) -> Result<u64, StreamError>
where
    R: tokio::io::AsyncBufRead + Unpin,
    F: FnMut(u64),
{
    use tokio::io::AsyncWriteExt;

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

        remaining -= bytes_read as u64;
        total_written += bytes_read as u64;

        // Send progress updates at most every PROGRESS_UPDATE_INTERVAL
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
    mut on_progress: F,
) -> Result<u64, StreamError>
where
    W: tokio::io::AsyncWrite + Unpin,
    F: FnMut(u64),
{
    use tokio::io::AsyncWriteExt;

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

        // Send progress updates at most every PROGRESS_UPDATE_INTERVAL
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
