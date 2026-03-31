//! Streaming hash wrappers for file transfers
//!
//! Provides `HashingReader` and `HashingWriter` — transparent `AsyncRead`/`AsyncWrite`
//! wrappers that feed all bytes through a `StreamingHasher`. This enables single-pass
//! hashing during file streaming without modifying the Transfer streaming infrastructure.
//!
//! Also provides `hash_file_with_keepalives` for hashing existing files (partial or
//! complete) while sending periodic `FileHashing` keepalive messages.
//!
//! Used by download.rs (HashingReader: read from disk + hash + send to client)
//! and upload.rs (HashingWriter: receive from client + hash + write to disk).

use std::io;
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;

use nexus_common::framing::{FrameWriter, MessageId};
use nexus_common::hash::StreamingHasher;
use nexus_common::io::send_server_message_with_id;
use nexus_common::protocol::ServerMessage;
use nexus_common::{HASH_BUFFER_SIZE, KEEPALIVE_INTERVAL};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};

pub(crate) use nexus_common::{FALLBACK_FILE_NAME, FALLBACK_PART_FILE_NAME};

/// Wrapper around an `AsyncRead` that feeds all bytes read through it to a
/// `StreamingHasher`. Each successful read updates the hasher with the new bytes
/// before returning them to the caller.
pub(crate) struct HashingReader<'a, R> {
    inner: R,
    hasher: &'a mut StreamingHasher,
}

impl<'a, R> HashingReader<'a, R> {
    pub(crate) fn new(inner: R, hasher: &'a mut StreamingHasher) -> Self {
        Self { inner, hasher }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for HashingReader<'_, R> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let before = buf.filled().len();
        let result = Pin::new(&mut this.inner).poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = &result {
            this.hasher.update(&buf.filled()[before..]);
        }
        result
    }
}

/// Wrapper around an `AsyncWrite` that feeds all written bytes to a
/// `StreamingHasher`. Each successful write updates the hasher with exactly
/// the bytes that were accepted by the inner writer.
pub(crate) struct HashingWriter<'a, W> {
    inner: W,
    hasher: &'a mut StreamingHasher,
}

impl<'a, W> HashingWriter<'a, W> {
    pub(crate) fn new(inner: W, hasher: &'a mut StreamingHasher) -> Self {
        Self { inner, hasher }
    }

    /// Consume the wrapper and return the inner writer.
    pub(crate) fn into_inner(self) -> W {
        self.inner
    }
}

impl<W: AsyncWrite + Unpin> AsyncWrite for HashingWriter<'_, W> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        let result = Pin::new(&mut this.inner).poll_write(cx, buf);
        if let Poll::Ready(Ok(n)) = &result {
            this.hasher.update(&buf[..*n]);
        }
        result
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}

/// Hash a file into a StreamingHasher, sending FileHashing keepalives periodically
///
/// Reads the file from the beginning up to `byte_count` bytes, feeding each chunk
/// into the hasher. Sends keepalive messages to the peer periodically to prevent
/// timeout during large file hashing.
///
/// Used by both download.rs (resume verification) and upload.rs (existing file hashing).
pub(crate) async fn hash_file_with_keepalives<W>(
    path: &Path,
    byte_count: u64,
    file_name: &str,
    frame_writer: &mut FrameWriter<W>,
) -> io::Result<StreamingHasher>
where
    W: AsyncWriteExt + Unpin,
{
    let mut hasher = StreamingHasher::new();
    let file = tokio::fs::File::open(path).await?;
    let mut reader = tokio::io::BufReader::new(file);
    let mut buffer = vec![0u8; HASH_BUFFER_SIZE];
    let mut remaining = byte_count;
    let mut last_keepalive = Instant::now();

    while remaining > 0 {
        let to_read = (remaining as usize).min(buffer.len());
        let bytes_read = reader.read(&mut buffer[..to_read]).await?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
        remaining -= bytes_read as u64;

        // Send keepalive periodically to prevent peer timeout
        if last_keepalive.elapsed() >= KEEPALIVE_INTERVAL {
            let msg = ServerMessage::FileHashing {
                file: file_name.to_string(),
            };
            let _ = send_server_message_with_id(frame_writer, &msg, MessageId::new()).await;
            last_keepalive = Instant::now();
        }
    }

    Ok(hasher)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // =========================================================================
    // HashingReader tests
    // =========================================================================

    #[tokio::test]
    async fn test_hashing_reader_passes_bytes_through() {
        let data = b"Hello, World!";
        let cursor = std::io::Cursor::new(data.to_vec());
        let async_reader = tokio::io::BufReader::new(cursor);
        let mut hasher = StreamingHasher::new();
        let mut hashing_reader = HashingReader::new(async_reader, &mut hasher);

        // Read all bytes through the hashing reader
        let mut output = Vec::new();
        hashing_reader.read_to_end(&mut output).await.unwrap();

        // Bytes should pass through unchanged
        assert_eq!(output, data);

        // Hasher should have accumulated the correct hash
        drop(hashing_reader);
        let hash = hasher.finalize();
        assert_eq!(
            hash,
            "dffd6021bb2bd5b0af676290809ec3a53191dd81c7f70a4b28688a362182986f"
        );
    }

    #[tokio::test]
    async fn test_hashing_reader_chunked_reads() {
        let data = b"Hello, World!";
        let cursor = std::io::Cursor::new(data.to_vec());
        let async_reader = tokio::io::BufReader::new(cursor);
        let mut hasher = StreamingHasher::new();
        let mut hashing_reader = HashingReader::new(async_reader, &mut hasher);

        // Read in small chunks (simulates streaming)
        let mut output = Vec::new();
        let mut buf = [0u8; 3];
        loop {
            let n = hashing_reader.read(&mut buf).await.unwrap();
            if n == 0 {
                break;
            }
            output.extend_from_slice(&buf[..n]);
        }

        assert_eq!(output, data);
        drop(hashing_reader);
        let hash = hasher.finalize();
        assert_eq!(
            hash,
            "dffd6021bb2bd5b0af676290809ec3a53191dd81c7f70a4b28688a362182986f"
        );
    }

    #[tokio::test]
    async fn test_hashing_reader_empty() {
        let cursor = std::io::Cursor::new(Vec::new());
        let async_reader = tokio::io::BufReader::new(cursor);
        let mut hasher = StreamingHasher::new();
        let mut hashing_reader = HashingReader::new(async_reader, &mut hasher);

        let mut output = Vec::new();
        hashing_reader.read_to_end(&mut output).await.unwrap();

        assert!(output.is_empty());
        drop(hashing_reader);
        let hash = hasher.finalize();
        // SHA-256 of empty input
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[tokio::test]
    async fn test_hashing_reader_partial_hash_mid_stream() {
        // Simulate reading a file where we want an intermediate hash
        let data = b"Hello, World!";
        let cursor = std::io::Cursor::new(data.to_vec());
        let async_reader = tokio::io::BufReader::new(cursor);
        let mut hasher = StreamingHasher::new();
        let mut hashing_reader = HashingReader::new(async_reader, &mut hasher);

        // Read first 5 bytes ("Hello")
        let mut buf = [0u8; 5];
        hashing_reader.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"Hello");

        // Get intermediate hash without consuming
        drop(hashing_reader);
        let partial = hasher.partial_hash();
        assert_eq!(
            partial,
            "185f8db32271fe25f561a6fc938b2e264306ec304eda518007d1764826381969"
        );

        // Feed remaining bytes manually and finalize
        hasher.update(b", World!");
        let full = hasher.finalize();
        assert_eq!(
            full,
            "dffd6021bb2bd5b0af676290809ec3a53191dd81c7f70a4b28688a362182986f"
        );
    }

    // =========================================================================
    // HashingWriter tests
    // =========================================================================

    #[tokio::test]
    async fn test_hashing_writer_passes_bytes_through() {
        let mut hasher = StreamingHasher::new();
        {
            let mut writer = HashingWriter::new(Vec::<u8>::new(), &mut hasher);
            writer.write_all(b"Hello, World!").await.unwrap();
            writer.flush().await.unwrap();

            // Bytes should pass through to inner writer
            let output = writer.into_inner();
            assert_eq!(output.as_slice(), b"Hello, World!");
        }
        // writer dropped, hasher borrow released
        let hash = hasher.finalize();
        assert_eq!(
            hash,
            "dffd6021bb2bd5b0af676290809ec3a53191dd81c7f70a4b28688a362182986f"
        );
    }

    #[tokio::test]
    async fn test_hashing_writer_chunked_writes() {
        let mut hasher = StreamingHasher::new();
        {
            let mut writer = HashingWriter::new(Vec::<u8>::new(), &mut hasher);
            // Write in small chunks (simulates streaming reception)
            writer.write_all(b"Hello").await.unwrap();
            writer.write_all(b", ").await.unwrap();
            writer.write_all(b"World!").await.unwrap();
            writer.flush().await.unwrap();

            let output = writer.into_inner();
            assert_eq!(output.as_slice(), b"Hello, World!");
        }
        let hash = hasher.finalize();
        assert_eq!(
            hash,
            "dffd6021bb2bd5b0af676290809ec3a53191dd81c7f70a4b28688a362182986f"
        );
    }

    #[tokio::test]
    async fn test_hashing_writer_empty() {
        let mut hasher = StreamingHasher::new();
        {
            let mut writer = HashingWriter::new(Vec::<u8>::new(), &mut hasher);
            writer.flush().await.unwrap();
            let output = writer.into_inner();
            assert!(output.is_empty());
        }
        let hash = hasher.finalize();
        // SHA-256 of empty input
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
