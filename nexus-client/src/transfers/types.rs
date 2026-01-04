//! Transfer types for file download and upload management
//!
//! These types are used to track file transfers and persist them across
//! application restarts for resume support.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use crate::types::ConnectionInfo;

// =============================================================================
// Transfer Direction
// =============================================================================

/// Direction of the transfer
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransferDirection {
    /// Downloading from server to local
    Download,
    /// Uploading from local to server
    #[allow(dead_code)] // For future upload support
    Upload,
}

// =============================================================================
// Transfer Status
// =============================================================================

/// Current status of a transfer
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransferStatus {
    /// Waiting to start (in queue)
    Queued,
    /// Establishing connection to transfer port
    Connecting,
    /// Actively transferring data
    Transferring,
    /// Paused by user (can be resumed)
    Paused,
    /// Successfully completed
    Completed,
    /// Failed with error (may be resumable)
    Failed,
}

impl TransferStatus {
    /// Returns true if the transfer can be resumed
    pub fn is_resumable(&self) -> bool {
        matches!(self, TransferStatus::Paused | TransferStatus::Failed)
    }

    /// Returns true if the transfer is active (connecting or transferring)
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            TransferStatus::Connecting | TransferStatus::Transferring
        )
    }

    /// Returns true if the transfer completed successfully
    pub fn is_completed(&self) -> bool {
        matches!(self, TransferStatus::Completed)
    }

    /// Returns true if the transfer failed
    pub fn is_failed(&self) -> bool {
        matches!(self, TransferStatus::Failed)
    }
}

// =============================================================================
// Transfer Error
// =============================================================================

/// Error kinds for transfer operations
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferError {
    /// Path doesn't exist on server
    NotFound,
    /// Permission denied
    Permission,
    /// Invalid path (malformed)
    Invalid,
    /// Protocol version not supported
    UnsupportedVersion,
    /// SHA-256 verification failed
    HashMismatch,
    /// File I/O error
    IoError,
    /// Protocol error (invalid/unexpected data)
    ProtocolError,
    /// Connection error (network failure)
    ConnectionError,
    /// Certificate fingerprint mismatch
    CertificateMismatch,
    /// Authentication failed
    AuthenticationFailed,
    /// Unknown error
    Unknown,
}

impl TransferError {
    /// Parse error_kind string from server
    pub fn from_server_error_kind(kind: &str) -> Self {
        match kind {
            "not_found" => TransferError::NotFound,
            "permission" => TransferError::Permission,
            "invalid" => TransferError::Invalid,
            "io_error" => TransferError::IoError,
            "protocol_error" => TransferError::ProtocolError,
            _ => TransferError::Unknown,
        }
    }

    /// Get the i18n translation key for this error
    pub fn to_i18n_key(&self) -> &'static str {
        match self {
            TransferError::NotFound => "transfer-error-not-found",
            TransferError::Permission => "transfer-error-permission",
            TransferError::Invalid => "transfer-error-invalid",
            TransferError::UnsupportedVersion => "transfer-error-unsupported-version",
            TransferError::HashMismatch => "transfer-error-hash-mismatch",
            TransferError::IoError => "transfer-error-io",
            TransferError::ProtocolError => "transfer-error-protocol",
            TransferError::ConnectionError => "transfer-error-connection",
            TransferError::CertificateMismatch => "transfer-error-certificate-mismatch",
            TransferError::AuthenticationFailed => "transfer-error-auth-failed",
            TransferError::Unknown => "transfer-error-unknown",
        }
    }
}

// =============================================================================
// Transfer
// =============================================================================

/// A file or directory transfer
///
/// Represents a single transfer operation which may contain multiple files
/// (for directory downloads). The transfer persists across application restarts
/// to support resume.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transfer {
    /// Unique identifier for this transfer
    pub id: Uuid,

    /// Bookmark ID if this transfer originated from a saved bookmark
    ///
    /// Used for UI display (show bookmark name). The connection info is always
    /// used for actual reconnection since bookmarks can be modified.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bookmark_id: Option<Uuid>,

    /// Connection info for reconnecting
    pub connection_info: ConnectionInfo,

    /// Direction of transfer (download or upload)
    pub direction: TransferDirection,

    /// Path on the server (e.g., "/Games/app.zip")
    pub remote_path: String,

    /// Whether the remote path uses root mode (requires file_root permission)
    #[serde(default)]
    pub remote_root: bool,

    /// Whether this is a directory download (affects how local_path is used)
    ///
    /// When true, local_path is the base directory and files are saved relative to it.
    /// When false, local_path is the exact file path to save to.
    #[serde(default)]
    pub is_directory: bool,

    /// Local file or directory path
    pub local_path: PathBuf,

    /// Total size in bytes (all files combined)
    pub total_bytes: u64,

    /// Bytes transferred so far
    pub transferred_bytes: u64,

    /// Total number of files in transfer
    pub file_count: u64,

    /// Number of files completed
    pub files_completed: u64,

    /// Current status
    pub status: TransferStatus,

    /// Error message if status is Failed
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    /// Error kind if status is Failed
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_kind: Option<TransferError>,

    /// Transfer ID from server (for log correlation)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_transfer_id: Option<String>,

    /// Timestamp when transfer was created (queued)
    pub created_at: i64,

    /// Timestamp when transfer actually started (connected to server)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<i64>,

    /// Timestamp when transfer completed or failed
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<i64>,

    /// Current file being transferred (relative path)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_file: Option<String>,
}

impl Transfer {
    /// Create a new download transfer
    pub fn new_download(
        connection_info: ConnectionInfo,
        remote_path: String,
        remote_root: bool,
        is_directory: bool,
        local_path: PathBuf,
        bookmark_id: Option<Uuid>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            bookmark_id,
            connection_info,
            direction: TransferDirection::Download,
            remote_path,
            remote_root,
            is_directory,
            local_path,
            total_bytes: 0,
            transferred_bytes: 0,
            file_count: 0,
            files_completed: 0,
            status: TransferStatus::Queued,
            error: None,
            error_kind: None,
            server_transfer_id: None,
            created_at: chrono::Utc::now().timestamp(),
            started_at: None,
            completed_at: None,
            current_file: None,
        }
    }

    /// Create a new upload transfer
    #[allow(dead_code)] // For future upload support
    pub fn new_upload(
        connection_info: ConnectionInfo,
        remote_path: String,
        remote_root: bool,
        is_directory: bool,
        local_path: PathBuf,
        bookmark_id: Option<Uuid>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            bookmark_id,
            connection_info,
            direction: TransferDirection::Upload,
            remote_path,
            remote_root,
            is_directory,
            local_path,
            total_bytes: 0,
            transferred_bytes: 0,
            file_count: 0,
            files_completed: 0,
            status: TransferStatus::Queued,
            error: None,
            error_kind: None,
            server_transfer_id: None,
            created_at: chrono::Utc::now().timestamp(),
            started_at: None,
            completed_at: None,
            current_file: None,
        }
    }

    /// Calculate progress as a percentage (0.0 to 100.0)
    pub fn progress_percent(&self) -> f32 {
        if self.total_bytes == 0 {
            if self.status == TransferStatus::Completed {
                100.0
            } else {
                0.0
            }
        } else {
            (self.transferred_bytes as f64 / self.total_bytes as f64 * 100.0) as f32
        }
    }

    /// Get a human-readable display name for the transfer
    pub fn display_name(&self) -> &str {
        // Use the last component of the remote path
        let name = self
            .remote_path
            .rsplit('/')
            .next()
            .filter(|s| !s.is_empty());

        // For root directory downloads ("/"), use server name instead
        match name {
            Some(n) => n,
            None => &self.connection_info.server_name,
        }
    }

    /// Mark the transfer as failed with an error
    pub fn fail(&mut self, error: String, error_kind: Option<TransferError>) {
        self.status = TransferStatus::Failed;
        self.error = Some(error);
        self.error_kind = error_kind;
        self.completed_at = Some(chrono::Utc::now().timestamp());
    }

    /// Mark the transfer as completed
    pub fn complete(&mut self) {
        self.status = TransferStatus::Completed;
        self.error = None;
        self.error_kind = None;
        self.completed_at = Some(chrono::Utc::now().timestamp());
    }

    /// Pause the transfer
    pub fn pause(&mut self) {
        if self.status.is_active() {
            self.status = TransferStatus::Paused;
        }
    }

    /// Re-queue a paused or failed transfer for resume
    pub fn queue(&mut self) {
        if self.status.is_resumable() {
            self.status = TransferStatus::Queued;
            self.error = None;
            self.error_kind = None;
            // Reset timestamps for the new attempt
            self.started_at = None;
            self.completed_at = None;
        }
    }

    /// Mark the transfer as started (called when connection established)
    pub fn start(&mut self) {
        self.started_at = Some(chrono::Utc::now().timestamp());
    }

    /// Calculate elapsed time in seconds (from start to now or completion)
    pub fn elapsed_seconds(&self) -> Option<i64> {
        let start = self.started_at?;
        let end = self
            .completed_at
            .unwrap_or_else(|| chrono::Utc::now().timestamp());
        Some(end - start)
    }

    /// Calculate transfer speed in bytes per second
    pub fn bytes_per_second(&self) -> Option<f64> {
        let elapsed = self.elapsed_seconds()?;
        if elapsed > 0 {
            Some(self.transferred_bytes as f64 / elapsed as f64)
        } else {
            None
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn test_connection_info() -> ConnectionInfo {
        ConnectionInfo {
            server_name: "Test Server".to_string(),
            address: "192.168.1.1".to_string(),
            port: 7500,
            transfer_port: 7501,
            certificate_fingerprint: "AA:BB:CC".to_string(),
            username: "alice".to_string(),
            password: "secret".to_string(),
            nickname: String::new(),
        }
    }

    #[test]
    fn test_transfer_new_download() {
        let conn = test_connection_info();
        let transfer = Transfer::new_download(
            conn.clone(),
            "/Games/app.zip".to_string(),
            false,
            false,
            PathBuf::from("/home/user/Downloads/app.zip"),
            None,
        );

        assert_eq!(transfer.direction, TransferDirection::Download);
        assert_eq!(transfer.remote_path, "/Games/app.zip");
        assert_eq!(transfer.status, TransferStatus::Queued);
        assert_eq!(transfer.total_bytes, 0);
        assert_eq!(transfer.transferred_bytes, 0);
    }

    #[test]
    fn test_transfer_progress_percent() {
        let conn = test_connection_info();
        let mut transfer = Transfer::new_download(
            conn,
            "/test.zip".to_string(),
            false,
            false,
            PathBuf::from("/tmp/test.zip"),
            None,
        );

        // 0 bytes total, not completed
        assert_eq!(transfer.progress_percent(), 0.0);

        // 0 bytes total, completed
        transfer.status = TransferStatus::Completed;
        assert_eq!(transfer.progress_percent(), 100.0);

        // Normal progress
        transfer.status = TransferStatus::Transferring;
        transfer.total_bytes = 1000;
        transfer.transferred_bytes = 250;
        assert!((transfer.progress_percent() - 25.0).abs() < 0.01);

        // Full progress
        transfer.transferred_bytes = 1000;
        assert!((transfer.progress_percent() - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_transfer_display_name() {
        let conn = test_connection_info();
        let transfer = Transfer::new_download(
            conn.clone(),
            "/Games/Emulators/app.zip".to_string(),
            false,
            false,
            PathBuf::from("/tmp/app.zip"),
            None,
        );
        assert_eq!(transfer.display_name(), "app.zip");

        // Root directory downloads use server name as display name
        let transfer2 = Transfer::new_download(
            conn.clone(),
            "/".to_string(),
            false,
            true,
            PathBuf::from("/tmp/root"),
            None,
        );
        assert_eq!(transfer2.display_name(), "Test Server");

        let transfer3 = Transfer::new_download(
            conn,
            "single_file.txt".to_string(),
            false,
            false,
            PathBuf::from("/tmp/single_file.txt"),
            None,
        );
        assert_eq!(transfer3.display_name(), "single_file.txt");
    }

    #[test]
    fn test_transfer_status_methods() {
        assert!(TransferStatus::Paused.is_resumable());
        assert!(TransferStatus::Failed.is_resumable());
        assert!(!TransferStatus::Queued.is_resumable());
        assert!(!TransferStatus::Completed.is_resumable());

        assert!(TransferStatus::Connecting.is_active());
        assert!(TransferStatus::Transferring.is_active());
        assert!(!TransferStatus::Paused.is_active());
        assert!(!TransferStatus::Queued.is_active());

        assert!(TransferStatus::Completed.is_completed());
        assert!(!TransferStatus::Failed.is_completed());

        assert!(TransferStatus::Failed.is_failed());
        assert!(!TransferStatus::Completed.is_failed());
    }

    #[test]
    fn test_transfer_fail() {
        let conn = test_connection_info();
        let mut transfer = Transfer::new_download(
            conn,
            "/test.zip".to_string(),
            false,
            false,
            PathBuf::from("/tmp/test.zip"),
            None,
        );

        transfer.status = TransferStatus::Transferring;
        transfer.fail(
            "Connection lost".to_string(),
            Some(TransferError::ConnectionError),
        );

        assert_eq!(transfer.status, TransferStatus::Failed);
        assert_eq!(transfer.error, Some("Connection lost".to_string()));
        assert_eq!(transfer.error_kind, Some(TransferError::ConnectionError));
    }

    #[test]
    fn test_transfer_complete() {
        let conn = test_connection_info();
        let mut transfer = Transfer::new_download(
            conn,
            "/test.zip".to_string(),
            false,
            false,
            PathBuf::from("/tmp/test.zip"),
            None,
        );

        transfer.status = TransferStatus::Transferring;
        transfer.error = Some("Previous error".to_string());
        transfer.complete();

        assert_eq!(transfer.status, TransferStatus::Completed);
        assert!(transfer.error.is_none());
        assert!(transfer.error_kind.is_none());
    }

    #[test]
    fn test_transfer_pause_and_queue() {
        let conn = test_connection_info();
        let mut transfer = Transfer::new_download(
            conn,
            "/test.zip".to_string(),
            false,
            false,
            PathBuf::from("/tmp/test.zip"),
            None,
        );

        // Can't pause if not active
        transfer.pause();
        assert_eq!(transfer.status, TransferStatus::Queued);

        // Can pause if active
        transfer.status = TransferStatus::Transferring;
        transfer.pause();
        assert_eq!(transfer.status, TransferStatus::Paused);

        // Can queue if paused
        transfer.queue();
        assert_eq!(transfer.status, TransferStatus::Queued);

        // Can't queue if not resumable
        transfer.status = TransferStatus::Completed;
        transfer.queue();
        assert_eq!(transfer.status, TransferStatus::Completed);
    }

    #[test]
    fn test_transfer_error_from_server() {
        // These are the error_kind values the server actually sends
        assert_eq!(
            TransferError::from_server_error_kind("not_found"),
            TransferError::NotFound
        );
        assert_eq!(
            TransferError::from_server_error_kind("permission"),
            TransferError::Permission
        );
        assert_eq!(
            TransferError::from_server_error_kind("invalid"),
            TransferError::Invalid
        );
        assert_eq!(
            TransferError::from_server_error_kind("io_error"),
            TransferError::IoError
        );
        assert_eq!(
            TransferError::from_server_error_kind("protocol_error"),
            TransferError::ProtocolError
        );
        // Unknown values fall back to Unknown
        assert_eq!(
            TransferError::from_server_error_kind("unknown_thing"),
            TransferError::Unknown
        );
    }

    #[test]
    fn test_transfer_serialization_roundtrip() {
        let conn = test_connection_info();
        let transfer = Transfer::new_download(
            conn,
            "/Games/app.zip".to_string(),
            false,
            false,
            PathBuf::from("/home/user/Downloads/app.zip"),
            Some(Uuid::new_v4()),
        );

        let json = serde_json::to_string(&transfer).expect("serialize");
        let deserialized: Transfer = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(transfer.id, deserialized.id);
        assert_eq!(transfer.remote_path, deserialized.remote_path);
        assert_eq!(transfer.direction, deserialized.direction);
        assert_eq!(transfer.status, deserialized.status);
    }

    #[test]
    fn test_connection_info_with_nickname() {
        let mut conn = test_connection_info();
        conn.nickname = "Bob".to_string();

        let json = serde_json::to_string(&conn).expect("serialize");
        assert!(json.contains("Bob"));

        let deserialized: ConnectionInfo = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.nickname, "Bob");
    }

    #[test]
    fn test_connection_info_with_empty_nickname() {
        let conn = test_connection_info();
        assert!(conn.nickname.is_empty());
        let json = serde_json::to_string(&conn).expect("serialize");
        // Empty nickname is still serialized (as empty string)
        assert!(json.contains(r#""nickname":""#));
    }

    #[test]
    fn test_transfer_timestamps() {
        let conn = test_connection_info();
        let mut transfer = Transfer::new_download(
            conn,
            "/test.zip".to_string(),
            false,
            false,
            PathBuf::from("/tmp/test.zip"),
            None,
        );

        // Initially no timestamps except created_at
        assert!(transfer.created_at > 0);
        assert!(transfer.started_at.is_none());
        assert!(transfer.completed_at.is_none());

        // Start sets started_at
        transfer.start();
        assert!(transfer.started_at.is_some());
        assert!(transfer.completed_at.is_none());

        // Complete sets completed_at
        transfer.complete();
        assert!(transfer.started_at.is_some());
        assert!(transfer.completed_at.is_some());
    }

    #[test]
    fn test_transfer_timestamps_on_fail() {
        let conn = test_connection_info();
        let mut transfer = Transfer::new_download(
            conn,
            "/test.zip".to_string(),
            false,
            false,
            PathBuf::from("/tmp/test.zip"),
            None,
        );

        transfer.start();
        transfer.fail(
            "Connection lost".to_string(),
            Some(TransferError::ConnectionError),
        );

        assert!(transfer.started_at.is_some());
        assert!(transfer.completed_at.is_some());
    }

    #[test]
    fn test_transfer_timestamps_reset_on_requeue() {
        let conn = test_connection_info();
        let mut transfer = Transfer::new_download(
            conn,
            "/test.zip".to_string(),
            false,
            false,
            PathBuf::from("/tmp/test.zip"),
            None,
        );

        transfer.start();
        transfer.fail("Connection lost".to_string(), None);
        assert!(transfer.started_at.is_some());
        assert!(transfer.completed_at.is_some());

        // Re-queue should reset timestamps
        transfer.queue();
        assert!(transfer.started_at.is_none());
        assert!(transfer.completed_at.is_none());
    }

    #[test]
    fn test_transfer_elapsed_seconds() {
        let conn = test_connection_info();
        let mut transfer = Transfer::new_download(
            conn,
            "/test.zip".to_string(),
            false,
            false,
            PathBuf::from("/tmp/test.zip"),
            None,
        );

        // No elapsed time if not started
        assert!(transfer.elapsed_seconds().is_none());

        // Set timestamps manually for predictable test
        transfer.started_at = Some(1000);
        transfer.completed_at = Some(1010);

        assert_eq!(transfer.elapsed_seconds(), Some(10));
    }

    #[test]
    fn test_transfer_bytes_per_second() {
        let conn = test_connection_info();
        let mut transfer = Transfer::new_download(
            conn,
            "/test.zip".to_string(),
            false,
            false,
            PathBuf::from("/tmp/test.zip"),
            None,
        );

        // No speed if not started
        assert!(transfer.bytes_per_second().is_none());

        // Set values manually for predictable test
        transfer.started_at = Some(1000);
        transfer.completed_at = Some(1010);
        transfer.transferred_bytes = 10000;

        // 10000 bytes / 10 seconds = 1000 bytes/sec
        let speed = transfer.bytes_per_second().unwrap();
        assert!((speed - 1000.0).abs() < 0.01);
    }
}
