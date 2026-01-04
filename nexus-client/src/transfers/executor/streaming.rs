//! Streaming utilities for the transfer executor
//!
//! Provides helpers for reading server messages with timeout and streaming
//! file data directly to disk with progress tracking and cancellation support.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::time::timeout;

use nexus_common::framing::{FrameError, FrameHeader, FrameReader};
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
pub async fn read_message_with_timeout<R>(
    reader: &mut FrameReader<R>,
    idle_timeout: Duration,
) -> Result<ServerMessage, TransferError>
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    let result = timeout(idle_timeout, read_server_message(reader)).await;

    match result {
        Ok(Ok(Some(received))) => Ok(received.message),
        Ok(Ok(None)) => Err(TransferError::ConnectionError),
        Ok(Err(_)) => Err(TransferError::ProtocolError),
        Err(_) => Err(TransferError::ConnectionError),
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
