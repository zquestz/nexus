//! Transfer connection wrapper. Ban signals are checked between streaming
//! chunks so a mid-transfer ban stops file data immediately.

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

/// Chunk size for streaming file data; ban is checked between chunks.
const CHUNK_SIZE: usize = 64 * 1024;
use super::types::AuthenticatedUser;

#[derive(Debug)]
pub enum StreamError {
    Io(io::Error),
    /// Connection terminated due to IP ban
    Banned,
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

/// Per-transfer context bundled to keep [`Transfer::new`]'s signature narrow.
pub struct TransferContext<'a> {
    pub user: AuthenticatedUser,
    pub locale: String,
    pub file_root: &'a Path,
    pub file_index: &'a Arc<FileIndex>,
    pub file_mutation_locks: &'a Arc<PathLockMap>,
    pub registry: &'a TransferRegistry,
}

/// A file transfer connection with integrated ban handling. Streaming methods
/// check the ban receiver between chunks; the transfer unregisters from the
/// registry on drop. Its `info` is shared (Arc) with the registry, which the
/// connection monitor snapshots.
pub struct Transfer<'a, R, W> {
    reader: FrameReader<R>,
    writer: FrameWriter<W>,

    // Option so we can take it once the ban signal is received
    ban_rx: Option<oneshot::Receiver<()>>,
    banned: bool,

    info: Arc<ActiveTransfer>,

    user: AuthenticatedUser,
    locale: String,
    file_root: &'a Path,
    file_index: &'a Arc<FileIndex>,
    file_mutation_locks: &'a Arc<PathLockMap>,

    // Must be last so it drops after the other fields
    _guard: TransferRegistryGuard<'a>,
}

impl<'a, R, W> Transfer<'a, R, W>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    /// Create a transfer; it unregisters from the registry on drop.
    pub fn new(
        reader: FrameReader<R>,
        writer: FrameWriter<W>,
        ban_rx: oneshot::Receiver<()>,
        info: Arc<ActiveTransfer>,
        ctx: TransferContext<'a>,
    ) -> Self {
        // Read the id before `info` is moved into the struct.
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

    pub fn id(&self) -> TransferId {
        self.info.id
    }

    pub fn elapsed(&self) -> std::time::Duration {
        self.info.elapsed()
    }

    pub fn bytes_transferred(&self) -> u64 {
        self.info.get_bytes_transferred()
    }

    /// Set the total size (downloads learn it after path resolution).
    pub fn set_total_size(&self, size: u64) {
        self.info.set_total_size(size);
    }

    pub fn user(&self) -> &AuthenticatedUser {
        &self.user
    }

    pub fn locale(&self) -> &str {
        &self.locale
    }

    pub fn peer_addr(&self) -> SocketAddr {
        self.info.peer_addr
    }

    pub fn file_root(&self) -> &Path {
        self.file_root
    }

    pub fn file_index(&self) -> &Arc<FileIndex> {
        self.file_index
    }

    pub fn file_mutation_locks(&self) -> &Arc<PathLockMap> {
        self.file_mutation_locks
    }

    pub async fn send(&mut self, msg: &ServerMessage) -> Result<(), StreamError> {
        self.send_with_id(msg, MessageId::new()).await
    }

    pub async fn send_with_id(
        &mut self,
        msg: &ServerMessage,
        msg_id: MessageId,
    ) -> Result<(), StreamError> {
        send_server_message_with_id(&mut self.writer, msg, msg_id)
            .await
            .map_err(StreamError::Io)
    }

    /// Stream a file to the client, checking for a ban between chunks.
    ///
    /// On ban, returns `Err(StreamError::Banned)`; the caller should close the
    /// connection immediately — the client gets the ban reason on the BBS port.
    pub async fn stream_file_to_client<S>(
        &mut self,
        message_type: &str,
        reader: &mut S,
        length: u64,
    ) -> Result<u64, StreamError>
    where
        S: AsyncRead + Unpin,
    {
        if self.is_banned() {
            return Err(StreamError::Banned);
        }

        // Frame header: NX|type_len|type|msg_id|payload_len|
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

        let mut buffer = vec![0u8; CHUNK_SIZE];
        let mut remaining = length;
        let mut total_written: u64 = 0;

        while remaining > 0 {
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

            self.info.add_bytes_transferred(bytes_read as u64);
        }

        // Frame terminator
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

    /// Receive file data from the client, checking for a ban between chunks.
    ///
    /// On ban, stops writing to `dest` but still drains the rest of the frame so
    /// the connection stays framed before the caller closes it. `progress_timeout`
    /// bounds the wait between chunks.
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

        if self.is_banned() {
            return Err(StreamError::Banned);
        }

        let mut buffer = vec![0u8; CHUNK_SIZE];
        let mut remaining = header.payload_length;
        let mut total_written: u64 = 0;

        while remaining > 0 {
            if self.is_banned() {
                // Stop writing; the drain loop below consumes the rest of the frame.
                break;
            }

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

            dest.write_all(&buffer[..bytes_read])
                .await
                .map_err(StreamError::Io)?;

            remaining -= bytes_read as u64;
            total_written += bytes_read as u64;

            self.info.add_bytes_transferred(bytes_read as u64);
        }

        dest.flush().await.map_err(StreamError::Io)?;

        // Drain any payload left after a mid-frame ban (not written to file).
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

        // Frame terminator
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

    /// Check (non-blocking) whether a ban signal has arrived.
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
                    // Sender dropped without signaling (e.g. registry gone) — not a ban.
                    false
                }
            }
        } else {
            false
        }
    }

    pub fn reader(&mut self) -> &mut FrameReader<R> {
        &mut self.reader
    }

    pub fn writer(&mut self) -> &mut FrameWriter<W> {
        &mut self.writer
    }

    /// Borrow reader and writer together (separate calls would conflict).
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

        assert!(!transfer.is_banned());

        registry.disconnect_matching(|_| true);

        // Banned and stays banned across repeated checks.
        assert!(transfer.is_banned());
        assert!(transfer.is_banned());

        drop(client);
    }

    #[tokio::test]
    async fn test_transfer_send_works_when_banned() {
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

        registry.disconnect_matching(|_| true);
        assert!(transfer.is_banned());

        // send() must still work after a ban (error messages, etc.)
        let msg = ServerMessage::Error {
            message: "Test".to_string(),
            command: None,
        };
        let result = transfer.send(&msg).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_stream_file_to_client_banned_mid_stream() {
        // Ban before a multi-chunk stream; the loop's between-chunk check trips.
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

        registry.disconnect_matching(|_| true);

        let file_data = vec![0xABu8; 100_000]; // multiple chunks
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

        registry.disconnect_matching(|_| true);

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

        assert_eq!(info.get_bytes_transferred(), 0);
        assert_eq!(transfer.bytes_transferred(), 0);

        let file_data = vec![0xABu8; 1000];
        let mut async_reader = std::io::Cursor::new(file_data);

        let result = transfer
            .stream_file_to_client("FileData", &mut async_reader, 1000)
            .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1000);

        // Update is visible through both the transfer and the shared info.
        assert_eq!(info.get_bytes_transferred(), 1000);
        assert_eq!(transfer.bytes_transferred(), 1000);
    }
}
