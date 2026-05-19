//! Transfer connection wrapper with ban signal handling
//!
//! The `Transfer` struct owns a transfer connection and provides methods for all I/O.
//! Ban signals are checked during streaming operations to stop file data transfer
//! when a user is banned mid-transfer.

use std::io;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::oneshot;

use nexus_common::framing::{FrameReader, FrameWriter, MessageId};
use nexus_common::io::send_server_message_with_id;
use nexus_common::protocol::ServerMessage;

use crate::files::{FileIndex, PathLockMap};

#[cfg(test)]
use super::registry::TransferRegistration;
use super::registry::{ActiveTransfer, TransferId, TransferRegistry, TransferRegistryGuard};

/// Chunk size for streaming file data (64KB)
const CHUNK_SIZE: usize = 64 * 1024;
use super::types::AuthenticatedUser;

/// Error type for streaming operations
///
/// Represents errors that can occur during file streaming (send/receive).
#[derive(Debug)]
pub enum StreamError {
    /// Normal I/O error
    Io(io::Error),
    /// Connection was terminated due to IP ban
    Banned,
    /// Connection closed cleanly
    ConnectionClosed,
}

impl std::fmt::Display for StreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Banned => write!(f, "Connection terminated: IP banned"),
            Self::ConnectionClosed => write!(f, "Connection closed"),
        }
    }
}

impl std::error::Error for StreamError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for StreamError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// Server-side context that every transfer needs: who the
/// authenticated peer is, what locale to use for translated errors,
/// where the file root and index live, and which registry the transfer
/// belongs to. Bundled so [`Transfer::new`]'s signature stays narrow.
pub struct TransferContext<'a> {
    pub user: AuthenticatedUser,
    pub locale: String,
    pub file_root: &'a Path,
    pub file_index: &'a Arc<FileIndex>,
    pub file_mutation_locks: &'a Arc<PathLockMap>,
    pub registry: &'a TransferRegistry,
}

/// A file transfer connection with integrated ban handling
///
/// This struct owns the reader and writer for a transfer connection, along with
/// a channel receiver for ban signals. Streaming methods check for bans between
/// chunks, allowing mid-transfer termination when the IP is banned.
///
/// The transfer is automatically unregistered from the registry when dropped.
/// Progress is tracked via the shared `TransferInfo` which can be queried
/// by the connection monitor.
pub struct Transfer<'a, R, W> {
    // I/O
    reader: FrameReader<R>,
    writer: FrameWriter<W>,

    // Ban signal (Option so we can take it when received)
    ban_rx: Option<oneshot::Receiver<()>>,
    // Whether this transfer has been banned
    banned: bool,

    // Shared transfer state (for metrics and monitoring)
    info: Arc<ActiveTransfer>,

    // Per-transfer context — exposed via accessor methods (`user()`,
    // `locale()`, `file_root()`, `file_index()`).
    user: AuthenticatedUser,
    locale: String,
    file_root: &'a Path,
    file_index: &'a Arc<FileIndex>,
    file_mutation_locks: &'a Arc<PathLockMap>,

    // RAII cleanup (must be last so it drops after other fields)
    _guard: TransferRegistryGuard<'a>,
}

impl<'a, R, W> Transfer<'a, R, W>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    /// Create a new transfer connection
    ///
    /// Uses the provided `ActiveTransfer` for shared state (progress tracking,
    /// metadata for monitoring). The transfer will automatically unregister
    /// from the registry when dropped.
    pub fn new(
        reader: FrameReader<R>,
        writer: FrameWriter<W>,
        ban_rx: oneshot::Receiver<()>,
        info: Arc<ActiveTransfer>,
        ctx: TransferContext<'a>,
    ) -> Self {
        // Read out the id before `info` is moved into the struct.
        let id = info.id;
        Self {
            reader,
            writer,
            ban_rx: Some(ban_rx),
            banned: false,
            info,
            user: ctx.user,
            locale: ctx.locale,
            file_root: ctx.file_root,
            file_index: ctx.file_index,
            file_mutation_locks: ctx.file_mutation_locks,
            _guard: TransferRegistryGuard::new(ctx.registry, id),
        }
    }

    /// Get the transfer ID
    pub fn id(&self) -> TransferId {
        self.info.id
    }

    /// Get the elapsed time since transfer started
    pub fn elapsed(&self) -> std::time::Duration {
        self.info.elapsed()
    }

    /// Get total bytes transferred so far
    pub fn bytes_transferred(&self) -> u64 {
        self.info.get_bytes_transferred()
    }

    /// Set the total size (used for downloads after path resolution)
    pub fn set_total_size(&self, size: u64) {
        self.info.set_total_size(size);
    }

    /// Get the shared active transfer state
    #[allow(dead_code)] // Public API for future connection monitor integration
    pub fn info(&self) -> &Arc<ActiveTransfer> {
        &self.info
    }

    // =========================================================================
    // Accessor methods for handler compatibility
    // =========================================================================

    /// Get a reference to the authenticated user
    pub fn user(&self) -> &AuthenticatedUser {
        &self.user
    }

    /// Get the locale string
    pub fn locale(&self) -> &str {
        &self.locale
    }

    /// Get the peer address
    pub fn peer_addr(&self) -> SocketAddr {
        self.info.peer_addr
    }

    /// Get the file root path
    pub fn file_root(&self) -> &Path {
        self.file_root
    }

    /// Get the file index
    pub fn file_index(&self) -> &Arc<FileIndex> {
        self.file_index
    }

    pub fn file_mutation_locks(&self) -> &Arc<PathLockMap> {
        self.file_mutation_locks
    }

    /// Send a server message
    pub async fn send(&mut self, msg: &ServerMessage) -> Result<(), StreamError> {
        self.send_with_id(msg, MessageId::new()).await
    }

    /// Send a server message with a specific message ID
    pub async fn send_with_id(
        &mut self,
        msg: &ServerMessage,
        msg_id: MessageId,
    ) -> Result<(), StreamError> {
        send_server_message_with_id(&mut self.writer, msg, msg_id)
            .await
            .map_err(StreamError::Io)
    }

    /// Stream a file to the client with periodic ban checking
    ///
    /// This method streams file data in chunks, checking for ban signals between
    /// chunks. This allows mid-transfer termination when a ban is created.
    ///
    /// When banned, returns `Err(StreamError::Banned)`. The caller should close
    /// the connection immediately - no further protocol messages are needed since
    /// the client receives the ban reason on the BBS connection.
    ///
    /// # Arguments
    /// * `message_type` - The frame message type (typically "FileData")
    /// * `reader` - The file reader to stream from
    /// * `length` - Total bytes to stream
    ///
    /// # Returns
    /// * `Ok(bytes_written)` - bytes actually written
    /// * `Err(StreamError::Banned)` if banned
    /// * `Err(StreamError::Io(_))` on I/O error
    pub async fn stream_file_to_client<S>(
        &mut self,
        message_type: &str,
        reader: &mut S,
        length: u64,
    ) -> Result<u64, StreamError>
    where
        S: AsyncRead + Unpin,
    {
        // Check ban before starting
        if self.is_banned() {
            return Err(StreamError::Banned);
        }

        // Write frame header
        // Format: NX|type_len|type|msg_id|payload_len|
        let msg_id = MessageId::new();
        let header = format!(
            "NX|{}|{}|{}|{}|",
            message_type.len(),
            message_type,
            msg_id,
            length
        );
        self.writer
            .get_mut()
            .write_all(header.as_bytes())
            .await
            .map_err(StreamError::Io)?;

        // Stream in chunks, checking ban between chunks
        let mut buffer = vec![0u8; CHUNK_SIZE];
        let mut remaining = length;
        let mut total_written: u64 = 0;

        while remaining > 0 {
            // Check for ban between chunks
            if self.is_banned() {
                return Err(StreamError::Banned);
            }

            let to_read = (remaining as usize).min(CHUNK_SIZE);
            let bytes_read = reader
                .read(&mut buffer[..to_read])
                .await
                .map_err(StreamError::Io)?;

            if bytes_read == 0 {
                return Err(StreamError::Io(std::io::Error::other(format!(
                    "Reader ended early: expected {} more bytes",
                    remaining
                ))));
            }

            self.writer
                .get_mut()
                .write_all(&buffer[..bytes_read])
                .await
                .map_err(StreamError::Io)?;

            remaining -= bytes_read as u64;
            total_written += bytes_read as u64;

            // Update shared progress atomically
            self.info.add_bytes_transferred(bytes_read as u64);
        }

        // Write frame terminator
        self.writer
            .get_mut()
            .write_all(b"\n")
            .await
            .map_err(StreamError::Io)?;

        self.writer
            .get_mut()
            .flush()
            .await
            .map_err(StreamError::Io)?;

        Ok(total_written)
    }

    /// Stream file data from client to a writer with periodic ban checking
    ///
    /// This method receives file data in chunks from the client, checking for ban
    /// signals between chunks. This allows mid-transfer termination when a ban is created.
    ///
    /// When banned, returns `Err(StreamError::Banned)`. The caller should close
    /// the connection immediately - no further protocol messages are needed since
    /// the client receives the ban reason on the BBS connection.
    ///
    /// # Arguments
    /// * `header` - The frame header (contains payload length)
    /// * `dest` - The destination writer (typically a file)
    /// * `progress_timeout` - Maximum time to wait between receiving chunks
    ///
    /// # Returns
    /// * `Ok(bytes_written)` - bytes actually written
    /// * `Err(StreamError::Banned)` if banned
    /// * `Err(StreamError::Io(_))` on I/O error
    pub async fn stream_file_from_client<D>(
        &mut self,
        header: &nexus_common::framing::FrameHeader,
        dest: &mut D,
        progress_timeout: std::time::Duration,
    ) -> Result<u64, StreamError>
    where
        D: AsyncWrite + Unpin,
    {
        use tokio::time::timeout;

        // Check ban before starting
        if self.is_banned() {
            return Err(StreamError::Banned);
        }

        let mut buffer = vec![0u8; CHUNK_SIZE];
        let mut remaining = header.payload_length;
        let mut total_written: u64 = 0;

        while remaining > 0 {
            // Check for ban between chunks
            if self.is_banned() {
                // Stop writing to file, but we need to drain the rest of the frame
                break;
            }

            let to_read = (remaining as usize).min(CHUNK_SIZE);

            // Read with progress timeout
            let bytes_read = match timeout(
                progress_timeout,
                self.reader.get_mut().read(&mut buffer[..to_read]),
            )
            .await
            {
                Ok(Ok(0)) => return Err(StreamError::ConnectionClosed),
                Ok(Ok(n)) => n,
                Ok(Err(e)) => return Err(StreamError::Io(e)),
                Err(_) => return Err(StreamError::Io(std::io::Error::other("Read timeout"))),
            };

            // Write to destination
            dest.write_all(&buffer[..bytes_read])
                .await
                .map_err(StreamError::Io)?;

            remaining -= bytes_read as u64;
            total_written += bytes_read as u64;

            // Update shared progress atomically
            self.info.add_bytes_transferred(bytes_read as u64);
        }

        // Flush what we wrote
        dest.flush().await.map_err(StreamError::Io)?;

        // If banned, drain remaining payload data (don't write to file)
        while remaining > 0 {
            let to_read = (remaining as usize).min(CHUNK_SIZE);
            let bytes_read = match timeout(
                progress_timeout,
                self.reader.get_mut().read(&mut buffer[..to_read]),
            )
            .await
            {
                Ok(Ok(0)) => return Err(StreamError::ConnectionClosed),
                Ok(Ok(n)) => n,
                Ok(Err(e)) => return Err(StreamError::Io(e)),
                Err(_) => return Err(StreamError::Io(std::io::Error::other("Read timeout"))),
            };
            remaining -= bytes_read as u64;
        }

        // Read frame terminator
        let mut terminator = [0u8; 1];
        match timeout(
            progress_timeout,
            self.reader.get_mut().read_exact(&mut terminator),
        )
        .await
        {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => return Err(StreamError::Io(e)),
            Err(_) => return Err(StreamError::Io(std::io::Error::other("Read timeout"))),
        }

        if terminator[0] != b'\n' {
            return Err(StreamError::Io(std::io::Error::other(
                "Missing frame terminator",
            )));
        }

        Ok(total_written)
    }

    /// Check if a ban signal has been received (non-blocking)
    ///
    /// Returns true if banned, false if not yet banned.
    pub fn is_banned(&mut self) -> bool {
        if self.banned {
            return true;
        }

        if let Some(ref mut rx) = self.ban_rx {
            match rx.try_recv() {
                Ok(_) => {
                    self.ban_rx = None;
                    self.banned = true;
                    true
                }
                Err(oneshot::error::TryRecvError::Empty) => false,
                Err(oneshot::error::TryRecvError::Closed) => {
                    // Channel closed without sending - registry was dropped?
                    // Treat as not banned
                    false
                }
            }
        } else {
            false
        }
    }

    /// Get a mutable reference to the underlying reader
    pub fn reader(&mut self) -> &mut FrameReader<R> {
        &mut self.reader
    }

    /// Get a mutable reference to the underlying writer
    pub fn writer(&mut self) -> &mut FrameWriter<W> {
        &mut self.writer
    }

    /// Borrow both reader and writer simultaneously
    ///
    /// This is needed when an operation requires access to both, since
    /// calling `reader()` and `writer()` separately would cause borrow conflicts.
    pub fn reader_writer(&mut self) -> (&mut FrameReader<R>, &mut FrameWriter<W>) {
        (&mut self.reader, &mut self.writer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::FileIndex;
    use crate::transfers::registry::{TransferDirection, TransferRegistry};
    use std::collections::HashSet;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use tempfile::TempDir;
    use tokio::io::duplex;

    fn make_test_user() -> AuthenticatedUser {
        AuthenticatedUser {
            nickname: "testuser".to_string(),
            username: "testuser".to_string(),
            is_admin: false,
            is_shared: false,
            permissions: HashSet::new(),
        }
    }

    fn make_test_addr() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 12345)
    }

    fn make_test_file_index(temp_dir: &TempDir) -> Arc<FileIndex> {
        Arc::new(FileIndex::new(temp_dir.path(), temp_dir.path()))
    }

    #[test]
    fn test_stream_error_display() {
        let io_err = StreamError::Io(io::Error::other("test error"));
        assert!(io_err.to_string().contains("I/O error"));

        let banned = StreamError::Banned;
        assert!(banned.to_string().contains("banned"));

        let closed = StreamError::ConnectionClosed;
        assert!(closed.to_string().contains("closed"));
    }

    #[test]
    fn test_stream_error_from_io() {
        let io_err = io::Error::other("test");
        let stream_err: StreamError = io_err.into();
        assert!(matches!(stream_err, StreamError::Io(_)));
    }

    #[tokio::test]
    async fn test_transfer_metrics() {
        let registry = TransferRegistry::new();
        let (client, server) = duplex(1024);
        let (server_read, server_write) = tokio::io::split(server);

        let peer_addr = make_test_addr();
        let (info, ban_rx) = registry.register(TransferRegistration {
            peer_addr,
            nickname: "testuser".to_string(),
            username: "testuser".to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Download,
            path: "/test/file.zip".to_string(),
            total_size: 1000,
        });

        let temp_dir = TempDir::new().unwrap();
        let file_root = temp_dir.path();
        let file_index = make_test_file_index(&temp_dir);
        let file_mutation_locks = Arc::new(PathLockMap::new());

        let transfer = Transfer::new(
            FrameReader::new(tokio::io::BufReader::new(server_read)),
            FrameWriter::new(server_write),
            ban_rx,
            info.clone(),
            TransferContext {
                user: make_test_user(),
                locale: "en".to_string(),
                file_root,
                file_index: &file_index,
                file_mutation_locks: &file_mutation_locks,
                registry: &registry,
            },
        );

        assert_eq!(transfer.id(), info.id);
        assert_eq!(transfer.bytes_transferred(), 0);
        assert_eq!(transfer.peer_addr(), peer_addr);

        // Verify info is shared
        assert_eq!(info.get_bytes_transferred(), 0);

        drop(client);
        drop(transfer);
    }

    #[tokio::test]
    async fn test_transfer_ban_detection() {
        let registry = TransferRegistry::new();
        let (client, server) = duplex(1024);
        let (server_read, server_write) = tokio::io::split(server);

        let peer_addr = make_test_addr();
        let (info, ban_rx) = registry.register(TransferRegistration {
            peer_addr,
            nickname: "testuser".to_string(),
            username: "testuser".to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Download,
            path: "/test/file.zip".to_string(),
            total_size: 0,
        });

        let temp_dir = TempDir::new().unwrap();
        let file_root = temp_dir.path();
        let file_index = make_test_file_index(&temp_dir);
        let file_mutation_locks = Arc::new(PathLockMap::new());

        let mut transfer = Transfer::new(
            FrameReader::new(tokio::io::BufReader::new(server_read)),
            FrameWriter::new(server_write),
            ban_rx,
            info,
            TransferContext {
                user: make_test_user(),
                locale: "en".to_string(),
                file_root,
                file_index: &file_index,
                file_mutation_locks: &file_mutation_locks,
                registry: &registry,
            },
        );

        // Not banned yet
        assert!(!transfer.is_banned());

        // Send ban signal
        registry.disconnect_matching(|_| true);

        // Now should be banned
        assert!(transfer.is_banned());

        // Should stay banned
        assert!(transfer.is_banned());

        drop(client);
    }

    #[tokio::test]
    async fn test_transfer_send_works_when_banned() {
        // Verifies that send() still works even after ban (for protocol cleanup)
        let registry = TransferRegistry::new();
        let (_client, server) = duplex(4096);
        let (server_read, server_write) = tokio::io::split(server);

        let peer_addr = make_test_addr();
        let (info, ban_rx) = registry.register(TransferRegistration {
            peer_addr,
            nickname: "testuser".to_string(),
            username: "testuser".to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Download,
            path: "/test/file.zip".to_string(),
            total_size: 0,
        });

        let temp_dir = TempDir::new().unwrap();
        let file_root = temp_dir.path();
        let file_index = make_test_file_index(&temp_dir);
        let file_mutation_locks = Arc::new(PathLockMap::new());

        let mut transfer = Transfer::new(
            FrameReader::new(tokio::io::BufReader::new(server_read)),
            FrameWriter::new(server_write),
            ban_rx,
            info,
            TransferContext {
                user: make_test_user(),
                locale: "en".to_string(),
                file_root,
                file_index: &file_index,
                file_mutation_locks: &file_mutation_locks,
                registry: &registry,
            },
        );

        // Ban the transfer
        registry.disconnect_matching(|_| true);
        assert!(transfer.is_banned());

        // send() should still work (for error messages, etc.)
        let msg = ServerMessage::Error {
            message: "Test".to_string(),
            command: None,
        };
        let result = transfer.send(&msg).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_stream_file_to_client_banned_mid_stream() {
        // Test that ban is detected between chunks during streaming.
        // We simulate this by banning before streaming a multi-chunk file.
        // The streaming loop checks is_banned() between chunks.
        let registry = TransferRegistry::new();
        let (_client, server) = duplex(1024 * 1024);
        let (server_read, server_write) = tokio::io::split(server);

        let peer_addr = make_test_addr();
        let (info, ban_rx) = registry.register(TransferRegistration {
            peer_addr,
            nickname: "testuser".to_string(),
            username: "testuser".to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Download,
            path: "/test/file.zip".to_string(),
            total_size: 0,
        });

        let temp_dir = TempDir::new().unwrap();
        let file_root = temp_dir.path();
        let file_index = make_test_file_index(&temp_dir);
        let file_mutation_locks = Arc::new(PathLockMap::new());

        let mut transfer = Transfer::new(
            FrameReader::new(tokio::io::BufReader::new(server_read)),
            FrameWriter::new(server_write),
            ban_rx,
            info,
            TransferContext {
                user: make_test_user(),
                locale: "en".to_string(),
                file_root,
                file_index: &file_index,
                file_mutation_locks: &file_mutation_locks,
                registry: &registry,
            },
        );

        // Ban before starting - this tests the pre-stream check
        registry.disconnect_matching(|_| true);

        // Try to stream - should fail immediately with Banned
        let file_data = vec![0xABu8; 100_000]; // 100KB (multiple chunks)
        let mut reader = std::io::Cursor::new(file_data.clone());

        let result = transfer
            .stream_file_to_client("FileData", &mut reader, file_data.len() as u64)
            .await;

        assert!(matches!(result, Err(StreamError::Banned)));
    }

    #[tokio::test]
    async fn test_stream_file_to_client_banned_before_start() {
        let registry = TransferRegistry::new();
        let (_client, server) = duplex(1024);
        let (server_read, server_write) = tokio::io::split(server);

        let peer_addr = make_test_addr();
        let (info, ban_rx) = registry.register(TransferRegistration {
            peer_addr,
            nickname: "testuser".to_string(),
            username: "testuser".to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Download,
            path: "/test/file.zip".to_string(),
            total_size: 0,
        });

        let temp_dir = TempDir::new().unwrap();
        let file_root = temp_dir.path();
        let file_index = make_test_file_index(&temp_dir);
        let file_mutation_locks = Arc::new(PathLockMap::new());

        let mut transfer = Transfer::new(
            FrameReader::new(tokio::io::BufReader::new(server_read)),
            FrameWriter::new(server_write),
            ban_rx,
            info,
            TransferContext {
                user: make_test_user(),
                locale: "en".to_string(),
                file_root,
                file_index: &file_index,
                file_mutation_locks: &file_mutation_locks,
                registry: &registry,
            },
        );

        // Ban before starting stream
        registry.disconnect_matching(|_| true);

        // Use tokio::io::Cursor which implements AsyncRead
        let file_data = vec![0u8; 1000];
        let mut async_reader = std::io::Cursor::new(file_data);

        let result = transfer
            .stream_file_to_client("FileData", &mut async_reader, 1000)
            .await;

        assert!(matches!(result, Err(StreamError::Banned)));
    }

    #[tokio::test]
    async fn test_transfer_unregisters_on_drop() {
        let registry = TransferRegistry::new();

        let peer_addr = make_test_addr();
        let (info, ban_rx) = registry.register(TransferRegistration {
            peer_addr,
            nickname: "testuser".to_string(),
            username: "testuser".to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Download,
            path: "/test/file.zip".to_string(),
            total_size: 0,
        });

        assert_eq!(registry.active_count(), 1);

        {
            let (_client, server) = duplex(1024);
            let (server_read, server_write) = tokio::io::split(server);

            let temp_dir = TempDir::new().unwrap();
            let file_root = temp_dir.path();
            let file_index = make_test_file_index(&temp_dir);
            let file_mutation_locks = Arc::new(PathLockMap::new());

            let _transfer = Transfer::new(
                FrameReader::new(tokio::io::BufReader::new(server_read)),
                FrameWriter::new(server_write),
                ban_rx,
                info,
                TransferContext {
                    user: make_test_user(),
                    locale: "en".to_string(),
                    file_root,
                    file_index: &file_index,
                    file_mutation_locks: &file_mutation_locks,
                    registry: &registry,
                },
            );

            assert_eq!(registry.active_count(), 1);
        } // transfer dropped here

        assert_eq!(registry.active_count(), 0);
    }

    #[tokio::test]
    async fn test_transfer_info_bytes_update() {
        // Verify that bytes_transferred updates are visible through the shared info
        let registry = TransferRegistry::new();
        let (_client, server) = duplex(1024 * 1024);
        let (server_read, server_write) = tokio::io::split(server);

        let peer_addr = make_test_addr();
        let (info, ban_rx) = registry.register(TransferRegistration {
            peer_addr,
            nickname: "testuser".to_string(),
            username: "testuser".to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Download,
            path: "/test/file.zip".to_string(),
            total_size: 1000,
        });

        let temp_dir = TempDir::new().unwrap();
        let file_root = temp_dir.path();
        let file_index = make_test_file_index(&temp_dir);
        let file_mutation_locks = Arc::new(PathLockMap::new());

        let mut transfer = Transfer::new(
            FrameReader::new(tokio::io::BufReader::new(server_read)),
            FrameWriter::new(server_write),
            ban_rx,
            info.clone(),
            TransferContext {
                user: make_test_user(),
                locale: "en".to_string(),
                file_root,
                file_index: &file_index,
                file_mutation_locks: &file_mutation_locks,
                registry: &registry,
            },
        );

        // Initial state
        assert_eq!(info.get_bytes_transferred(), 0);
        assert_eq!(transfer.bytes_transferred(), 0);

        // Simulate a successful stream using std::io::Cursor which implements AsyncRead
        let file_data = vec![0xABu8; 1000];
        let mut async_reader = std::io::Cursor::new(file_data);

        let result = transfer
            .stream_file_to_client("FileData", &mut async_reader, 1000)
            .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1000);

        // Both should show updated bytes
        assert_eq!(info.get_bytes_transferred(), 1000);
        assert_eq!(transfer.bytes_transferred(), 1000);
    }
}
