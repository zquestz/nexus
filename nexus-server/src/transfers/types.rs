//! Type definitions for file transfer handling
//!
//! Contains shared structs and enums used across the transfer module.

use std::collections::HashSet;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use nexus_common::framing::{FrameReader, FrameWriter};

use crate::db::{Database, Permission};
use crate::files::FileIndex;

/// Parameters for handling a transfer connection
pub struct TransferParams {
    pub peer_addr: SocketAddr,
    pub db: Database,
    pub debug: bool,
    pub file_root: Option<&'static Path>,
    pub file_index: Arc<FileIndex>,
}

/// Information about a file to transfer (for downloads)
pub(crate) struct FileInfo {
    /// Relative path from download root (e.g., "Games/app.zip")
    pub relative_path: String,
    /// Absolute filesystem path
    pub absolute_path: PathBuf,
    /// File size in bytes
    pub size: u64,
}

/// Authenticated user information (minimal for transfer port)
pub(crate) struct AuthenticatedUser {
    pub username: String,
    pub is_admin: bool,
    pub permissions: HashSet<Permission>,
}

/// Request type after authentication (either download or upload)
pub(crate) enum TransferRequest {
    Download(DownloadParams),
    Upload(UploadParams),
}

/// Parameters for a download request
pub(crate) struct DownloadParams {
    pub path: String,
    pub root: bool,
}

/// Parameters for an upload request
pub(crate) struct UploadParams {
    pub destination: String,
    pub file_count: u64,
    pub total_size: u64,
    pub root: bool,
}

/// Context for handling a transfer (download or upload)
pub(crate) struct TransferContext<'a, R, W>
where
    R: AsyncReadExt + Unpin,
    W: AsyncWriteExt + Unpin,
{
    pub frame_reader: &'a mut FrameReader<R>,
    pub frame_writer: &'a mut FrameWriter<W>,
    pub user: &'a AuthenticatedUser,
    pub file_root: &'a Path,
    pub locale: &'a str,
    pub peer_addr: SocketAddr,
    pub debug: bool,
    pub file_index: &'a Arc<FileIndex>,
}

/// Parameters for receiving a file upload
pub(crate) struct ReceiveFileParams<'a> {
    pub area_root: &'a Path,
    pub destination: &'a Path,
    pub locale: &'a str,
    pub debug: bool,
    pub transfer_id: &'a str,
    pub file_index: u64,
}
