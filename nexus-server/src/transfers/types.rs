//! Shared structs and enums used across the transfer module.

use std::collections::HashSet;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::db::{Database, Permission};
use crate::egress::task::EgressHandle;
use crate::files::{FileActivityMap, FileIndex};
use crate::ip_rule_cache::IpRuleState;
use crate::scheduler::ConnectionId;
use crate::users::UserManager;
use nexus_common::rate_limiter::RateLimiter;

use super::registry::TransferRegistry;

/// Parameters for handling a transfer connection
pub struct TransferParams {
    pub peer_addr: SocketAddr,
    pub db: Database,
    pub file_root: Option<&'static Path>,
    pub file_index: Arc<FileIndex>,
    /// See `files::activity`.
    pub file_activity: Arc<FileActivityMap>,
    pub transfer_registry: Arc<TransferRegistry>,
    pub ip_rule_cache: Arc<IpRuleState>,
    pub login_limiter: Arc<RateLimiter>,
    pub user_manager: UserManager,
    /// Sent in HandshakeResponse so the client can detect TLS interception
    /// before sending credentials.
    pub fingerprint: &'static str,
    pub egress: EgressHandle,
    pub egress_connection_id: ConnectionId,
    pub lan_egress_bypass_enabled: bool,
}

/// A file to download.
pub(crate) struct FileInfo {
    /// Relative path from download root (e.g. "Games/app.zip").
    pub relative_path: String,
    pub absolute_path: PathBuf,
    pub size: u64,
}

/// Minimal authenticated user for the transfer port.
pub(crate) struct AuthenticatedUser {
    pub user_id: i64,
    pub nickname: String,
    pub username: String,
    pub is_admin: bool,
    pub is_shared: bool,
    pub permissions: HashSet<Permission>,
}

pub(crate) enum TransferRequest {
    Download(DownloadParams),
    Upload(UploadParams),
}

pub(crate) struct DownloadParams {
    pub path: String,
    pub root: bool,
}

pub(crate) struct UploadParams {
    pub destination: String,
    pub file_count: u64,
    pub total_size: u64,
    pub root: bool,
}

pub(crate) struct ReceiveFileParams<'a> {
    pub area_root: &'a Path,
    pub destination: &'a Path,
    pub upload_targets: &'a mut HashSet<PathBuf>,
    pub locale: &'a str,
    pub transfer_id: &'a str,
    pub file_index: u64,
}
