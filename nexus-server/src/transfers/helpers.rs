//! Helper utilities for file transfer handling
//!
//! Contains utility functions for sending error responses, generating transfer IDs,
//! shared validation helpers, and common path resolution utilities.

use std::io;
use std::path::{Path, PathBuf};

use tokio::io::AsyncWriteExt;

use nexus_common::framing::{FrameWriter, MessageId};
use nexus_common::io::send_server_message_with_id;
use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{self, FilePathError};
use nexus_common::{
    ERROR_KIND_CONFLICT, ERROR_KIND_EXISTS, ERROR_KIND_HASH_MISMATCH, ERROR_KIND_INVALID,
    ERROR_KIND_IO_ERROR, ERROR_KIND_NOT_FOUND, ERROR_KIND_PERMISSION, ERROR_KIND_PROTOCOL_ERROR,
};

use crate::db::Permission;
use crate::files::area::resolve_user_area;
use crate::files::path::{PathError, build_and_validate_candidate_path};
use crate::handlers::{
    err_file_area_not_accessible, err_permission_denied, err_transfer_path_invalid,
    err_transfer_path_not_found, err_transfer_path_too_long,
};

use super::types::AuthenticatedUser;

// =============================================================================
// Transfer Error Type
// =============================================================================

/// Error type for file transfer operations
///
/// This provides structured error handling with both human-readable messages
/// and machine-readable error kinds for client decision-making.
#[derive(Debug, Clone)]
pub struct TransferError {
    /// Human-readable, translated error message
    pub message: String,
    /// Machine-readable error kind (e.g., "exists", "permission")
    pub kind: &'static str,
}

impl TransferError {
    /// Create a new transfer error
    pub fn new(message: impl Into<String>, kind: &'static str) -> Self {
        Self {
            message: message.into(),
            kind,
        }
    }

    /// Create an "invalid" error (validation failure)
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(message, ERROR_KIND_INVALID)
    }

    /// Create a "not_found" error
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(message, ERROR_KIND_NOT_FOUND)
    }

    /// Create a "permission" error
    pub fn permission(message: impl Into<String>) -> Self {
        Self::new(message, ERROR_KIND_PERMISSION)
    }

    /// Create an "io_error" error
    pub fn io_error(message: impl Into<String>) -> Self {
        Self::new(message, ERROR_KIND_IO_ERROR)
    }

    /// Create a "protocol_error" error
    pub fn protocol_error(message: impl Into<String>) -> Self {
        Self::new(message, ERROR_KIND_PROTOCOL_ERROR)
    }

    /// Create an "exists" error (file already exists)
    pub fn exists(message: impl Into<String>) -> Self {
        Self::new(message, ERROR_KIND_EXISTS)
    }

    /// Create a "conflict" error (concurrent upload)
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(message, ERROR_KIND_CONFLICT)
    }

    /// Create a "hash_mismatch" error
    pub fn hash_mismatch(message: impl Into<String>) -> Self {
        Self::new(message, ERROR_KIND_HASH_MISMATCH)
    }
}

impl std::fmt::Display for TransferError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.kind)
    }
}

impl std::error::Error for TransferError {}

// =============================================================================
// Path Validation
// =============================================================================

/// Validate a file path and return a translated error message if invalid
///
/// This is a shared helper that handles the common `FilePathError` to
/// translated error message conversion used by both download and upload.
pub(crate) fn validate_transfer_path(path: &str, locale: &str) -> Result<(), TransferError> {
    if let Err(e) = validators::validate_file_path(path) {
        let error_msg = match e {
            FilePathError::TooLong => err_transfer_path_too_long(locale),
            FilePathError::ContainsNull
            | FilePathError::InvalidCharacters
            | FilePathError::ContainsWindowsDrive => err_transfer_path_invalid(locale),
        };
        return Err(TransferError::invalid(error_msg));
    }
    Ok(())
}

/// Check if the user has the required permission
///
/// Returns `Ok(())` if the user is admin or has the permission,
/// otherwise returns a permission error.
pub(crate) fn check_permission(
    user: &AuthenticatedUser,
    permission: Permission,
    locale: &str,
) -> Result<(), TransferError> {
    if !user.is_admin && !user.permissions.contains(&permission) {
        return Err(TransferError::permission(err_permission_denied(locale)));
    }
    Ok(())
}

/// Check file_root permission if root mode is requested
///
/// Returns `Ok(())` if root mode is not requested, or if the user has file_root permission.
pub(crate) fn check_root_permission(
    user: &AuthenticatedUser,
    use_root: bool,
    locale: &str,
) -> Result<(), TransferError> {
    if use_root {
        check_permission(user, Permission::FileRoot, locale)?;
    }
    Ok(())
}

// =============================================================================
// Path Resolution
// =============================================================================

/// Resolve and canonicalize the area root for a user
///
/// If `use_root` is true, returns the file root directly.
/// Otherwise, returns the user's personal area (or shared area).
pub(crate) fn resolve_area_root(
    file_root: &Path,
    username: &str,
    use_root: bool,
    locale: &str,
) -> Result<PathBuf, TransferError> {
    let area_root = if use_root {
        file_root.to_path_buf()
    } else {
        resolve_user_area(file_root, username)
    };

    std::fs::canonicalize(&area_root)
        .map_err(|_| TransferError::not_found(err_file_area_not_accessible(locale)))
}

/// Build and validate a candidate path within an area root
///
/// Converts `PathError` to `TransferError` with appropriate error kinds.
pub(crate) fn build_validated_path(
    area_root: &Path,
    client_path: &str,
    locale: &str,
) -> Result<PathBuf, TransferError> {
    build_and_validate_candidate_path(area_root, client_path)
        .map_err(|_| TransferError::invalid(err_transfer_path_invalid(locale)))
}

/// Convert a PathError to a TransferError
pub(crate) fn path_error_to_transfer_error(e: PathError, locale: &str) -> TransferError {
    match e {
        PathError::NotFound => TransferError::not_found(err_transfer_path_not_found(locale)),
        PathError::AccessDenied => TransferError::permission(err_transfer_path_invalid(locale)),
        _ => TransferError::invalid(err_transfer_path_invalid(locale)),
    }
}

// =============================================================================
// Response Helpers
// =============================================================================

/// Create a LoginResponse error message (simplified for transfer port)
pub(crate) fn login_error_response(error: String) -> ServerMessage {
    ServerMessage::LoginResponse {
        success: false,
        error: Some(error),
        session_id: None,
        is_admin: None,
        permissions: None,
        server_info: None,
        locale: None,
        channels: None,
    }
}

/// Send a download error response and close the connection
///
/// This is a convenience wrapper that sends the error, shuts down the writer,
/// and returns `Ok(())` for early exit from the handler.
pub(crate) async fn send_download_error_and_close<W>(
    frame_writer: &mut FrameWriter<W>,
    error: &str,
    error_kind: Option<&str>,
) -> io::Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    let response = ServerMessage::FileDownloadResponse {
        success: false,
        error: Some(error.to_string()),
        error_kind: error_kind.map(String::from),
        size: None,
        file_count: None,
        transfer_id: None,
    };
    let _ = send_server_message_with_id(frame_writer, &response, MessageId::new()).await;
    let _ = frame_writer.get_mut().shutdown().await;
    Ok(())
}

/// Send a download error from a TransferError and close the connection
pub(crate) async fn send_download_transfer_error<W>(
    frame_writer: &mut FrameWriter<W>,
    error: &TransferError,
) -> io::Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    send_download_error_and_close(frame_writer, &error.message, Some(error.kind)).await
}

/// Send an upload error response and close the connection
///
/// This is a convenience wrapper that sends the error, shuts down the writer,
/// and returns `Ok(())` for early exit from the handler.
pub(crate) async fn send_upload_error_and_close<W>(
    frame_writer: &mut FrameWriter<W>,
    error: &str,
    error_kind: Option<&str>,
) -> io::Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    let response = ServerMessage::FileUploadResponse {
        success: false,
        error: Some(error.to_string()),
        error_kind: error_kind.map(String::from),
        transfer_id: None,
    };
    let _ = send_server_message_with_id(frame_writer, &response, MessageId::new()).await;
    let _ = frame_writer.get_mut().shutdown().await;
    Ok(())
}

/// Send an upload error from a TransferError and close the connection
pub(crate) async fn send_upload_transfer_error<W>(
    frame_writer: &mut FrameWriter<W>,
    error: &TransferError,
) -> io::Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    send_upload_error_and_close(frame_writer, &error.message, Some(error.kind)).await
}

/// Send a generic error response and close the connection
///
/// Used when the client sends an unexpected message type and we can't
/// respond with a specific response type (e.g., FileDownloadResponse
/// or FileUploadResponse) because we don't know what they intended.
pub(crate) async fn send_error_and_close<W>(
    frame_writer: &mut FrameWriter<W>,
    error: &str,
) -> io::Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    let response = ServerMessage::Error {
        message: error.to_string(),
        command: None,
    };
    let _ = send_server_message_with_id(frame_writer, &response, MessageId::new()).await;
    let _ = frame_writer.get_mut().shutdown().await;
    Ok(())
}

// =============================================================================
// Utilities
// =============================================================================

/// Generate a random transfer ID (8 hex chars, 32 bits)
///
/// Used for log correlation to track all messages related to a single transfer.
/// This is NOT cryptographically secure and should NOT be used for authentication
/// or security-sensitive purposes.
pub(crate) fn generate_transfer_id() -> String {
    use rand::Rng;
    let bytes: [u8; 4] = rand::rng().random();
    hex::encode(bytes)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transfer_error_display() {
        let err = TransferError::permission("Access denied".to_string());
        assert_eq!(format!("{err}"), "Access denied (permission)");
    }

    #[test]
    fn test_transfer_error_kinds() {
        assert_eq!(TransferError::invalid("x").kind, ERROR_KIND_INVALID);
        assert_eq!(TransferError::not_found("x").kind, ERROR_KIND_NOT_FOUND);
        assert_eq!(TransferError::permission("x").kind, ERROR_KIND_PERMISSION);
        assert_eq!(TransferError::io_error("x").kind, ERROR_KIND_IO_ERROR);
        assert_eq!(
            TransferError::protocol_error("x").kind,
            ERROR_KIND_PROTOCOL_ERROR
        );
        assert_eq!(TransferError::exists("x").kind, ERROR_KIND_EXISTS);
        assert_eq!(TransferError::conflict("x").kind, ERROR_KIND_CONFLICT);
        assert_eq!(
            TransferError::hash_mismatch("x").kind,
            ERROR_KIND_HASH_MISMATCH
        );
    }

    #[test]
    fn test_generate_transfer_id_format() {
        let id = generate_transfer_id();
        assert_eq!(id.len(), 8);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_generate_transfer_id_uniqueness() {
        // Generate multiple IDs and verify they're different
        let ids: Vec<_> = (0..100).map(|_| generate_transfer_id()).collect();
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        // With 32 bits of randomness, collisions in 100 samples are extremely unlikely
        assert!(unique.len() >= 99);
    }
}
