//! Handshake, login, and initial-request phases for transfer connections.

use std::collections::HashSet;
use std::io;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use nexus_common::framing::{FrameReader, FrameWriter};
use nexus_common::io::{read_client_message_with_full_timeout, send_server_message_with_id};
use nexus_common::protocol::{ClientMessage, ServerMessage};
use nexus_common::validators::{self, PasswordError, VersionError};
use nexus_common::version::{self, CompatibilityResult};

use crate::constants::{
    ERR_TRANSFER_ACCOUNT_DISABLED, ERR_TRANSFER_CONNECTION_CLOSED, ERR_TRANSFER_DB_ERROR,
    ERR_TRANSFER_HANDSHAKE_CLOSED, ERR_TRANSFER_HANDSHAKE_EXPECTED,
    ERR_TRANSFER_INVALID_CREDENTIALS, ERR_TRANSFER_LOGIN_CLOSED, ERR_TRANSFER_LOGIN_EXPECTED,
    ERR_TRANSFER_PASSWORD_INVALID, ERR_TRANSFER_PASSWORD_VERIFY_ERROR, ERR_TRANSFER_READ_HANDSHAKE,
    ERR_TRANSFER_READ_LOGIN, ERR_TRANSFER_READ_MESSAGE, ERR_TRANSFER_USER_NOT_FOUND,
    ERR_TRANSFER_USERNAME_INVALID, ERR_TRANSFER_VERSION_CLIENT_TOO_NEW,
    ERR_TRANSFER_VERSION_INVALID, ERR_TRANSFER_VERSION_MAJOR_MISMATCH,
    ERR_TRANSFER_VERSION_MINOR_MISMATCH,
};
use crate::db::sql::GUEST_USERNAME;
use crate::db::{self, Database};
use crate::handlers::{
    err_account_disabled, err_authentication, err_database, err_guest_disabled,
    err_handshake_required, err_invalid_credentials, err_message_not_supported, err_not_logged_in,
    err_version_client_too_new, err_version_empty, err_version_invalid_semver,
    err_version_major_mismatch, err_version_minor_mismatch, err_version_too_long,
};

use super::helpers::{login_error_response, send_error_and_close};
use super::types::{AuthenticatedUser, DownloadParams, TransferRequest, UploadParams};

pub(crate) async fn handle_transfer_handshake<R, W>(
    frame_reader: &mut FrameReader<R>,
    frame_writer: &mut FrameWriter<W>,
    locale: &str,
    fingerprint: &'static str,
) -> io::Result<()>
where
    R: AsyncReadExt + Unpin,
    W: AsyncWriteExt + Unpin,
{
    let server_version_str = nexus_common::PROTOCOL_VERSION;

    // Idle timeout enforced — no idle connections allowed on the transfer port.
    let received = match read_client_message_with_full_timeout(frame_reader, None, None).await {
        Ok(Some(msg)) => msg,
        Ok(None) => return Err(io::Error::other(ERR_TRANSFER_HANDSHAKE_CLOSED)),
        Err(e) => {
            return Err(io::Error::other(format!(
                "{}{}",
                ERR_TRANSFER_READ_HANDSHAKE, e
            )));
        }
    };

    let version = match received.message {
        ClientMessage::Handshake { version } => version,
        _ => {
            let response = ServerMessage::HandshakeResponse {
                success: false,
                version: Some(server_version_str.to_string()),
                fingerprint: fingerprint.to_string(),
                error: Some(err_handshake_required(locale)),
            };
            send_server_message_with_id(frame_writer, &response, received.message_id).await?;
            return Err(io::Error::other(ERR_TRANSFER_HANDSHAKE_EXPECTED));
        }
    };

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
                fingerprint: fingerprint.to_string(),
                error: Some(error_msg),
            };
            send_server_message_with_id(frame_writer, &response, received.message_id).await?;
            return Err(io::Error::other(ERR_TRANSFER_VERSION_INVALID));
        }
    };

    match version::check_compatibility(&version::protocol_version(), &client_version) {
        CompatibilityResult::Compatible => {
            let response = ServerMessage::HandshakeResponse {
                success: true,
                version: Some(server_version_str.to_string()),
                fingerprint: fingerprint.to_string(),
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
                fingerprint: fingerprint.to_string(),
                error: Some(err_version_major_mismatch(
                    locale,
                    server_major,
                    client_major,
                )),
            };
            send_server_message_with_id(frame_writer, &response, received.message_id).await?;
            Err(io::Error::other(ERR_TRANSFER_VERSION_MAJOR_MISMATCH))
        }
        CompatibilityResult::MinorMismatch {
            server_minor,
            client_minor: _,
        } => {
            let response = ServerMessage::HandshakeResponse {
                success: false,
                version: Some(server_version_str.to_string()),
                fingerprint: fingerprint.to_string(),
                error: Some(err_version_minor_mismatch(
                    locale,
                    server_version_str,
                    &version,
                )),
            };
            send_server_message_with_id(frame_writer, &response, received.message_id).await?;
            Err(io::Error::other(format!(
                "{}{}",
                ERR_TRANSFER_VERSION_MINOR_MISMATCH, server_minor
            )))
        }
        CompatibilityResult::ClientTooNew {
            server_minor,
            client_minor: _,
        } => {
            let response = ServerMessage::HandshakeResponse {
                success: false,
                version: Some(server_version_str.to_string()),
                fingerprint: fingerprint.to_string(),
                error: Some(err_version_client_too_new(
                    locale,
                    server_version_str,
                    &version,
                )),
            };
            send_server_message_with_id(frame_writer, &response, received.message_id).await?;
            Err(io::Error::other(format!(
                "{}{}",
                ERR_TRANSFER_VERSION_CLIENT_TOO_NEW, server_minor
            )))
        }
    }
}

/// Transfer-port login: authentication only (no server_info, channels, etc.).
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
    // Idle timeout enforced — no idle connections allowed on the transfer port.
    let received = match read_client_message_with_full_timeout(frame_reader, None, None).await {
        Ok(Some(msg)) => msg,
        Ok(None) => return Err(io::Error::other(ERR_TRANSFER_LOGIN_CLOSED)),
        Err(e) => {
            return Err(io::Error::other(format!(
                "{}{}",
                ERR_TRANSFER_READ_LOGIN, e
            )));
        }
    };

    let (raw_username, password, request_locale, request_nickname) = match received.message {
        ClientMessage::Login {
            username,
            password,
            locale: req_locale,
            nickname,
            ..
        } => (username, password, req_locale, nickname),
        _ => {
            let response = login_error_response(err_not_logged_in(locale));
            send_server_message_with_id(frame_writer, &response, received.message_id).await?;
            return Err(io::Error::other(ERR_TRANSFER_LOGIN_EXPECTED));
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
        return Err(io::Error::other(ERR_TRANSFER_USERNAME_INVALID));
    }

    // validate_password_input allows empty for guest accounts.
    if let Err(PasswordError::TooLong) = validators::validate_password_input(&password) {
        let response = login_error_response(err_invalid_credentials(locale));
        send_server_message_with_id(frame_writer, &response, received.message_id).await?;
        return Err(io::Error::other(ERR_TRANSFER_PASSWORD_INVALID));
    }

    let account = match db.users.get_user_by_username(&username).await {
        Ok(Some(acc)) => acc,
        Ok(None) => {
            let response = login_error_response(err_invalid_credentials(locale));
            send_server_message_with_id(frame_writer, &response, received.message_id).await?;
            return Err(io::Error::other(ERR_TRANSFER_USER_NOT_FOUND));
        }
        Err(e) => {
            let response = login_error_response(err_database(locale));
            send_server_message_with_id(frame_writer, &response, received.message_id).await?;
            return Err(io::Error::other(format!("{}{}", ERR_TRANSFER_DB_ERROR, e)));
        }
    };

    let password_valid = if account.hashed_password.is_empty() {
        // Guest account — password must be empty.
        password.is_empty()
    } else {
        match db::verify_password_async(password.clone(), account.hashed_password.clone()).await {
            Ok(valid) => valid,
            Err(e) => {
                let response = login_error_response(err_authentication(locale));
                send_server_message_with_id(frame_writer, &response, received.message_id).await?;
                return Err(io::Error::other(format!(
                    "{}{}",
                    ERR_TRANSFER_PASSWORD_VERIFY_ERROR, e
                )));
            }
        }
    };

    if !password_valid {
        let response = login_error_response(err_invalid_credentials(locale));
        send_server_message_with_id(frame_writer, &response, received.message_id).await?;
        return Err(io::Error::other(ERR_TRANSFER_INVALID_CREDENTIALS));
    }

    if !account.enabled {
        let error_msg = if username.to_lowercase() == GUEST_USERNAME {
            err_guest_disabled(locale)
        } else {
            err_account_disabled(locale, &username)
        };
        let response = login_error_response(error_msg);
        send_server_message_with_id(frame_writer, &response, received.message_id).await?;
        return Err(io::Error::other(ERR_TRANSFER_ACCOUNT_DISABLED));
    }

    let permissions = if account.is_admin {
        HashSet::new()
    } else {
        match db.users.get_user_permissions(account.id).await {
            Ok(perms) => perms.permissions,
            Err(_) => HashSet::new(),
        }
    };

    // Shared accounts use the requested nickname; otherwise it equals username.
    let nickname = if account.is_shared {
        request_nickname.unwrap_or_else(|| account.username.clone())
    } else {
        account.username.clone()
    };

    let response = ServerMessage::LoginResponse {
        success: true,
        error: None,
        session_id: None,
        user_id: None,
        is_admin: None,
        permissions: None,
        server_info: None,
        channels: None,
        locale: None,
        nickname: None,
        group_id: None,
        group_name: None,
    };
    send_server_message_with_id(frame_writer, &response, received.message_id).await?;

    Ok(AuthenticatedUser {
        nickname,
        username: account.username,
        is_admin: account.is_admin,
        is_shared: account.is_shared,
        permissions,
    })
}

/// Read the initial transfer request (FileDownload or FileUpload).
pub(crate) async fn handle_transfer_request<R, W>(
    frame_reader: &mut FrameReader<R>,
    frame_writer: &mut FrameWriter<W>,
    locale: &str,
) -> io::Result<TransferRequest>
where
    R: AsyncReadExt + Unpin,
    W: AsyncWriteExt + Unpin,
{
    // Idle timeout enforced — no idle connections allowed on the transfer port.
    let received = match read_client_message_with_full_timeout(frame_reader, None, None).await {
        Ok(Some(msg)) => msg,
        Ok(None) => return Err(io::Error::other(ERR_TRANSFER_CONNECTION_CLOSED)),
        Err(e) => {
            return Err(io::Error::other(format!(
                "{}{}",
                ERR_TRANSFER_READ_MESSAGE, e
            )));
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    use tokio::io::BufReader;
    use tokio::net::{TcpListener, TcpStream};

    use nexus_common::PROTOCOL_VERSION;
    use nexus_common::io::{read_server_message, send_client_message};

    use crate::handlers::testing::{DEFAULT_TEST_LOCALE, TEST_FINGERPRINT};

    /// Set up a TCP socket pair and run `handle_transfer_handshake` server-side
    /// while sending `client_handshake_version` from the client side. Returns
    /// the parsed `HandshakeResponse` fields plus the server-side result.
    async fn run_handshake(
        client_handshake_version: &str,
    ) -> (io::Result<()>, bool, Option<String>, String, Option<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let client_handle = tokio::spawn(async move { TcpStream::connect(addr).await.unwrap() });
        let (server_stream, _) = listener.accept().await.unwrap();
        let client_stream = client_handle.await.unwrap();

        let (server_read, server_write) = server_stream.into_split();
        let mut server_frame_reader = FrameReader::new(BufReader::new(server_read));
        let mut server_frame_writer = FrameWriter::new(server_write);

        let (client_read, client_write) = client_stream.into_split();
        let mut client_frame_writer = FrameWriter::new(client_write);
        let mut client_frame_reader = FrameReader::new(BufReader::new(client_read));

        let server_task = tokio::spawn(async move {
            let result = handle_transfer_handshake(
                &mut server_frame_reader,
                &mut server_frame_writer,
                DEFAULT_TEST_LOCALE,
                TEST_FINGERPRINT,
            )
            .await;
            (result, server_frame_writer)
        });

        let handshake = ClientMessage::Handshake {
            version: client_handshake_version.to_string(),
        };
        send_client_message(&mut client_frame_writer, &handshake)
            .await
            .unwrap();

        let received = read_server_message(&mut client_frame_reader)
            .await
            .unwrap()
            .expect("Server should send a HandshakeResponse before closing");

        let (server_result, _writer) = server_task.await.unwrap();

        match received.message {
            ServerMessage::HandshakeResponse {
                success,
                version,
                fingerprint,
                error,
            } => (server_result, success, version, fingerprint, error),
            other => panic!("Expected HandshakeResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_transfer_handshake_success_includes_fingerprint() {
        let (result, success, version, fingerprint, error) = run_handshake(PROTOCOL_VERSION).await;

        assert!(
            result.is_ok(),
            "Handshake should succeed with matching version"
        );
        assert!(success, "Response should indicate success");
        assert_eq!(version, Some(PROTOCOL_VERSION.to_string()));
        assert_eq!(fingerprint, TEST_FINGERPRINT);
        assert!(error.is_none(), "Error should be None on success");
    }

    #[tokio::test]
    async fn test_transfer_handshake_invalid_semver_includes_fingerprint() {
        let (result, success, _version, fingerprint, error) = run_handshake("not-valid").await;

        assert!(result.is_err(), "Handshake should fail with invalid semver");
        assert!(!success, "Response should indicate failure");
        assert_eq!(
            fingerprint, TEST_FINGERPRINT,
            "Fingerprint must be sent even on failure responses"
        );
        assert!(error.is_some(), "Should have error message");
    }

    #[tokio::test]
    async fn test_transfer_handshake_major_mismatch_includes_fingerprint() {
        let server_ver = nexus_common::version::protocol_version();
        let client_version = format!("{}.0.0", server_ver.major + 1);
        let (result, success, _version, fingerprint, error) = run_handshake(&client_version).await;

        assert!(result.is_err(), "Handshake should fail with major mismatch");
        assert!(!success, "Response should indicate failure");
        assert_eq!(
            fingerprint, TEST_FINGERPRINT,
            "Fingerprint must be sent even on failure responses"
        );
        assert!(error.is_some(), "Should have error message");
    }
}
