//! Authentication and handshake handling for file transfers
//!
//! Contains functions for handling the handshake, login, and initial
//! request phases of a transfer connection.

use std::collections::HashSet;
use std::io;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use nexus_common::framing::{FrameReader, FrameWriter};
use nexus_common::io::{read_client_message_with_full_timeout, send_server_message_with_id};
use nexus_common::protocol::{ClientMessage, ServerMessage};
use nexus_common::validators::{self, PasswordError, VersionError};
use nexus_common::version::{self, CompatibilityResult};

use crate::db::sql::GUEST_USERNAME;
use crate::db::{self, Database};
use crate::handlers::{
    err_account_disabled, err_authentication, err_database, err_guest_disabled,
    err_handshake_required, err_invalid_credentials, err_message_not_supported, err_not_logged_in,
    err_version_client_too_new, err_version_empty, err_version_invalid_semver,
    err_version_major_mismatch, err_version_too_long,
};

use super::helpers::{login_error_response, send_error_and_close};
use super::types::{AuthenticatedUser, DownloadParams, TransferRequest, UploadParams};

/// Handle the handshake phase for transfer connections
pub(crate) async fn handle_transfer_handshake<R, W>(
    frame_reader: &mut FrameReader<R>,
    frame_writer: &mut FrameWriter<W>,
    locale: &str,
) -> io::Result<()>
where
    R: AsyncReadExt + Unpin,
    W: AsyncWriteExt + Unpin,
{
    let server_version_str = nexus_common::PROTOCOL_VERSION;

    // Read handshake message (with idle timeout - no idle connections on transfer port)
    let received = match read_client_message_with_full_timeout(frame_reader, None, None).await {
        Ok(Some(msg)) => msg,
        Ok(None) => return Err(io::Error::other("Connection closed during handshake")),
        Err(e) => return Err(io::Error::other(format!("Failed to read handshake: {e}"))),
    };

    let version = match received.message {
        ClientMessage::Handshake { version } => version,
        _ => {
            let response = ServerMessage::HandshakeResponse {
                success: false,
                version: Some(server_version_str.to_string()),
                error: Some(err_handshake_required(locale)),
            };
            send_server_message_with_id(frame_writer, &response, received.message_id).await?;
            return Err(io::Error::other("Expected Handshake message"));
        }
    };

    // Validate version
    let client_version = match validators::validate_version(&version) {
        Ok(v) => v,
        Err(e) => {
            let error_msg = match e {
                VersionError::Empty => err_version_empty(locale),
                VersionError::TooLong => {
                    err_version_too_long(locale, validators::MAX_VERSION_LENGTH)
                }
                VersionError::InvalidSemver => err_version_invalid_semver(locale),
            };
            let response = ServerMessage::HandshakeResponse {
                success: false,
                version: Some(server_version_str.to_string()),
                error: Some(error_msg),
            };
            send_server_message_with_id(frame_writer, &response, received.message_id).await?;
            return Err(io::Error::other("Invalid version string"));
        }
    };

    // Check compatibility
    match version::check_compatibility(&client_version) {
        CompatibilityResult::Compatible => {
            let response = ServerMessage::HandshakeResponse {
                success: true,
                version: Some(server_version_str.to_string()),
                error: None,
            };
            send_server_message_with_id(frame_writer, &response, received.message_id).await?;
            Ok(())
        }
        CompatibilityResult::MajorMismatch {
            server_major,
            client_major,
        } => {
            let response = ServerMessage::HandshakeResponse {
                success: false,
                version: Some(server_version_str.to_string()),
                error: Some(err_version_major_mismatch(
                    locale,
                    server_major,
                    client_major,
                )),
            };
            send_server_message_with_id(frame_writer, &response, received.message_id).await?;
            Err(io::Error::other("Major version mismatch"))
        }
        CompatibilityResult::ClientTooNew {
            server_minor,
            client_minor: _,
        } => {
            let response = ServerMessage::HandshakeResponse {
                success: false,
                version: Some(server_version_str.to_string()),
                error: Some(err_version_client_too_new(
                    locale,
                    server_version_str,
                    &version,
                )),
            };
            send_server_message_with_id(frame_writer, &response, received.message_id).await?;
            Err(io::Error::other(format!(
                "Client version too new (server minor: {server_minor})"
            )))
        }
    }
}

/// Handle the login phase for transfer connections (simplified - just authentication)
pub(crate) async fn handle_transfer_login<R, W>(
    frame_reader: &mut FrameReader<R>,
    frame_writer: &mut FrameWriter<W>,
    db: &Database,
    locale: &mut String,
) -> io::Result<AuthenticatedUser>
where
    R: AsyncReadExt + Unpin,
    W: AsyncWriteExt + Unpin,
{
    // Read login message (with idle timeout - no idle connections on transfer port)
    let received = match read_client_message_with_full_timeout(frame_reader, None, None).await {
        Ok(Some(msg)) => msg,
        Ok(None) => return Err(io::Error::other("Connection closed during login")),
        Err(e) => return Err(io::Error::other(format!("Failed to read login: {e}"))),
    };

    let (raw_username, password, request_locale) = match received.message {
        ClientMessage::Login {
            username,
            password,
            locale: req_locale,
            ..
        } => (username, password, req_locale),
        _ => {
            let response = login_error_response(err_not_logged_in(locale));
            send_server_message_with_id(frame_writer, &response, received.message_id).await?;
            return Err(io::Error::other("Expected Login message"));
        }
    };

    // Update locale for subsequent error messages
    *locale = request_locale.clone();

    // Normalize empty username to "guest"
    let username = if raw_username.is_empty() {
        GUEST_USERNAME.to_string()
    } else {
        raw_username
    };

    // Validate username (skip for guest which was normalized from empty)
    if username.to_lowercase() != GUEST_USERNAME
        && let Err(_) = validators::validate_username(&username)
    {
        let response = login_error_response(err_invalid_credentials(locale));
        send_server_message_with_id(frame_writer, &response, received.message_id).await?;
        return Err(io::Error::other("Invalid username"));
    }

    // Validate password (use validate_password_input which allows empty for guest accounts)
    if let Err(PasswordError::TooLong) = validators::validate_password_input(&password) {
        let response = login_error_response(err_invalid_credentials(locale));
        send_server_message_with_id(frame_writer, &response, received.message_id).await?;
        return Err(io::Error::other("Invalid password"));
    }

    // Look up user
    let account = match db.users.get_user_by_username(&username).await {
        Ok(Some(acc)) => acc,
        Ok(None) => {
            let response = login_error_response(err_invalid_credentials(locale));
            send_server_message_with_id(frame_writer, &response, received.message_id).await?;
            return Err(io::Error::other("User not found"));
        }
        Err(e) => {
            let response = login_error_response(err_database(locale));
            send_server_message_with_id(frame_writer, &response, received.message_id).await?;
            return Err(io::Error::other(format!("Database error: {e}")));
        }
    };

    // Verify password
    let password_valid = if account.hashed_password.is_empty() {
        // Guest account - password must be empty
        password.is_empty()
    } else {
        match db::verify_password(&password, &account.hashed_password) {
            Ok(valid) => valid,
            Err(e) => {
                let response = login_error_response(err_authentication(locale));
                send_server_message_with_id(frame_writer, &response, received.message_id).await?;
                return Err(io::Error::other(format!(
                    "Password verification error: {e}"
                )));
            }
        }
    };

    if !password_valid {
        let response = login_error_response(err_invalid_credentials(locale));
        send_server_message_with_id(frame_writer, &response, received.message_id).await?;
        return Err(io::Error::other("Invalid credentials"));
    }

    // Check if account is enabled
    if !account.enabled {
        let error_msg = if username.to_lowercase() == GUEST_USERNAME {
            err_guest_disabled(locale)
        } else {
            err_account_disabled(locale, &username)
        };
        let response = login_error_response(error_msg);
        send_server_message_with_id(frame_writer, &response, received.message_id).await?;
        return Err(io::Error::other("Account disabled"));
    }

    // Get permissions
    let permissions = if account.is_admin {
        HashSet::new()
    } else {
        match db.users.get_user_permissions(account.id).await {
            Ok(perms) => perms.permissions,
            Err(_) => HashSet::new(),
        }
    };

    // Send simplified success response (no server_info, chat_info, etc.)
    let response = ServerMessage::LoginResponse {
        success: true,
        error: None,
        session_id: None,
        is_admin: None,
        permissions: None,
        server_info: None,
        chat_info: None,
        locale: None,
    };
    send_server_message_with_id(frame_writer, &response, received.message_id).await?;

    Ok(AuthenticatedUser {
        username: account.username,
        is_admin: account.is_admin,
        permissions,
    })
}

/// Handle transfer request (FileDownload or FileUpload)
pub(crate) async fn handle_transfer_request<R, W>(
    frame_reader: &mut FrameReader<R>,
    frame_writer: &mut FrameWriter<W>,
    locale: &str,
) -> io::Result<TransferRequest>
where
    R: AsyncReadExt + Unpin,
    W: AsyncWriteExt + Unpin,
{
    // With idle timeout - no idle connections on transfer port
    let received = match read_client_message_with_full_timeout(frame_reader, None, None).await {
        Ok(Some(msg)) => msg,
        Ok(None) => return Err(io::Error::other("Connection closed")),
        Err(e) => return Err(io::Error::other(format!("Failed to read message: {e}"))),
    };

    match received.message {
        ClientMessage::FileDownload { path, root } => {
            Ok(TransferRequest::Download(DownloadParams { path, root }))
        }
        ClientMessage::FileUpload {
            destination,
            file_count,
            total_size,
            root,
        } => Ok(TransferRequest::Upload(UploadParams {
            destination,
            file_count,
            total_size,
            root,
        })),
        _ => {
            send_error_and_close(frame_writer, &err_message_not_supported(locale)).await?;
            Err(io::Error::other(
                "Expected FileDownload or FileUpload message",
            ))
        }
    }
}
