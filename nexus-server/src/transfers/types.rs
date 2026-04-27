//! Type definitions for file transfer handling
//!
//! Contains shared structs and enums used across the transfer module.

use std::collections::HashSet;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::db::{Database, Permission};
use crate::files::FileIndex;

use super::registry::TransferRegistry;

/// Parameters for handling a transfer connection
pub struct TransferParams {
    pub peer_addr: SocketAddr,
    pub db: Database,
    pub file_root: Option<&'static Path>,
    pub file_index: Arc<FileIndex>,
    /// Transfer registry for ban signal handling
    pub transfer_registry: Arc<TransferRegistry>,
    /// Server certificate fingerprint, included in HandshakeResponse so the
    /// client can detect TLS interception before sending credentials.
    pub fingerprint: &'static str,
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
    pub nickname: String,
    pub username: String,
    pub is_admin: bool,
    pub is_shared: bool,
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

/// Parameters for receiving a file upload
pub(crate) struct ReceiveFileParams<'a> {
    pub area_root: &'a Path,
    pub destination: &'a Path,
    pub locale: &'a str,
    pub transfer_id: &'a str,
    pub file_index: u64,
}
