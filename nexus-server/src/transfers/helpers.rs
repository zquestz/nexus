//! Helper utilities for file transfer handling: error responses, transfer-ID
//! generation, shared validation, and path resolution.

use std::io;
use std::path::{Path, PathBuf};

use tokio::io::AsyncWriteExt;

use nexus_common::framing::{FrameWriter, MessageId};
use nexus_common::io::send_server_message_with_id;
use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{self, FilePathError};
use nexus_common::{
    ERROR_KIND_CAPACITY, ERROR_KIND_CONFLICT, ERROR_KIND_EXISTS, ERROR_KIND_HASH_MISMATCH,
    ERROR_KIND_INVALID, ERROR_KIND_IO_ERROR, ERROR_KIND_NOT_FOUND, ERROR_KIND_PERMISSION,
    ERROR_KIND_PROTOCOL_ERROR,
};

use crate::db::Permission;
use crate::files::path::{PathError, build_and_validate_candidate_path};
use crate::handlers::{
    err_file_area_not_accessible, err_permission_denied, err_transfer_path_invalid,
    err_transfer_path_not_found, err_transfer_path_too_long,
};

use super::types::AuthenticatedUser;

/// Structured transfer-response error: a translated message plus a
/// machine-readable kind the client branches on.
#[derive(Debug, Clone)]
pub struct TransferError {
    pub message: String,
    /// Machine-readable error kind (e.g. "exists", "permission").
    pub kind: &'static str,
}

impl TransferError {
    pub fn new(message: impl Into<String>, kind: &'static str) -> Self {
        Self {
            message: message.into(),
            kind,
        }
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(message, ERROR_KIND_INVALID)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(message, ERROR_KIND_NOT_FOUND)
    }

    pub fn permission(message: impl Into<String>) -> Self {
        Self::new(message, ERROR_KIND_PERMISSION)
    }

    pub fn io_error(message: impl Into<String>) -> Self {
        Self::new(message, ERROR_KIND_IO_ERROR)
    }

    pub fn protocol_error(message: impl Into<String>) -> Self {
        Self::new(message, ERROR_KIND_PROTOCOL_ERROR)
    }

    pub fn exists(message: impl Into<String>) -> Self {
        Self::new(message, ERROR_KIND_EXISTS)
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(message, ERROR_KIND_CONFLICT)
    }

    pub fn hash_mismatch(message: impl Into<String>) -> Self {
        Self::new(message, ERROR_KIND_HASH_MISMATCH)
    }

    pub fn capacity(message: impl Into<String>) -> Self {
        Self::new(message, ERROR_KIND_CAPACITY)
    }
}

impl std::fmt::Display for TransferError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.kind)
    }
}

impl std::error::Error for TransferError {}

/// Validate a transfer path, converting `FilePathError` to a translated error.
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

/// `Ok(())` if the user is admin or holds `permission`, else a permission error.
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

/// `Ok(())` if the user is admin or holds at least one of `permissions`, else
/// a permission error.
pub(crate) fn check_any_permission(
    user: &AuthenticatedUser,
    permissions: &[Permission],
    locale: &str,
) -> Result<(), TransferError> {
    if user.is_admin || permissions.iter().any(|p| user.permissions.contains(p)) {
        return Ok(());
    }
    Err(TransferError::permission(err_permission_denied(locale)))
}

/// Require `file_root` only when root mode is requested.
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

/// Resolve and canonicalize the area root: the file root when `use_root`,
/// otherwise the user's personal (or shared) area.
pub(crate) async fn resolve_area_root(
    file_root: &Path,
    user_area_root: Option<&Path>,
    use_root: bool,
    locale: &str,
) -> Result<PathBuf, TransferError> {
    let area_root = if use_root {
        file_root.to_path_buf()
    } else {
        user_area_root
            .ok_or_else(|| TransferError::not_found(err_file_area_not_accessible(locale)))?
            .to_path_buf()
    };

    tokio::fs::canonicalize(&area_root)
        .await
        .map_err(|_| TransferError::not_found(err_file_area_not_accessible(locale)))
}

/// Build and validate a candidate path within an area root.
pub(crate) async fn build_validated_path(
    area_root: &Path,
    client_path: &str,
    locale: &str,
) -> Result<PathBuf, TransferError> {
    build_and_validate_candidate_path(area_root, client_path)
        .await
        .map_err(|_| TransferError::invalid(err_transfer_path_invalid(locale)))
}

pub(crate) fn path_error_to_transfer_error(e: PathError, locale: &str) -> TransferError {
    match e {
        PathError::NotFound => TransferError::not_found(err_transfer_path_not_found(locale)),
        PathError::AccessDenied => TransferError::permission(err_transfer_path_invalid(locale)),
        _ => TransferError::invalid(err_transfer_path_invalid(locale)),
    }
}

/// Build a failure `LoginResponse` (simplified for the transfer port).
pub(crate) fn login_error_response(error: String) -> ServerMessage {
    ServerMessage::LoginResponse {
        success: false,
        error: Some(error),
        session_id: None,
        user_id: None,
        is_admin: None,
        permissions: None,
        features: None,
        server_info: None,
        locale: None,
        channels: None,
        nickname: None,
        group_id: None,
        group_name: None,
    }
}

/// Send a download error, shut down the writer, and return `Ok(())` so the
/// handler can early-exit.
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

pub(crate) async fn send_download_transfer_error<W>(
    frame_writer: &mut FrameWriter<W>,
    error: &TransferError,
) -> io::Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    send_download_error_and_close(frame_writer, &error.message, Some(error.kind)).await
}

/// Send an upload error, shut down the writer, and return `Ok(())` so the
/// handler can early-exit.
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

pub(crate) async fn send_upload_transfer_error<W>(
    frame_writer: &mut FrameWriter<W>,
    error: &TransferError,
) -> io::Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    send_upload_error_and_close(frame_writer, &error.message, Some(error.kind)).await
}

/// Send a generic error and close. Used when the client's message type is
/// unexpected, so no specific `File*Response` fits the intent.
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
        disconnect: true,
    };
    let _ = send_server_message_with_id(frame_writer, &response, MessageId::new()).await;
    let _ = frame_writer.get_mut().shutdown().await;
    Ok(())
}

/// Random 8-hex-char (32-bit) transfer ID for log correlation. NOT
/// cryptographically secure; never use for auth or anything security-sensitive.
pub(crate) fn generate_transfer_id() -> String {
    use rand::RngExt;
    let bytes: [u8; 4] = rand::rng().random();
    hex::encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::super::test_helpers::make_authenticated_user;
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
        assert_eq!(TransferError::capacity("x").kind, ERROR_KIND_CAPACITY);
    }

    #[test]
    fn test_generate_transfer_id_format() {
        let id = generate_transfer_id();
        assert_eq!(id.len(), 8);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_generate_transfer_id_uniqueness() {
        let ids: Vec<_> = (0..100).map(|_| generate_transfer_id()).collect();
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        // 32 bits of randomness makes collisions in 100 samples extremely unlikely.
        assert!(unique.len() >= 99);
    }

    #[test]
    fn test_check_any_permission_admin_always_ok() {
        let user = make_authenticated_user(true, &[]);
        assert!(check_any_permission(&user, &[Permission::FileUpload], "en").is_ok());
    }

    #[test]
    fn test_check_any_permission_admin_with_empty_list_ok() {
        // Admin passes even against an empty permission list.
        let user = make_authenticated_user(true, &[]);
        assert!(check_any_permission(&user, &[], "en").is_ok());
    }

    #[test]
    fn test_check_any_permission_has_first() {
        let user = make_authenticated_user(false, &[Permission::FileUpload]);
        let result = check_any_permission(
            &user,
            &[Permission::FileUpload, Permission::FileUploadAnywhere],
            "en",
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_any_permission_has_second() {
        // Motivating case: holding only FileUploadAnywhere (no base FileUpload)
        // still grants upload.
        let user = make_authenticated_user(false, &[Permission::FileUploadAnywhere]);
        let result = check_any_permission(
            &user,
            &[Permission::FileUpload, Permission::FileUploadAnywhere],
            "en",
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_any_permission_has_neither() {
        let user = make_authenticated_user(false, &[Permission::FileList]);
        let result = check_any_permission(
            &user,
            &[Permission::FileUpload, Permission::FileUploadAnywhere],
            "en",
        );
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind, ERROR_KIND_PERMISSION);
    }

    #[test]
    fn test_check_any_permission_empty_list_rejects_non_admin() {
        let user = make_authenticated_user(false, &[Permission::FileUpload]);
        assert!(check_any_permission(&user, &[], "en").is_err());
    }
}
