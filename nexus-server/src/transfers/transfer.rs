//! Transfer connection wrapper. Ban signals are checked between streaming
//! chunks so a mid-transfer ban stops file data immediately.

use std::io;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::oneshot;

use nexus_common::framing::{FrameReader, FrameWriter, MessageId};
use nexus_common::io::{send_server_message_with_id, server_message_to_frame_bytes};
use nexus_common::protocol::ServerMessage;

use crate::constants::{
    ERR_TRANSFER_EGRESS_ENQUEUE_FAILED, ERR_TRANSFER_EGRESS_TASK_FAILED,
    ERR_TRANSFER_MISSING_FRAME_TERMINATOR, ERR_TRANSFER_READ_TIMEOUT,
};
use crate::egress;
use crate::egress::EgressEnqueueError;
use crate::egress::task::{EgressHandle, EgressTaskError};
use crate::files::{FileActivityMap, FileIndex};
use crate::scheduler::ConnectionId;

#[cfg(test)]
use super::registry::TransferRegistration;
use super::registry::{ActiveTransfer, TransferId, TransferRegistry, TransferRegistryGuard};

/// Chunk size for streaming file data; ban is checked between chunks.
const CHUNK_SIZE: usize = 64 * 1024;
use super::types::AuthenticatedUser;

#[derive(Debug)]
pub enum StreamError {
    Io(io::Error),
    FrameStarted(io::Error),
    /// Connection terminated due to IP ban
    Banned,
    ConnectionClosed,
}

impl std::fmt::Display for StreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::FrameStarted(e) => write!(f, "I/O error after frame started: {e}"),
            Self::Banned => write!(f, "Connection terminated: IP banned"),
            Self::ConnectionClosed => write!(f, "Connection closed"),
        }
    }
}

impl std::error::Error for StreamError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) | Self::FrameStarted(e) => Some(e),
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
    pub file_activity: &'a Arc<FileActivityMap>,
    pub user_area_root: Option<PathBuf>,
    pub registry: &'a TransferRegistry,
    pub egress: Option<TransferEgress>,
}

pub struct TransferEgress {
    handle: EgressHandle,
    connection_id: ConnectionId,
    dispatch_rx: egress::EgressDispatchRx,
}

impl TransferEgress {
    pub fn new(
        handle: EgressHandle,
        connection_id: ConnectionId,
        dispatch_rx: egress::EgressDispatchRx,
    ) -> Self {
        Self {
            handle,
            connection_id,
            dispatch_rx,
        }
    }
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
    file_activity: &'a Arc<FileActivityMap>,
    user_area_root: Option<PathBuf>,
    egress: Option<TransferEgress>,

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
            file_activity: ctx.file_activity,
            user_area_root: ctx.user_area_root,
            egress: ctx.egress,
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

    pub fn file_activity(&self) -> &Arc<FileActivityMap> {
        self.file_activity
    }

    pub fn user_area_root(&self) -> Option<&Path> {
        self.user_area_root.as_deref()
    }

    pub async fn send(&mut self, msg: &ServerMessage) -> Result<(), StreamError> {
        self.send_with_id(msg, MessageId::new()).await
    }

    pub async fn send_with_id(
        &mut self,
        msg: &ServerMessage,
        msg_id: MessageId,
    ) -> Result<(), StreamError> {
        if self.egress.is_some() {
            let frame = server_message_to_frame_bytes(msg, msg_id).map_err(StreamError::Io)?;
            return self.send_egress_frame(frame).await;
        }

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

        if self.egress.is_some() {
            self.stream_file_to_client_egress(message_type, reader, length)
                .await
        } else {
            self.stream_file_to_client_direct(message_type, reader, length)
                .await
        }
    }

    async fn stream_file_to_client_direct<S>(
        &mut self,
        message_type: &str,
        reader: &mut S,
        length: u64,
    ) -> Result<u64, StreamError>
    where
        S: AsyncRead + Unpin,
    {
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
                .map_err(StreamError::FrameStarted)?;

            if bytes_read == 0 {
                return Err(StreamError::FrameStarted(std::io::Error::other(format!(
                    "Reader ended early: expected {} more bytes",
                    remaining
                ))));
            }

            self.writer
                .get_mut()
                .write_all(&buffer[..bytes_read])
                .await
                .map_err(StreamError::FrameStarted)?;

            remaining -= bytes_read as u64;
            total_written += bytes_read as u64;

            self.info.add_bytes_transferred(bytes_read as u64);
        }

        // Frame terminator
        self.writer
            .get_mut()
            .write_all(b"\n")
            .await
            .map_err(StreamError::FrameStarted)?;

        self.writer
            .get_mut()
            .flush()
            .await
            .map_err(StreamError::FrameStarted)?;

        Ok(total_written)
    }

    async fn stream_file_to_client_egress<S>(
        &mut self,
        message_type: &str,
        reader: &mut S,
        length: u64,
    ) -> Result<u64, StreamError>
    where
        S: AsyncRead + Unpin,
    {
        let msg_id = MessageId::new();
        let header: Arc<[u8]> = Arc::from(
            format!(
                "NX|{}|{}|{}|{}|",
                message_type.len(),
                message_type,
                msg_id,
                length
            )
            .into_bytes(),
        );

        let mut buffer = vec![0u8; CHUNK_SIZE];
        let mut remaining = length;
        let mut total_written: u64 = 0;
        let first_block = if remaining > 0 {
            let to_read = (remaining as usize).min(CHUNK_SIZE);
            let bytes_read = reader
                .read(&mut buffer[..to_read])
                .await
                .map_err(StreamError::Io)?;
            if bytes_read == 0 {
                return Err(StreamError::Io(std::io::Error::other(format!(
                    "Reader ended early: expected {remaining} more bytes"
                ))));
            }
            remaining -= bytes_read as u64;
            let block: Arc<[u8]> = Arc::from(buffer[..bytes_read].to_vec());
            Some((block, bytes_read))
        } else {
            None
        };

        let header_chunks = self.begin_egress_stream_frame(header).await?;
        let result = self
            .stream_file_to_client_egress_after_begin(
                reader,
                remaining,
                first_block,
                header_chunks,
                &mut total_written,
            )
            .await;

        if result.is_err() {
            self.notify_egress_write_failed().await;
        }

        result.map(|()| total_written)
    }

    async fn stream_file_to_client_egress_after_begin<S>(
        &mut self,
        reader: &mut S,
        mut remaining: u64,
        first_block: Option<(Arc<[u8]>, usize)>,
        header_chunks: usize,
        total_written: &mut u64,
    ) -> Result<(), StreamError>
    where
        S: AsyncRead + Unpin,
    {
        let mut chunks_to_drain = header_chunks;
        if let Some((block, bytes_read)) = first_block {
            chunks_to_drain += self.stage_egress_stream_chunk(block).await?;
            self.drain_egress_chunks(chunks_to_drain).await?;
            *total_written += bytes_read as u64;
            self.info.add_bytes_transferred(bytes_read as u64);
        } else {
            self.drain_egress_chunks(chunks_to_drain).await?;
        }

        let mut buffer = vec![0u8; CHUNK_SIZE];
        while remaining > 0 {
            if self.is_banned() {
                return Err(StreamError::Banned);
            }

            let to_read = (remaining as usize).min(CHUNK_SIZE);
            let bytes_read = reader
                .read(&mut buffer[..to_read])
                .await
                .map_err(StreamError::FrameStarted)?;
            if bytes_read == 0 {
                return Err(StreamError::FrameStarted(std::io::Error::other(format!(
                    "Reader ended early: expected {remaining} more bytes"
                ))));
            }
            remaining -= bytes_read as u64;

            let block: Arc<[u8]> = Arc::from(buffer[..bytes_read].to_vec());
            let chunks = self.stage_egress_stream_chunk(block).await?;
            self.drain_egress_chunks(chunks).await?;
            *total_written += bytes_read as u64;
            self.info.add_bytes_transferred(bytes_read as u64);
        }

        let terminator_chunks = self.finish_egress_stream_frame().await?;
        self.drain_egress_chunks(terminator_chunks).await
    }

    async fn begin_egress_stream_frame(&self, header: Arc<[u8]>) -> Result<usize, StreamError> {
        let Some(egress) = self.egress.as_ref() else {
            return Err(StreamError::Io(io::Error::other(
                "egress is not registered",
            )));
        };

        match egress
            .handle
            .begin_stream_frame(egress.connection_id, header)
            .await
        {
            Ok(Ok(chunks)) => Ok(chunks),
            Ok(Err(err)) => Err(StreamError::Io(egress_enqueue_io_error(err))),
            Err(err) => Err(StreamError::Io(egress_task_io_error(err))),
        }
    }

    async fn stage_egress_stream_chunk(&self, chunk: Arc<[u8]>) -> Result<usize, StreamError> {
        let Some(egress) = self.egress.as_ref() else {
            return Err(StreamError::FrameStarted(io::Error::other(
                "egress is not registered",
            )));
        };

        match egress
            .handle
            .stage_stream_chunk(egress.connection_id, chunk)
            .await
        {
            Ok(Ok(chunks)) => Ok(chunks),
            Ok(Err(err)) => Err(StreamError::FrameStarted(egress_enqueue_io_error(err))),
            Err(err) => Err(StreamError::FrameStarted(egress_task_io_error(err))),
        }
    }

    async fn finish_egress_stream_frame(&self) -> Result<usize, StreamError> {
        let Some(egress) = self.egress.as_ref() else {
            return Err(StreamError::FrameStarted(io::Error::other(
                "egress is not registered",
            )));
        };

        match egress
            .handle
            .finish_stream_frame(egress.connection_id)
            .await
        {
            Ok(Ok(chunks)) => Ok(chunks),
            Ok(Err(err)) => Err(StreamError::FrameStarted(egress_enqueue_io_error(err))),
            Err(err) => Err(StreamError::FrameStarted(egress_task_io_error(err))),
        }
    }

    async fn drain_egress_chunks(&mut self, mut chunks: usize) -> Result<(), StreamError> {
        while chunks > 0 {
            let Some(egress) = self.egress.as_mut() else {
                return Err(StreamError::FrameStarted(io::Error::other(
                    "egress is not registered",
                )));
            };

            let Some(dispatch) = egress.dispatch_rx.recv().await else {
                return Err(StreamError::FrameStarted(io::Error::other(
                    "egress dispatch channel closed",
                )));
            };

            if dispatch.connection_id != egress.connection_id {
                let _ = egress.handle.write_failed(dispatch.connection_id).await;
                continue;
            }

            if let Err(err) = self
                .writer
                .get_mut()
                .write_all(dispatch.chunk.as_bytes())
                .await
            {
                let _ = egress.handle.write_failed(egress.connection_id).await;
                return Err(StreamError::FrameStarted(err));
            }
            if let Err(err) = self.writer.get_mut().flush().await {
                let _ = egress.handle.write_failed(egress.connection_id).await;
                return Err(StreamError::FrameStarted(err));
            }

            if let Err(err) = egress.handle.ack(egress.connection_id).await {
                return Err(StreamError::FrameStarted(egress_task_io_error(err)));
            }

            chunks -= 1;
        }

        Ok(())
    }

    async fn notify_egress_write_failed(&self) {
        if let Some(egress) = self.egress.as_ref() {
            let _ = egress.handle.write_failed(egress.connection_id).await;
        }
    }

    async fn send_egress_frame(&mut self, frame: Arc<[u8]>) -> Result<(), StreamError> {
        let Some(egress) = self.egress.as_mut() else {
            self.writer
                .get_mut()
                .write_all(&frame)
                .await
                .map_err(StreamError::Io)?;
            self.writer
                .get_mut()
                .flush()
                .await
                .map_err(StreamError::Io)?;
            return Ok(());
        };

        match egress
            .handle
            .stage_frame(egress.connection_id, Arc::clone(&frame))
            .await
        {
            Ok(Ok(_)) => {}
            Ok(Err(err)) => return Err(StreamError::Io(egress_enqueue_io_error(err))),
            Err(err) => return Err(StreamError::Io(egress_task_io_error(err))),
        }

        loop {
            let Some(dispatch) = egress.dispatch_rx.recv().await else {
                return Err(StreamError::ConnectionClosed);
            };

            if dispatch.connection_id != egress.connection_id {
                let _ = egress.handle.write_failed(dispatch.connection_id).await;
                continue;
            }

            let is_final_frame_chunk = dispatch.chunk.is_final_frame_chunk();
            if let Err(err) = self
                .writer
                .get_mut()
                .write_all(dispatch.chunk.as_bytes())
                .await
            {
                let _ = egress.handle.write_failed(egress.connection_id).await;
                return Err(StreamError::Io(err));
            }
            if let Err(err) = self.writer.get_mut().flush().await {
                let _ = egress.handle.write_failed(egress.connection_id).await;
                return Err(StreamError::Io(err));
            }

            if let Err(err) = egress.handle.ack(egress.connection_id).await {
                return Err(StreamError::Io(egress_task_io_error(err)));
            }

            if is_final_frame_chunk {
                return Ok(());
            }
        }
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
                Err(_) => {
                    return Err(StreamError::Io(std::io::Error::other(
                        ERR_TRANSFER_READ_TIMEOUT,
                    )));
                }
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
                Err(_) => {
                    return Err(StreamError::Io(std::io::Error::other(
                        ERR_TRANSFER_READ_TIMEOUT,
                    )));
                }
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
            Err(_) => {
                return Err(StreamError::Io(std::io::Error::other(
                    ERR_TRANSFER_READ_TIMEOUT,
                )));
            }
        }

        if terminator[0] != b'\n' {
            return Err(StreamError::Io(std::io::Error::other(
                ERR_TRANSFER_MISSING_FRAME_TERMINATOR,
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

fn egress_enqueue_io_error(err: EgressEnqueueError) -> io::Error {
    io::Error::other(format!("{ERR_TRANSFER_EGRESS_ENQUEUE_FAILED}{err:?}"))
}

fn egress_task_io_error(err: EgressTaskError) -> io::Error {
    io::Error::other(format!("{ERR_TRANSFER_EGRESS_TASK_FAILED}{err:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::egress::task::EgressTask;
    use crate::egress::{self, EgressManager};
    use crate::files::{FileActivityMap, FileIndex};
    use crate::scheduler::{ConnectionClass, ConnectionId};
    use crate::transfers::registry::{TransferDirection, TransferRegistry};
    use nexus_common::io::read_transfer_server_message as read_server_message;
    use std::collections::HashSet;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::num::NonZeroUsize;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use tempfile::TempDir;
    use tokio::io::{AsyncRead, ReadBuf, duplex};
    use tokio::sync::mpsc;

    fn make_test_user() -> AuthenticatedUser {
        AuthenticatedUser {
            user_id: 1,
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

    async fn registered_transfer_egress(
        chunk_size: usize,
    ) -> (
        TransferEgress,
        EgressHandle,
        tokio::task::JoinHandle<()>,
        ConnectionId,
    ) {
        registered_transfer_egress_with_rate(chunk_size, 0).await
    }

    async fn registered_transfer_egress_with_rate(
        chunk_size: usize,
        bytes_per_second: u64,
    ) -> (
        TransferEgress,
        EgressHandle,
        tokio::task::JoinHandle<()>,
        ConnectionId,
    ) {
        let manager = EgressManager::new(NonZeroUsize::new(chunk_size).unwrap());
        let (egress, task) = EgressTask::channel_with_rate(manager, bytes_per_second);
        let task = tokio::spawn(task.run());
        let connection_id = ConnectionId::new(88_001);
        let (dispatch_tx, dispatch_rx) = mpsc::channel(egress::EGRESS_DISPATCH_QUEUE_CAPACITY);
        assert!(
            egress
                .register_anon(connection_id, ConnectionClass::Bulk, dispatch_tx)
                .await
                .unwrap()
        );

        (
            TransferEgress::new(egress.clone(), connection_id, dispatch_rx),
            egress,
            task,
            connection_id,
        )
    }

    async fn stop_egress_task(
        egress: EgressHandle,
        task: tokio::task::JoinHandle<()>,
        connection_id: ConnectionId,
    ) {
        let _ = egress.unregister(connection_id).await;
        drop(egress);
        task.await.unwrap();
    }

    struct ErrorAfterDataReader {
        data: Vec<u8>,
        offset: usize,
        failed: bool,
    }

    impl ErrorAfterDataReader {
        fn new(data: Vec<u8>) -> Self {
            Self {
                data,
                offset: 0,
                failed: false,
            }
        }
    }

    impl AsyncRead for ErrorAfterDataReader {
        fn poll_read(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            if self.offset < self.data.len() {
                let remaining = self.data.len() - self.offset;
                let len = remaining.min(buf.remaining());
                buf.put_slice(&self.data[self.offset..self.offset + len]);
                self.offset += len;
                return Poll::Ready(Ok(()));
            }

            if !self.failed {
                self.failed = true;
                return Poll::Ready(Err(io::Error::other("injected read failure")));
            }

            Poll::Ready(Ok(()))
        }
    }

    struct BanAfterFirstRead<'a> {
        data: Vec<u8>,
        offset: usize,
        registry: &'a TransferRegistry,
        signaled: bool,
    }

    impl<'a> BanAfterFirstRead<'a> {
        fn new(data: Vec<u8>, registry: &'a TransferRegistry) -> Self {
            Self {
                data,
                offset: 0,
                registry,
                signaled: false,
            }
        }
    }

    impl AsyncRead for BanAfterFirstRead<'_> {
        fn poll_read(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            if self.offset < self.data.len() {
                let remaining = self.data.len() - self.offset;
                let len = remaining.min(buf.remaining());
                buf.put_slice(&self.data[self.offset..self.offset + len]);
                self.offset += len;

                if !self.signaled {
                    self.signaled = true;
                    assert_eq!(self.registry.disconnect_matching(|_| true), 1);
                }

                return Poll::Ready(Ok(()));
            }

            Poll::Ready(Ok(()))
        }
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
            user_id: 1,
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
        let file_activity = Arc::new(FileActivityMap::new());

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
                file_activity: &file_activity,
                user_area_root: None,
                registry: &registry,
                egress: None,
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
            user_id: 1,
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
        let file_activity = Arc::new(FileActivityMap::new());

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
                file_activity: &file_activity,
                user_area_root: None,
                registry: &registry,
                egress: None,
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
            user_id: 1,
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
        let file_activity = Arc::new(FileActivityMap::new());

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
                file_activity: &file_activity,
                user_area_root: None,
                registry: &registry,
                egress: None,
            },
        );

        registry.disconnect_matching(|_| true);
        assert!(transfer.is_banned());

        // send() must still work after a ban (error messages, etc.)
        let msg = ServerMessage::Error {
            message: "Test".to_string(),
            command: None,
            disconnect: false,
        };
        let result = transfer.send(&msg).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn transfer_send_routes_complete_frame_through_egress() {
        let registry = TransferRegistry::new();
        let (client, server) = duplex(4096);
        let (client_read, _client_write) = tokio::io::split(client);
        let (server_read, server_write) = tokio::io::split(server);
        let mut client_reader = FrameReader::new(tokio::io::BufReader::new(client_read));

        let peer_addr = make_test_addr();
        let (info, ban_rx) = registry.register(TransferRegistration {
            user_id: 1,
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
        let file_activity = Arc::new(FileActivityMap::new());
        let (transfer_egress, egress, egress_task, connection_id) =
            registered_transfer_egress(8).await;

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
                file_activity: &file_activity,
                user_area_root: None,
                registry: &registry,
                egress: Some(transfer_egress),
            },
        );

        let message = ServerMessage::TransferComplete {
            success: true,
            error: None,
            error_kind: None,
        };
        transfer
            .send_with_id(&message, MessageId::new())
            .await
            .unwrap();

        let received = read_server_message(&mut client_reader)
            .await
            .unwrap()
            .unwrap();
        match received.message {
            ServerMessage::TransferComplete {
                success,
                error,
                error_kind,
            } => {
                assert!(success);
                assert!(error.is_none());
                assert!(error_kind.is_none());
            }
            other => panic!("expected TransferComplete, got {other:?}"),
        }

        drop(transfer);
        stop_egress_task(egress, egress_task, connection_id).await;
    }

    #[tokio::test]
    async fn egress_registered_file_data_keeps_single_streaming_frame() {
        let registry = TransferRegistry::new();
        let (client, server) = duplex(256 * 1024);
        let (client_read, _client_write) = tokio::io::split(client);
        let (server_read, server_write) = tokio::io::split(server);
        let mut client_reader = FrameReader::new(tokio::io::BufReader::new(client_read));

        let peer_addr = make_test_addr();
        let (info, ban_rx) = registry.register(TransferRegistration {
            user_id: 1,
            peer_addr,
            nickname: "testuser".to_string(),
            username: "testuser".to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Download,
            path: "/test/file.zip".to_string(),
            total_size: CHUNK_SIZE as u64 + 5,
        });

        let temp_dir = TempDir::new().unwrap();
        let file_root = temp_dir.path();
        let file_index = make_test_file_index(&temp_dir);
        let file_activity = Arc::new(FileActivityMap::new());
        let (transfer_egress, egress, egress_task, connection_id) =
            registered_transfer_egress(1024).await;

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
                file_activity: &file_activity,
                user_area_root: None,
                registry: &registry,
                egress: Some(transfer_egress),
            },
        );

        let mut file_data = vec![0xAB; CHUNK_SIZE];
        file_data.extend_from_slice(b"final");
        let mut reader = std::io::Cursor::new(file_data.clone());

        let written = transfer
            .stream_file_to_client("FileData", &mut reader, file_data.len() as u64)
            .await
            .unwrap();
        assert_eq!(written, file_data.len() as u64);
        assert_eq!(info.get_bytes_transferred(), file_data.len() as u64);

        let frame = client_reader.read_frame_header().await.unwrap().unwrap();
        assert_eq!(frame.message_type, "FileData");
        assert_eq!(frame.payload_length, file_data.len() as u64);
        let payload = client_reader.read_payload_into_vec(&frame).await.unwrap();
        assert_eq!(payload, file_data);

        transfer
            .send(&ServerMessage::FileHash {
                blake3: "abc123".to_string(),
            })
            .await
            .unwrap();
        let file_hash = read_server_message(&mut client_reader)
            .await
            .unwrap()
            .unwrap()
            .message;
        assert!(matches!(file_hash, ServerMessage::FileHash { .. }));

        drop(transfer);
        stop_egress_task(egress, egress_task, connection_id).await;
    }

    #[tokio::test]
    async fn egress_registered_small_file_data_uses_single_streaming_frame() {
        let registry = TransferRegistry::new();
        let (client, server) = duplex(4096);
        let (client_read, _client_write) = tokio::io::split(client);
        let (server_read, server_write) = tokio::io::split(server);
        let mut client_reader = FrameReader::new(tokio::io::BufReader::new(client_read));

        let peer_addr = make_test_addr();
        let (info, ban_rx) = registry.register(TransferRegistration {
            user_id: 1,
            peer_addr,
            nickname: "testuser".to_string(),
            username: "testuser".to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Download,
            path: "/test/small.txt".to_string(),
            total_size: 9,
        });

        let temp_dir = TempDir::new().unwrap();
        let file_root = temp_dir.path();
        let file_index = make_test_file_index(&temp_dir);
        let file_activity = Arc::new(FileActivityMap::new());
        let (transfer_egress, egress, egress_task, connection_id) =
            registered_transfer_egress(1024).await;

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
                file_activity: &file_activity,
                user_area_root: None,
                registry: &registry,
                egress: Some(transfer_egress),
            },
        );

        let file_data = b"tiny data".to_vec();
        let mut reader = std::io::Cursor::new(file_data.clone());

        let written = transfer
            .stream_file_to_client("FileData", &mut reader, file_data.len() as u64)
            .await
            .unwrap();
        assert_eq!(written, file_data.len() as u64);
        assert_eq!(info.get_bytes_transferred(), file_data.len() as u64);

        let frame = client_reader.read_frame_header().await.unwrap().unwrap();
        assert_eq!(frame.message_type, "FileData");
        assert_eq!(frame.payload_length, file_data.len() as u64);
        let payload = client_reader.read_payload_into_vec(&frame).await.unwrap();
        assert_eq!(payload, file_data);

        transfer
            .send(&ServerMessage::FileHash {
                blake3: "small-hash".to_string(),
            })
            .await
            .unwrap();
        let file_hash = read_server_message(&mut client_reader)
            .await
            .unwrap()
            .unwrap()
            .message;
        assert!(matches!(file_hash, ServerMessage::FileHash { .. }));

        drop(transfer);
        stop_egress_task(egress, egress_task, connection_id).await;
    }

    #[tokio::test]
    async fn egress_file_data_is_rate_limited() {
        let registry = TransferRegistry::new();
        let (client, server) = duplex(64 * 1024);
        let (client_read, _client_write) = tokio::io::split(client);
        let (server_read, server_write) = tokio::io::split(server);
        let mut client_reader = FrameReader::new(tokio::io::BufReader::new(client_read));

        let peer_addr = make_test_addr();
        let (info, ban_rx) = registry.register(TransferRegistration {
            user_id: 1,
            peer_addr,
            nickname: "testuser".to_string(),
            username: "testuser".to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Download,
            path: "/test/rate.bin".to_string(),
            total_size: 2048,
        });

        let temp_dir = TempDir::new().unwrap();
        let file_root = temp_dir.path();
        let file_index = make_test_file_index(&temp_dir);
        let file_activity = Arc::new(FileActivityMap::new());
        let (transfer_egress, egress, egress_task, connection_id) =
            registered_transfer_egress_with_rate(256, 2048).await;

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
                file_activity: &file_activity,
                user_area_root: None,
                registry: &registry,
                egress: Some(transfer_egress),
            },
        );

        let file_data = vec![0x44; 2048];
        let mut reader = std::io::Cursor::new(file_data.clone());
        let started = tokio::time::Instant::now();
        let written = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            transfer.stream_file_to_client("FileData", &mut reader, file_data.len() as u64),
        )
        .await
        .expect("rate-limited transfer should eventually complete")
        .unwrap();
        let elapsed = started.elapsed();

        assert_eq!(written, file_data.len() as u64);
        assert!(
            elapsed >= std::time::Duration::from_millis(200),
            "FileData completed too quickly for configured egress rate: {elapsed:?}"
        );

        let frame = client_reader.read_frame_header().await.unwrap().unwrap();
        assert_eq!(frame.message_type, "FileData");
        assert_eq!(frame.payload_length, file_data.len() as u64);
        let payload = client_reader.read_payload_into_vec(&frame).await.unwrap();
        assert_eq!(payload, file_data);

        drop(transfer);
        stop_egress_task(egress, egress_task, connection_id).await;
    }

    #[tokio::test]
    async fn egress_file_data_first_read_error_does_not_start_frame() {
        let registry = TransferRegistry::new();
        let (client, server) = duplex(4096);
        let (client_read, _client_write) = tokio::io::split(client);
        let (server_read, server_write) = tokio::io::split(server);
        let mut client_reader = FrameReader::new(tokio::io::BufReader::new(client_read));

        let peer_addr = make_test_addr();
        let (info, ban_rx) = registry.register(TransferRegistration {
            user_id: 1,
            peer_addr,
            nickname: "testuser".to_string(),
            username: "testuser".to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Download,
            path: "/test/file.zip".to_string(),
            total_size: 10,
        });

        let temp_dir = TempDir::new().unwrap();
        let file_root = temp_dir.path();
        let file_index = make_test_file_index(&temp_dir);
        let file_activity = Arc::new(FileActivityMap::new());
        let (transfer_egress, egress, egress_task, connection_id) =
            registered_transfer_egress(1024).await;

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
                file_activity: &file_activity,
                user_area_root: None,
                registry: &registry,
                egress: Some(transfer_egress),
            },
        );

        let mut empty_reader = std::io::Cursor::new(Vec::<u8>::new());
        let result = transfer
            .stream_file_to_client("FileData", &mut empty_reader, 10)
            .await;
        assert!(matches!(result, Err(StreamError::Io(_))));

        transfer
            .send(&ServerMessage::TransferComplete {
                success: false,
                error: Some("read failed".to_string()),
                error_kind: Some("io_error".to_string()),
            })
            .await
            .unwrap();
        let response = read_server_message(&mut client_reader)
            .await
            .unwrap()
            .unwrap()
            .message;
        assert!(matches!(
            response,
            ServerMessage::TransferComplete { success: false, .. }
        ));

        drop(transfer);
        stop_egress_task(egress, egress_task, connection_id).await;
    }

    #[tokio::test]
    async fn egress_file_data_post_begin_read_error_clears_stream_state() {
        let registry = TransferRegistry::new();
        let (_client, server) = duplex(256 * 1024);
        let (server_read, server_write) = tokio::io::split(server);

        let peer_addr = make_test_addr();
        let (info, ban_rx) = registry.register(TransferRegistration {
            user_id: 1,
            peer_addr,
            nickname: "testuser".to_string(),
            username: "testuser".to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Download,
            path: "/test/file.zip".to_string(),
            total_size: CHUNK_SIZE as u64 + 1,
        });

        let temp_dir = TempDir::new().unwrap();
        let file_root = temp_dir.path();
        let file_index = make_test_file_index(&temp_dir);
        let file_activity = Arc::new(FileActivityMap::new());
        let (transfer_egress, egress, egress_task, connection_id) =
            registered_transfer_egress(1024).await;

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
                file_activity: &file_activity,
                user_area_root: None,
                registry: &registry,
                egress: Some(transfer_egress),
            },
        );

        let mut reader = ErrorAfterDataReader::new(vec![0xCC; CHUNK_SIZE]);
        let result = transfer
            .stream_file_to_client("FileData", &mut reader, CHUNK_SIZE as u64 + 1)
            .await;
        assert!(matches!(result, Err(StreamError::FrameStarted(_))));
        assert_eq!(info.get_bytes_transferred(), CHUNK_SIZE as u64);

        let result = transfer
            .send(&ServerMessage::TransferComplete {
                success: false,
                error: Some("stream failed".to_string()),
                error_kind: Some("io_error".to_string()),
            })
            .await;
        assert!(matches!(result, Err(StreamError::Io(_))));

        drop(transfer);

        let (dispatch_tx, dispatch_rx) = mpsc::channel(egress::EGRESS_DISPATCH_QUEUE_CAPACITY);
        assert!(
            egress
                .register_anon(connection_id, ConnectionClass::Bulk, dispatch_tx)
                .await
                .unwrap()
        );

        let (client, server) = duplex(4096);
        let (client_read, _client_write) = tokio::io::split(client);
        let (server_read, server_write) = tokio::io::split(server);
        let mut client_reader = FrameReader::new(tokio::io::BufReader::new(client_read));
        let (info, ban_rx) = registry.register(TransferRegistration {
            user_id: 1,
            peer_addr,
            nickname: "testuser".to_string(),
            username: "testuser".to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Download,
            path: "/test/recovered.txt".to_string(),
            total_size: 9,
        });

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
                file_activity: &file_activity,
                user_area_root: None,
                registry: &registry,
                egress: Some(TransferEgress::new(
                    egress.clone(),
                    connection_id,
                    dispatch_rx,
                )),
            },
        );

        let file_data = b"recovered".to_vec();
        let mut reader = std::io::Cursor::new(file_data.clone());
        let written = transfer
            .stream_file_to_client("FileData", &mut reader, file_data.len() as u64)
            .await
            .unwrap();
        assert_eq!(written, file_data.len() as u64);
        assert_eq!(info.get_bytes_transferred(), file_data.len() as u64);

        let frame = client_reader.read_frame_header().await.unwrap().unwrap();
        assert_eq!(frame.message_type, "FileData");
        assert_eq!(frame.payload_length, file_data.len() as u64);
        let payload = client_reader.read_payload_into_vec(&frame).await.unwrap();
        assert_eq!(payload, file_data);

        drop(transfer);
        stop_egress_task(egress, egress_task, connection_id).await;
    }

    #[tokio::test]
    async fn egress_file_data_banned_mid_stream_clears_stream_state() {
        let registry = TransferRegistry::new();
        let (_client, server) = duplex(256 * 1024);
        let (server_read, server_write) = tokio::io::split(server);

        let peer_addr = make_test_addr();
        let (info, ban_rx) = registry.register(TransferRegistration {
            user_id: 1,
            peer_addr,
            nickname: "testuser".to_string(),
            username: "testuser".to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Download,
            path: "/test/file.zip".to_string(),
            total_size: CHUNK_SIZE as u64 + 1,
        });

        let temp_dir = TempDir::new().unwrap();
        let file_root = temp_dir.path();
        let file_index = make_test_file_index(&temp_dir);
        let file_activity = Arc::new(FileActivityMap::new());
        let (transfer_egress, egress, egress_task, connection_id) =
            registered_transfer_egress(1024).await;

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
                file_activity: &file_activity,
                user_area_root: None,
                registry: &registry,
                egress: Some(transfer_egress),
            },
        );

        let mut reader = BanAfterFirstRead::new(vec![0xAB; CHUNK_SIZE], &registry);
        let result = transfer
            .stream_file_to_client("FileData", &mut reader, CHUNK_SIZE as u64 + 1)
            .await;
        assert!(matches!(result, Err(StreamError::Banned)));
        assert_eq!(info.get_bytes_transferred(), CHUNK_SIZE as u64);

        let result = transfer
            .send(&ServerMessage::TransferComplete {
                success: false,
                error: Some("banned".to_string()),
                error_kind: None,
            })
            .await;
        assert!(matches!(result, Err(StreamError::Io(_))));

        drop(transfer);
        stop_egress_task(egress, egress_task, connection_id).await;
    }

    #[tokio::test]
    async fn test_stream_file_to_client_banned_mid_stream() {
        // Ban before a multi-chunk stream; the loop's between-chunk check trips.
        let registry = TransferRegistry::new();
        let (_client, server) = duplex(1024 * 1024);
        let (server_read, server_write) = tokio::io::split(server);

        let peer_addr = make_test_addr();
        let (info, ban_rx) = registry.register(TransferRegistration {
            user_id: 1,
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
        let file_activity = Arc::new(FileActivityMap::new());

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
                file_activity: &file_activity,
                user_area_root: None,
                registry: &registry,
                egress: None,
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
            user_id: 1,
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
        let file_activity = Arc::new(FileActivityMap::new());

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
                file_activity: &file_activity,
                user_area_root: None,
                registry: &registry,
                egress: None,
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
            user_id: 1,
            peer_addr,
            nickname: "testuser".to_string(),
            username: "testuser".to_string(),
            is_admin: false,
            is_shared: false,
            direction: TransferDirection::Download,
            path: "/test/file.zip".to_string(),
            total_size: 0,
        });

        assert_eq!(registry.snapshot().len(), 1);

        {
            let (_client, server) = duplex(1024);
            let (server_read, server_write) = tokio::io::split(server);

            let temp_dir = TempDir::new().unwrap();
            let file_root = temp_dir.path();
            let file_index = make_test_file_index(&temp_dir);
            let file_activity = Arc::new(FileActivityMap::new());

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
                    file_activity: &file_activity,
                    user_area_root: None,
                    registry: &registry,
                    egress: None,
                },
            );

            assert_eq!(registry.snapshot().len(), 1);
        } // transfer dropped here

        assert_eq!(registry.snapshot().len(), 0);
    }

    #[tokio::test]
    async fn test_transfer_info_bytes_update() {
        let registry = TransferRegistry::new();
        let (_client, server) = duplex(1024 * 1024);
        let (server_read, server_write) = tokio::io::split(server);

        let peer_addr = make_test_addr();
        let (info, ban_rx) = registry.register(TransferRegistration {
            user_id: 1,
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
        let file_activity = Arc::new(FileActivityMap::new());

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
                file_activity: &file_activity,
                user_area_root: None,
                registry: &registry,
                egress: None,
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
