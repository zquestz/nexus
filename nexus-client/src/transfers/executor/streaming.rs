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
use tokio::time::{timeout, timeout_at};

use nexus_common::framing::{FrameError, FrameHeader, FrameReader, FrameWriter, MessageId};
use nexus_common::io::read_transfer_server_message;
use nexus_common::protocol::{ClientMessage, ServerMessage};
use nexus_common::{TRANSFER_CONTROL_FRAME_TIMEOUT, TRANSFER_IO_PROGRESS_TIMEOUT};

use super::file_utils::is_cancelled;
use super::{BUFFER_SIZE, ERR_TRANSFER_CONTROL_WRITE_PROGRESS_TIMEOUT, TransferError};
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
        TRANSFER_IO_PROGRESS_TIMEOUT,
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
    /// File or socket I/O error
    Io,
}

/// Read a server message within one total frame deadline, including idle wait.
///
/// Automatically skips FileHashing keepalive messages and continues waiting
/// for the next message.
pub async fn read_message_with_timeout<R>(
    reader: &mut FrameReader<R>,
    frame_timeout: Duration,
) -> Result<ServerMessage, TransferError>
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    // Loop to skip any FileHashing keepalive messages
    loop {
        let result = timeout(frame_timeout, read_transfer_server_message(reader)).await;

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
        let deadline = tokio::time::Instant::now() + TRANSFER_CONTROL_FRAME_TIMEOUT;
        let header = match timeout_at(deadline, reader.read_frame_header()).await {
            Ok(Ok(Some(h))) => h,
            Ok(Ok(None)) => return Err(TransferError::ConnectionError),
            Ok(Err(_)) => return Err(TransferError::ProtocolError),
            Err(_) => return Err(TransferError::ConnectionError),
        };

        match header.message_type.as_str() {
            "FileData" => {
                return Ok(ServerFileFrame::FileData(header));
            }
            "FileHashing" | "FileHash" | "TransferComplete" => {
                // Control payload and terminator must finish within the header's deadline.
                let payload = timeout_at(deadline, reader.read_payload_into_vec(&header))
                    .await
                    .map_err(|_| TransferError::ConnectionError)?
                    .map_err(|_| TransferError::ProtocolError)?;
                let msg: ServerMessage =
                    serde_json::from_slice(&payload).map_err(|_| TransferError::ProtocolError)?;
                match (header.message_type.as_str(), msg) {
                    ("FileHashing", ServerMessage::FileHashing { .. }) => continue,
                    ("FileHash", ServerMessage::FileHash { blake3 }) => {
                        return Ok(ServerFileFrame::FileHash { blake3 });
                    }
                    (
                        "TransferComplete",
                        ServerMessage::TransferComplete {
                            error, error_kind, ..
                        },
                    ) => {
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
/// Cancellation is checked between individual socket writes.
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

    write_transfer_bytes(
        writer.get_mut(),
        header.as_bytes(),
        progress_timeout,
        cancel_flag,
    )
    .await?;

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
        write_transfer_bytes(
            writer.get_mut(),
            &buffer[..bytes_read],
            progress_timeout,
            cancel_flag,
        )
        .await?;

        remaining -= bytes_read as u64;
        total_sent += bytes_read as u64;

        // Rate limit progress updates to reduce UI rebuilds
        if last_progress_time.elapsed() >= PROGRESS_UPDATE_INTERVAL {
            on_progress(total_sent);
            last_progress_time = Instant::now();
        }
    }

    // Write terminator
    write_transfer_bytes(writer.get_mut(), b"\n", progress_timeout, cancel_flag).await?;

    if is_cancelled(cancel_flag) {
        return Err(StreamError::Cancelled);
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

/// FileData writes need per-write cancellation as well as per-write deadlines.
async fn write_transfer_bytes<W>(
    writer: &mut W,
    mut bytes: &[u8],
    progress_timeout: Duration,
    cancel_flag: &Option<Arc<AtomicBool>>,
) -> Result<(), StreamError>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    while !bytes.is_empty() {
        if is_cancelled(cancel_flag) {
            return Err(StreamError::Cancelled);
        }
        let written = match timeout(progress_timeout, writer.write(bytes)).await {
            Ok(Ok(0)) | Ok(Err(_)) => return Err(StreamError::Io),
            Ok(Ok(n)) => n,
            Err(_) => return Err(StreamError::Frame(FrameError::FrameTimeout)),
        };
        bytes = &bytes[written..];
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::io;
    use std::pin::Pin;
    use std::sync::atomic::Ordering;
    use std::task::{Context, Poll, ready};

    use nexus_common::framing::FrameReader;
    use nexus_common::io::{
        read_transfer_client_message_with_full_timeout, server_message_to_frame_bytes,
    };
    use tokio::io::{AsyncWrite, duplex};
    use tokio::time::{Instant as TokioInstant, Sleep, sleep};

    use super::super::test_helpers::{FailingWriter, WriteFailure};

    struct SlowWriter {
        bytes: Vec<u8>,
        chunk_size: usize,
        delay: Option<Pin<Box<Sleep>>>,
        cancel_after: Option<(usize, Arc<AtomicBool>)>,
    }

    impl SlowWriter {
        fn new() -> Self {
            Self {
                bytes: Vec::new(),
                chunk_size: 4,
                delay: None,
                cancel_after: None,
            }
        }

        fn poll_delay(&mut self, cx: &mut Context<'_>) -> Poll<()> {
            let delay = self
                .delay
                .get_or_insert_with(|| Box::pin(sleep(TRANSFER_IO_PROGRESS_TIMEOUT * 3 / 4)));
            ready!(delay.as_mut().poll(cx));
            self.delay = None;
            Poll::Ready(())
        }
    }

    impl AsyncWrite for SlowWriter {
        fn poll_write(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            let this = self.get_mut();
            ready!(this.poll_delay(cx));
            let len = buf.len().min(this.chunk_size);
            this.bytes.extend_from_slice(&buf[..len]);
            if let Some((limit, flag)) = &this.cancel_after
                && this.bytes.len() >= *limit
            {
                flag.store(true, Ordering::Relaxed);
            }
            Poll::Ready(Ok(len))
        }

        fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            ready!(self.get_mut().poll_delay(cx));
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    #[tokio::test(start_paused = true)]
    async fn test_control_short_writes_and_flush_have_separate_progress_windows() {
        let mut writer = FrameWriter::new(SlowWriter::new());
        let message = ClientMessage::FileHashing {
            file: "test.txt".into(),
        };
        let started = TokioInstant::now();
        timeout(
            TRANSFER_IO_PROGRESS_TIMEOUT * 40,
            send_transfer_control_message(&mut writer, &message),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(started.elapsed() > TRANSFER_IO_PROGRESS_TIMEOUT);
        let mut reader = FrameReader::new(writer.get_ref().bytes.as_slice());
        assert!(matches!(
            read_transfer_client_message_with_full_timeout(&mut reader, None, None).await.unwrap().unwrap().message,
            ClientMessage::FileHashing { file } if file == "test.txt"
        ));
        assert!(reader.get_ref().is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn test_control_write_failures_are_reported() {
        for failure in [
            WriteFailure::Error,
            WriteFailure::Zero,
            WriteFailure::Stall,
            WriteFailure::FlushError,
            WriteFailure::FlushStall,
        ] {
            let mut writer = FrameWriter::new(FailingWriter::new(0, 8, failure));
            let started = TokioInstant::now();
            let result = timeout(
                TRANSFER_IO_PROGRESS_TIMEOUT * 2,
                send_transfer_control_message(
                    &mut writer,
                    &ClientMessage::FileHashing {
                        file: "test.txt".into(),
                    },
                ),
            )
            .await
            .unwrap();
            assert_eq!(result, Err(TransferError::ConnectionError));
            if matches!(failure, WriteFailure::Stall | WriteFailure::FlushStall) {
                assert!(started.elapsed() >= TRANSFER_IO_PROGRESS_TIMEOUT);
                assert!(started.elapsed() < TRANSFER_IO_PROGRESS_TIMEOUT + Duration::from_secs(1));
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn test_upload_short_writes_keep_one_frame_and_correct_hash() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("upload.bin");
        let data = b"a payload requiring many writes";
        tokio::fs::write(&path, data).await.unwrap();
        let mut file = File::open(&path).await.unwrap();
        let mut writer = FrameWriter::new(SlowWriter::new());
        let mut hasher = StreamingHasher::new();
        let mut progress = Vec::new();
        let started = TokioInstant::now();
        let result = timeout(
            TRANSFER_IO_PROGRESS_TIMEOUT * 40,
            stream_file_to_server(
                &mut writer,
                &mut file,
                data.len() as u64,
                TRANSFER_IO_PROGRESS_TIMEOUT,
                &None,
                Some(&mut hasher),
                |sent| progress.push(sent),
            ),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(result, data.len() as u64);
        assert!(started.elapsed() > TRANSFER_IO_PROGRESS_TIMEOUT);
        assert_eq!(progress.last(), Some(&(data.len() as u64)));
        let mut expected_hash = StreamingHasher::new();
        expected_hash.update(data);
        assert_eq!(hasher.finalize(), expected_hash.finalize());
        let mut reader = FrameReader::new(writer.get_ref().bytes.as_slice());
        let header = reader.read_frame_header().await.unwrap().unwrap();
        assert_eq!(header.message_type, "FileData");
        assert_eq!(reader.read_payload_into_vec(&header).await.unwrap(), data);
        assert!(reader.get_ref().is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn test_upload_write_failures_stop_at_header_payload_or_terminator() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("upload.bin");
        let data = b"a payload requiring many writes";
        tokio::fs::write(&path, data).await.unwrap();
        let header_len = build_frame("FileData", data).len() - data.len() - 1;
        for failure in [
            WriteFailure::Error,
            WriteFailure::Zero,
            WriteFailure::Stall,
            WriteFailure::FlushError,
            WriteFailure::FlushStall,
        ] {
            for prefix in [8, header_len + 3, header_len + data.len()] {
                let mut file = File::open(&path).await.unwrap();
                let mut writer = FrameWriter::new(FailingWriter::new(0, prefix, failure));
                let result = timeout(
                    TRANSFER_IO_PROGRESS_TIMEOUT * 2,
                    stream_file_to_server(
                        &mut writer,
                        &mut file,
                        data.len() as u64,
                        TRANSFER_IO_PROGRESS_TIMEOUT,
                        &None,
                        None,
                        |_| {},
                    ),
                )
                .await
                .unwrap();
                if matches!(failure, WriteFailure::Stall | WriteFailure::FlushStall) {
                    assert!(matches!(
                        result,
                        Err(StreamError::Frame(FrameError::FrameTimeout))
                    ));
                } else {
                    assert!(matches!(result, Err(StreamError::Io)));
                }
                if !matches!(failure, WriteFailure::FlushError | WriteFailure::FlushStall) {
                    assert_eq!(writer.get_ref().bytes.len(), prefix);
                }
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn test_upload_cancellation_is_checked_between_short_writes() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("upload.bin");
        let data = b"a payload requiring many writes";
        tokio::fs::write(&path, data).await.unwrap();
        let mut file = File::open(&path).await.unwrap();
        let header_len = build_frame("FileData", data).len() - data.len() - 1;
        let cancel = Arc::new(AtomicBool::new(false));
        let mut slow_writer = SlowWriter::new();
        slow_writer.cancel_after = Some((header_len + slow_writer.chunk_size, Arc::clone(&cancel)));
        let mut writer = FrameWriter::new(slow_writer);
        let result = timeout(
            TRANSFER_IO_PROGRESS_TIMEOUT * 40,
            stream_file_to_server(
                &mut writer,
                &mut file,
                data.len() as u64,
                TRANSFER_IO_PROGRESS_TIMEOUT,
                &Some(cancel),
                None,
                |_| {},
            ),
        )
        .await
        .unwrap();
        assert!(matches!(result, Err(StreamError::Cancelled)));
        assert_eq!(writer.get_ref().bytes.len(), header_len + 4);
    }

    async fn read_control<R: tokio::io::AsyncBufRead + Unpin>(
        reader: &mut FrameReader<R>,
        mixed_reader: bool,
    ) -> Result<(), TransferError> {
        if mixed_reader {
            read_file_data_or_file_hash(reader).await.map(|_| ())
        } else {
            read_message_with_timeout(reader, TRANSFER_CONTROL_FRAME_TIMEOUT)
                .await
                .map(|_| ())
        }
    }

    #[tokio::test(start_paused = true)]
    async fn test_control_read_stalls_include_payload_and_terminator() {
        for message in [
            ServerMessage::FileHashing {
                file: "test.txt".into(),
            },
            ServerMessage::FileHash {
                blake3: "abc123".into(),
            },
            ServerMessage::TransferComplete {
                success: false,
                error: None,
                error_kind: None,
            },
        ] {
            let frame = server_message_to_frame_bytes(&message, MessageId::new()).unwrap();
            let payload_len = serde_json::to_vec(&message).unwrap().len();
            let header_len = frame.len() - payload_len - 1;
            for mixed_reader in [false, true] {
                for prefix_len in [
                    0,
                    1,
                    header_len,
                    header_len + payload_len / 2,
                    frame.len() - 1,
                ] {
                    let (mut sender, receiver) = duplex(8192);
                    sender.write_all(&frame[..prefix_len]).await.unwrap();
                    let mut reader = FrameReader::new(BufReader::new(receiver));
                    let started = TokioInstant::now();
                    let result = timeout(
                        TRANSFER_CONTROL_FRAME_TIMEOUT * 2,
                        read_control(&mut reader, mixed_reader),
                    )
                    .await
                    .unwrap();
                    assert_eq!(
                        result,
                        Err(TransferError::ConnectionError),
                        "mixed={mixed_reader}, prefix={prefix_len}"
                    );
                    assert!(started.elapsed() >= TRANSFER_CONTROL_FRAME_TIMEOUT);
                    assert!(
                        started.elapsed() < TRANSFER_CONTROL_FRAME_TIMEOUT + Duration::from_secs(1)
                    );
                }
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn test_control_idle_and_payload_share_one_deadline() {
        for mixed_reader in [false, true] {
            let message = ServerMessage::FileHashing {
                file: "test.txt".into(),
            };
            let frame = server_message_to_frame_bytes(&message, MessageId::new()).unwrap();
            let header_len = frame.len() - serde_json::to_vec(&message).unwrap().len() - 1;
            let (mut sender, receiver) = duplex(8192);
            let mut reader = FrameReader::new(BufReader::new(receiver));
            let read = async {
                let started = TokioInstant::now();
                let result = timeout(
                    TRANSFER_CONTROL_FRAME_TIMEOUT * 2,
                    read_control(&mut reader, mixed_reader),
                )
                .await
                .unwrap();
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
            assert_eq!(result, Err(TransferError::ConnectionError));
            assert!(elapsed >= TRANSFER_CONTROL_FRAME_TIMEOUT);
            assert!(elapsed < TRANSFER_CONTROL_FRAME_TIMEOUT + Duration::from_secs(1));
        }
    }

    #[tokio::test(start_paused = true)]
    async fn test_complete_keepalives_renew_control_deadline() {
        for mixed_reader in [false, true] {
            let (mut sender, receiver) = duplex(8192);
            let mut reader = FrameReader::new(BufReader::new(receiver));
            let started = TokioInstant::now();
            let read = read_control(&mut reader, mixed_reader);
            let send = async {
                let keepalive = server_message_to_frame_bytes(
                    &ServerMessage::FileHashing {
                        file: "test.txt".into(),
                    },
                    MessageId::new(),
                )
                .unwrap();
                for _ in 0..3 {
                    sleep(TRANSFER_CONTROL_FRAME_TIMEOUT * 3 / 4).await;
                    sender.write_all(&keepalive).await.unwrap();
                }
                sleep(TRANSFER_CONTROL_FRAME_TIMEOUT * 3 / 4).await;
                sender
                    .write_all(
                        &server_message_to_frame_bytes(
                            &ServerMessage::FileHash {
                                blake3: "abc123".into(),
                            },
                            MessageId::new(),
                        )
                        .unwrap(),
                    )
                    .await
                    .unwrap();
            };
            let (result, ()) = timeout(TRANSFER_CONTROL_FRAME_TIMEOUT * 5, async {
                tokio::join!(read, send)
            })
            .await
            .unwrap();
            result.unwrap();
            assert!(started.elapsed() > TRANSFER_CONTROL_FRAME_TIMEOUT);
        }
    }

    #[tokio::test]
    async fn test_malformed_keepalives_do_not_renew_control_wait() {
        for payload in [
            b"not JSON".as_slice(),
            br#"{"type":"FileHashing"}"#,
            br#"{"type":"FileHash","blake3":"abc123"}"#,
        ] {
            for mixed_reader in [false, true] {
                let mut frames = build_frame("FileHashing", payload);
                frames.extend_from_slice(&build_frame(
                    "FileHash",
                    br#"{"type":"FileHash","blake3":"abc123"}"#,
                ));
                let mut reader = FrameReader::new(frames.as_slice());
                assert_eq!(
                    read_control(&mut reader, mixed_reader).await,
                    Err(TransferError::ProtocolError)
                );
                assert!(
                    !reader.get_ref().is_empty(),
                    "must not consume the next frame"
                );
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn test_download_payload_keeps_separate_progress_timeout() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("download.part");
        let mut file = File::create(&path).await.unwrap();
        let frame = build_frame("FileData", b"abc");
        let header_len = frame.len() - 4;
        let (mut sender, receiver) = duplex(8192);
        sender.write_all(&frame[..header_len]).await.unwrap();
        let mut reader = FrameReader::new(BufReader::new(receiver));
        let header = match read_file_data_or_file_hash(&mut reader).await.unwrap() {
            ServerFileFrame::FileData(header) => header,
            other => panic!("expected FileData, got {other:?}"),
        };
        let started = TokioInstant::now();
        let read = stream_payload_to_file_with_progress(
            &mut reader,
            &header,
            &mut file,
            TRANSFER_IO_PROGRESS_TIMEOUT,
            &None,
            None,
            |_| {},
        );
        let send = async {
            for byte in b"abc\n" {
                sleep(TRANSFER_IO_PROGRESS_TIMEOUT * 3 / 4).await;
                sender.write_all(&[*byte]).await.unwrap();
            }
        };
        let (result, ()) = timeout(TRANSFER_IO_PROGRESS_TIMEOUT * 5, async {
            tokio::join!(read, send)
        })
        .await
        .unwrap();
        assert_eq!(result.unwrap(), 3);
        assert!(started.elapsed() > TRANSFER_CONTROL_FRAME_TIMEOUT);
        assert_eq!(tokio::fs::read(&path).await.unwrap(), b"abc");
    }

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
