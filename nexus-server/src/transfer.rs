//! Transfer connection handler for file downloads (port 7501)
//!
//! This module handles file transfer connections on a separate port from the main
//! BBS protocol. The transfer protocol uses the same TLS certificate and framing
//! format, but with a simplified flow:
//!
//! 1. Client: Handshake → Server: HandshakeResponse
//! 2. Client: Login → Server: LoginResponse (simplified: just success/error)
//! 3. Client: FileDownload → Server: FileDownloadResponse
//! 4. For each file: Server: FileStart → Client: FileStartResponse → Server: FileData
//! 5. Server: TransferComplete
//! 6. Server closes connection

use std::io;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, BufReader, SeekFrom};
use tokio::net::TcpStream;
use tokio_rustls::TlsAcceptor;

use nexus_common::framing::{FrameReader, FrameWriter, MessageId};
use nexus_common::io::{read_client_message_with_full_timeout, send_server_message_with_id};
use nexus_common::protocol::{ClientMessage, ServerMessage};
use nexus_common::validators::{self, FilePathError, PasswordError, UsernameError, VersionError};
use nexus_common::version::{self, CompatibilityResult};

use crate::constants::{DEFAULT_LOCALE, HASH_BUFFER_SIZE};
use crate::db::sql::GUEST_USERNAME;
use crate::db::{self, Database, Permission};
use crate::files::area::resolve_user_area;
use crate::files::folder_type::{FolderType, parse_folder_type};
use crate::files::path::{build_and_validate_candidate_path, resolve_path};
use crate::handlers::{
    err_account_disabled, err_authentication, err_database, err_expected_file_download,
    err_file_area_not_accessible, err_file_area_not_configured, err_guest_disabled,
    err_handshake_required, err_invalid_credentials, err_not_logged_in, err_permission_denied,
    err_transfer_access_denied, err_transfer_file_failed, err_transfer_path_invalid,
    err_transfer_path_not_found, err_transfer_path_too_long, err_transfer_read_failed,
    err_version_client_too_new, err_version_empty, err_version_invalid_semver,
    err_version_major_mismatch, err_version_too_long,
};

/// Parameters for handling a transfer connection
pub struct TransferParams {
    pub peer_addr: SocketAddr,
    pub db: Database,
    pub debug: bool,
    pub file_root: Option<&'static Path>,
}

/// Information about a file to transfer
struct FileInfo {
    /// Relative path from download root (e.g., "Games/app.zip")
    relative_path: String,
    /// Absolute filesystem path
    absolute_path: PathBuf,
    /// File size in bytes
    size: u64,
}

/// Authenticated user information (minimal for transfer port)
struct AuthenticatedUser {
    username: String,
    is_admin: bool,
    permissions: std::collections::HashSet<Permission>,
}

/// Handle a transfer connection (file downloads on port 7501)
pub async fn handle_transfer_connection(
    socket: TcpStream,
    tls_acceptor: TlsAcceptor,
    params: TransferParams,
) -> io::Result<()> {
    let TransferParams {
        peer_addr,
        db,
        debug,
        file_root,
    } = params;

    // Perform TLS handshake (mandatory, same cert as main port)
    let tls_stream = tls_acceptor
        .accept(socket)
        .await
        .map_err(|e| io::Error::other(format!("TLS handshake failed: {e}")))?;

    if debug {
        eprintln!("Transfer connection from {peer_addr}");
    }

    // Set up framed I/O
    let (reader, writer) = tokio::io::split(tls_stream);
    let buf_reader = BufReader::new(reader);
    let mut frame_reader = FrameReader::new(buf_reader);
    let mut frame_writer = FrameWriter::new(writer);

    // Default locale for error messages before login
    let mut locale = DEFAULT_LOCALE.to_string();

    // Phase 1: Handshake
    let handshake_result =
        handle_transfer_handshake(&mut frame_reader, &mut frame_writer, &locale).await;
    if let Err(e) = handshake_result {
        if debug {
            eprintln!("Transfer handshake failed from {peer_addr}: {e}");
        }
        let _ = frame_writer.get_mut().shutdown().await;
        return Ok(());
    }

    // Phase 2: Login (simplified - just authentication)
    let user =
        match handle_transfer_login(&mut frame_reader, &mut frame_writer, &db, &mut locale).await {
            Ok(user) => user,
            Err(e) => {
                if debug {
                    eprintln!("Transfer login failed from {peer_addr}: {e}");
                }
                let _ = frame_writer.get_mut().shutdown().await;
                return Ok(());
            }
        };

    if debug {
        eprintln!("Transfer authenticated: {} from {peer_addr}", user.username);
    }

    // Phase 3: FileDownload request
    let Some(file_root) = file_root else {
        // File area not configured
        send_transfer_error(
            &mut frame_writer,
            &err_file_area_not_configured(&locale),
            Some("not_found"),
            "FileDownload",
        )
        .await?;
        let _ = frame_writer.get_mut().shutdown().await;
        return Ok(());
    };

    let (download_path, use_root) =
        match handle_file_download_request(&mut frame_reader, &mut frame_writer, &locale).await {
            Ok(req) => req,
            Err(e) => {
                if debug {
                    eprintln!("FileDownload request failed from {peer_addr}: {e}");
                }
                let _ = frame_writer.get_mut().shutdown().await;
                return Ok(());
            }
        };

    // Validate path using shared validator
    if let Err(e) = validators::validate_file_path(&download_path) {
        let error_msg = match e {
            FilePathError::TooLong => err_transfer_path_too_long(&locale),
            FilePathError::ContainsNull
            | FilePathError::InvalidCharacters
            | FilePathError::ContainsWindowsDrive => err_transfer_path_invalid(&locale),
        };
        send_transfer_error(
            &mut frame_writer,
            &error_msg,
            Some("invalid"),
            "FileDownload",
        )
        .await?;
        let _ = frame_writer.get_mut().shutdown().await;
        return Ok(());
    }

    // Check download permission
    if !user.is_admin && !user.permissions.contains(&Permission::FileDownload) {
        send_transfer_error(
            &mut frame_writer,
            &err_permission_denied(&locale),
            Some("permission"),
            "FileDownload",
        )
        .await?;
        let _ = frame_writer.get_mut().shutdown().await;
        return Ok(());
    }

    // Check file_root permission if using root mode
    if use_root && !user.is_admin && !user.permissions.contains(&Permission::FileRoot) {
        send_transfer_error(
            &mut frame_writer,
            &err_permission_denied(&locale),
            Some("permission"),
            "FileDownload",
        )
        .await?;
        let _ = frame_writer.get_mut().shutdown().await;
        return Ok(());
    }

    // Resolve area root
    let area_root = if use_root {
        file_root.to_path_buf()
    } else {
        resolve_user_area(file_root, &user.username)
    };

    // Canonicalize area root
    let area_root = match std::fs::canonicalize(&area_root) {
        Ok(p) => p,
        Err(_) => {
            send_transfer_error(
                &mut frame_writer,
                &err_file_area_not_accessible(&locale),
                Some("not_found"),
                "FileDownload",
            )
            .await?;
            let _ = frame_writer.get_mut().shutdown().await;
            return Ok(());
        }
    };

    // Resolve the download path
    let candidate = match build_and_validate_candidate_path(&area_root, &download_path) {
        Ok(p) => p,
        Err(_) => {
            send_transfer_error(
                &mut frame_writer,
                &err_transfer_path_invalid(&locale),
                Some("invalid"),
                "FileDownload",
            )
            .await?;
            let _ = frame_writer.get_mut().shutdown().await;
            return Ok(());
        }
    };

    let resolved_path = match resolve_path(&area_root, &candidate) {
        Ok(p) => p,
        Err(e) => {
            let (error_msg, error_kind) = match e {
                crate::files::path::PathError::NotFound => {
                    (err_transfer_path_not_found(&locale), "not_found")
                }
                crate::files::path::PathError::AccessDenied => {
                    (err_transfer_access_denied(&locale), "permission")
                }
                _ => (err_transfer_path_invalid(&locale), "invalid"),
            };
            send_transfer_error(
                &mut frame_writer,
                &error_msg,
                Some(error_kind),
                "FileDownload",
            )
            .await?;
            let _ = frame_writer.get_mut().shutdown().await;
            return Ok(());
        }
    };

    // Check dropbox access
    if !can_access_for_download(&resolved_path, &user.username, user.is_admin) {
        send_transfer_error(
            &mut frame_writer,
            &err_transfer_access_denied(&locale),
            Some("permission"),
            "FileDownload",
        )
        .await?;
        let _ = frame_writer.get_mut().shutdown().await;
        return Ok(());
    }

    // Scan files to transfer
    let files =
        match scan_files_for_transfer(&resolved_path, &user.username, user.is_admin, debug).await {
            Ok(files) => files,
            Err(e) => {
                if debug {
                    eprintln!("Failed to scan files: {e}");
                }
                send_transfer_error(
                    &mut frame_writer,
                    &err_transfer_read_failed(&locale),
                    Some("io_error"),
                    "FileDownload",
                )
                .await?;
                let _ = frame_writer.get_mut().shutdown().await;
                return Ok(());
            }
        };

    // Calculate total size
    let total_size: u64 = files.iter().map(|f| f.size).sum();
    let file_count = files.len() as u64;

    // Generate transfer ID for logging
    let transfer_id = generate_transfer_id();

    if debug {
        eprintln!(
            "Transfer {transfer_id}: {} files, {} bytes from {}",
            file_count, total_size, peer_addr
        );
    }

    // Send FileDownloadResponse
    let response = ServerMessage::FileDownloadResponse {
        success: true,
        error: None,
        error_kind: None,
        size: Some(total_size),
        file_count: Some(file_count),
        transfer_id: Some(transfer_id.clone()),
    };
    send_server_message_with_id(&mut frame_writer, &response, MessageId::new()).await?;

    // Stream each file
    let mut transfer_success = true;
    let mut transfer_error: Option<String> = None;
    let mut transfer_error_kind: Option<String> = None;

    for file_info in &files {
        match stream_file(
            &mut frame_reader,
            &mut frame_writer,
            file_info,
            debug,
            &transfer_id,
        )
        .await
        {
            Ok(()) => {}
            Err(e) => {
                if debug {
                    eprintln!(
                        "Transfer {transfer_id}: Error streaming {}: {e}",
                        file_info.relative_path
                    );
                }
                transfer_success = false;
                transfer_error = Some(err_transfer_file_failed(
                    &locale,
                    &file_info.relative_path,
                    &e.to_string(),
                ));
                transfer_error_kind = Some("io_error".to_string());
                break;
            }
        }
    }

    // Send TransferComplete
    let complete = ServerMessage::TransferComplete {
        success: transfer_success,
        error: transfer_error,
        error_kind: transfer_error_kind,
    };
    send_server_message_with_id(&mut frame_writer, &complete, MessageId::new()).await?;

    if debug {
        if transfer_success {
            eprintln!("Transfer {transfer_id}: Complete");
        } else {
            eprintln!("Transfer {transfer_id}: Failed");
        }
    }

    // Close connection
    let _ = frame_writer.get_mut().shutdown().await;

    Ok(())
}

/// Handle the handshake phase for transfer connections
async fn handle_transfer_handshake<R, W>(
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
async fn handle_transfer_login<R, W>(
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
            let response = ServerMessage::LoginResponse {
                success: false,
                error: Some(err_not_logged_in(locale)),
                session_id: None,
                is_admin: None,
                permissions: None,
                server_info: None,
                chat_info: None,
                locale: None,
            };
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
        && let Err(e) = validators::validate_username(&username)
    {
        let error_msg = match e {
            UsernameError::Empty | UsernameError::TooLong | UsernameError::InvalidCharacters => {
                err_invalid_credentials(locale)
            }
        };
        let response = ServerMessage::LoginResponse {
            success: false,
            error: Some(error_msg),
            session_id: None,
            is_admin: None,
            permissions: None,
            server_info: None,
            chat_info: None,
            locale: None,
        };
        send_server_message_with_id(frame_writer, &response, received.message_id).await?;
        return Err(io::Error::other("Invalid username"));
    }

    // Validate password (use validate_password_input which allows empty for guest accounts)
    if let Err(e) = validators::validate_password_input(&password) {
        let error_msg = match e {
            PasswordError::Empty => err_invalid_credentials(locale), // Should never happen with validate_password_input
            PasswordError::TooLong => err_invalid_credentials(locale),
        };
        let response = ServerMessage::LoginResponse {
            success: false,
            error: Some(error_msg),
            session_id: None,
            is_admin: None,
            permissions: None,
            server_info: None,
            chat_info: None,
            locale: None,
        };
        send_server_message_with_id(frame_writer, &response, received.message_id).await?;
        return Err(io::Error::other("Invalid password"));
    }

    // Look up user
    let account = match db.users.get_user_by_username(&username).await {
        Ok(Some(acc)) => acc,
        Ok(None) => {
            let response = ServerMessage::LoginResponse {
                success: false,
                error: Some(err_invalid_credentials(locale)),
                session_id: None,
                is_admin: None,
                permissions: None,
                server_info: None,
                chat_info: None,
                locale: None,
            };
            send_server_message_with_id(frame_writer, &response, received.message_id).await?;
            return Err(io::Error::other("User not found"));
        }
        Err(e) => {
            let response = ServerMessage::LoginResponse {
                success: false,
                error: Some(err_database(locale)),
                session_id: None,
                is_admin: None,
                permissions: None,
                server_info: None,
                chat_info: None,
                locale: None,
            };
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
                let response = ServerMessage::LoginResponse {
                    success: false,
                    error: Some(err_authentication(locale)),
                    session_id: None,
                    is_admin: None,
                    permissions: None,
                    server_info: None,
                    chat_info: None,
                    locale: None,
                };
                send_server_message_with_id(frame_writer, &response, received.message_id).await?;
                return Err(io::Error::other(format!(
                    "Password verification error: {e}"
                )));
            }
        }
    };

    if !password_valid {
        let response = ServerMessage::LoginResponse {
            success: false,
            error: Some(err_invalid_credentials(locale)),
            session_id: None,
            is_admin: None,
            permissions: None,
            server_info: None,
            chat_info: None,
            locale: None,
        };
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
        let response = ServerMessage::LoginResponse {
            success: false,
            error: Some(error_msg),
            session_id: None,
            is_admin: None,
            permissions: None,
            server_info: None,
            chat_info: None,
            locale: None,
        };
        send_server_message_with_id(frame_writer, &response, received.message_id).await?;
        return Err(io::Error::other("Account disabled"));
    }

    // Get permissions
    let permissions = if account.is_admin {
        std::collections::HashSet::new()
    } else {
        match db.users.get_user_permissions(account.id).await {
            Ok(perms) => perms.permissions,
            Err(_) => std::collections::HashSet::new(),
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

/// Handle FileDownload request
async fn handle_file_download_request<R, W>(
    frame_reader: &mut FrameReader<R>,
    frame_writer: &mut FrameWriter<W>,
    locale: &str,
) -> io::Result<(String, bool)>
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
        ClientMessage::FileDownload { path, root } => Ok((path, root)),
        _ => {
            send_transfer_error(
                frame_writer,
                &err_expected_file_download(locale),
                Some("protocol_error"),
                "FileDownload",
            )
            .await?;
            Err(io::Error::other("Expected FileDownload message"))
        }
    }
}

/// Send a transfer error response
async fn send_transfer_error<W>(
    frame_writer: &mut FrameWriter<W>,
    error: &str,
    error_kind: Option<&str>,
    command: &str,
) -> io::Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    // For FileDownload errors, use FileDownloadResponse
    if command == "FileDownload" {
        let response = ServerMessage::FileDownloadResponse {
            success: false,
            error: Some(error.to_string()),
            error_kind: error_kind.map(String::from),
            size: None,
            file_count: None,
            transfer_id: None,
        };
        send_server_message_with_id(frame_writer, &response, MessageId::new()).await
    } else {
        // For other errors, use generic Error message
        let response = ServerMessage::Error {
            message: error.to_string(),
            command: Some(command.to_string()),
        };
        send_server_message_with_id(frame_writer, &response, MessageId::new()).await
    }
}

/// Check if a path can be accessed for download (dropbox restrictions)
fn can_access_for_download(path: &Path, username: &str, is_admin: bool) -> bool {
    // Check each component of the path for dropbox folders
    for ancestor in path.ancestors() {
        if let Some(name) = ancestor.file_name().and_then(|n| n.to_str()) {
            match parse_folder_type(name) {
                FolderType::DropBox => {
                    // Only admins can download from generic dropboxes
                    if !is_admin {
                        return false;
                    }
                }
                FolderType::UserDropBox(owner) => {
                    // Only the owner or admins can download from user dropboxes
                    if !is_admin && owner.to_lowercase() != username.to_lowercase() {
                        return false;
                    }
                }
                FolderType::Default | FolderType::Upload => {
                    // Default and upload folders allow downloads
                }
            }
        }
    }
    true
}

/// Scan files to transfer from a path (file or directory)
async fn scan_files_for_transfer(
    resolved_path: &Path,
    username: &str,
    is_admin: bool,
    debug: bool,
) -> io::Result<Vec<FileInfo>> {
    let mut files = Vec::new();

    let metadata = tokio::fs::metadata(resolved_path).await?;

    if metadata.is_file() {
        // Single file download
        let file_name = resolved_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file");

        // Use just the filename for single file downloads
        files.push(FileInfo {
            relative_path: file_name.to_string(),
            absolute_path: resolved_path.to_path_buf(),
            size: metadata.len(),
        });
    } else if metadata.is_dir() {
        // Directory download - recursively scan
        // Use empty prefix because the client already includes the directory name in local_path.
        // Files will have paths relative to inside the directory (e.g., "song.mp3", "Jazz/tune.mp3")
        // rather than including the directory name (e.g., "Music/song.mp3", "Music/Jazz/tune.mp3").
        scan_directory_recursive(resolved_path, "", &mut files, username, is_admin, debug).await?;
    }

    Ok(files)
}

/// Recursively scan a directory for files
///
/// Filters out files in dropbox folders that the user doesn't have access to.
/// This prevents information leakage when downloading a parent directory that
/// contains dropbox subfolders.
fn scan_directory_recursive<'a>(
    dir: &'a Path,
    prefix: &'a str,
    files: &'a mut Vec<FileInfo>,
    username: &'a str,
    is_admin: bool,
    debug: bool,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = io::Result<()>> + Send + 'a>> {
    Box::pin(async move {
        if debug {
            eprintln!("Scanning directory: {:?} (prefix: {:?})", dir, prefix);
        }

        let mut entries = tokio::fs::read_dir(dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if debug {
                eprintln!("  Processing entry: {:?}", path);
            }
            // Use tokio::fs::metadata instead of entry.metadata() to follow symlinks.
            // entry.metadata() uses lstat which returns symlink metadata, not target metadata.
            let metadata = match tokio::fs::metadata(&path).await {
                Ok(m) => m,
                Err(e) => {
                    if debug {
                        eprintln!("  Skipping {:?} - metadata failed: {}", path, e);
                    }
                    continue;
                }
            };
            // Skip files with non-UTF-8 names
            let Some(file_name) = entry.file_name().to_str().map(|s| s.to_string()) else {
                if debug {
                    eprintln!("  Skipping non-UTF-8 filename: {:?}", entry.file_name());
                }
                continue;
            };

            // Note: Hidden files (dotfiles) are included in downloads.
            // The show_hidden setting only affects the file browser UI, not transfers.

            // Canonicalize the path before dropbox check to handle symlinks correctly.
            // If a symlink points to a dropbox folder, the canonical path will reveal it.
            // Skip broken symlinks (canonicalize fails) silently.
            let canonical_path = match std::fs::canonicalize(&path) {
                Ok(p) => p,
                Err(e) => {
                    if debug {
                        eprintln!("  Skipping {} - canonicalize failed: {}", file_name, e);
                    }
                    continue;
                }
            };

            // Check dropbox access on canonical path (security: prevents leaking dropbox contents
            // even through symlinks that point to dropbox folders)
            if !can_access_for_download(&canonical_path, username, is_admin) {
                if debug {
                    eprintln!("  Skipping {} - dropbox access denied", file_name);
                }
                continue;
            }

            // Build relative path, handling empty prefix for top-level files
            let relative = if prefix.is_empty() {
                file_name.clone()
            } else {
                format!("{}/{}", prefix, file_name)
            };

            if metadata.is_file() {
                if debug {
                    eprintln!("  Adding file: {} (size: {})", relative, metadata.len());
                }
                files.push(FileInfo {
                    relative_path: relative,
                    absolute_path: path,
                    size: metadata.len(),
                });
            } else if metadata.is_dir() {
                if debug {
                    eprintln!("  Recursing into directory: {}", relative);
                }
                // For subdirectories, use the relative path as the new prefix
                scan_directory_recursive(&path, &relative, files, username, is_admin, debug)
                    .await?;
            } else if debug {
                eprintln!(
                    "  Skipping {} - neither file nor directory (is_symlink: {})",
                    file_name,
                    metadata.is_symlink()
                );
            }
        }

        if debug {
            eprintln!(
                "Done scanning directory: {:?} (found {} files so far)",
                dir,
                files.len()
            );
        }

        Ok(())
    })
}

/// Stream a single file to the client
async fn stream_file<R, W>(
    frame_reader: &mut FrameReader<R>,
    frame_writer: &mut FrameWriter<W>,
    file_info: &FileInfo,
    debug: bool,
    transfer_id: &str,
) -> io::Result<()>
where
    R: AsyncReadExt + Unpin,
    W: AsyncWriteExt + Unpin,
{
    // Re-canonicalize to get the current real path (handles symlinks)
    // Note: Admin-created symlinks pointing outside the file area are allowed
    // (e.g., shared/Videos -> /mnt/nas/videos). Users cannot create symlinks
    // through the BBS protocol, so all symlinks are trusted.
    let canonical_path = std::fs::canonicalize(&file_info.absolute_path)?;

    // Compute SHA-256 of the file
    let sha256 = compute_file_sha256(&canonical_path).await?;

    // Send FileStart
    let file_start = ServerMessage::FileStart {
        path: file_info.relative_path.clone(),
        size: file_info.size,
        sha256: sha256.clone(),
    };
    let file_start_id = MessageId::new();
    send_server_message_with_id(frame_writer, &file_start, file_start_id).await?;

    // Read FileStartResponse to determine resume offset
    let offset =
        read_file_start_response(frame_reader, &sha256, file_info.size, &canonical_path).await?;

    if debug {
        if offset > 0 {
            eprintln!(
                "Transfer {transfer_id}: Resuming {} from offset {} ({}%)",
                file_info.relative_path,
                offset,
                (offset * 100) / file_info.size.max(1)
            );
        } else if file_info.size > 0 {
            eprintln!(
                "Transfer {transfer_id}: Sending {} ({} bytes)",
                file_info.relative_path, file_info.size
            );
        }
    }

    // If offset equals file size, file is already complete - skip streaming
    if offset >= file_info.size {
        if debug && file_info.size > 0 {
            eprintln!(
                "Transfer {transfer_id}: {} already complete",
                file_info.relative_path
            );
        }
        return Ok(());
    }

    // Calculate bytes to send
    let bytes_to_send = file_info.size - offset;

    // Open file and seek to offset (use canonical path for safety)
    let file = File::open(&canonical_path).await?;
    let mut reader = BufReader::new(file);
    if offset > 0 {
        reader.seek(SeekFrom::Start(offset)).await?;
    }

    // Stream file data using the framing helper
    frame_writer
        .write_streaming_frame(MessageId::new(), "FileData", &mut reader, bytes_to_send)
        .await
        .map_err(|e| io::Error::other(format!("Failed to stream file: {e}")))?;

    Ok(())
}

/// Read FileStartResponse and calculate resume offset
///
/// Verifies that the client's reported partial file hash matches the hash of
/// the first N bytes of the server's file before allowing resume.
async fn read_file_start_response<R>(
    frame_reader: &mut FrameReader<R>,
    server_sha256: &str,
    server_size: u64,
    file_path: &Path,
) -> io::Result<u64>
where
    R: AsyncReadExt + Unpin,
{
    // With idle timeout - client must respond promptly to FileStart
    let received = match read_client_message_with_full_timeout(frame_reader, None, None).await {
        Ok(Some(msg)) => msg,
        Ok(None) => {
            return Err(io::Error::other(
                "Connection closed waiting for FileStartResponse",
            ));
        }
        Err(e) => {
            return Err(io::Error::other(format!(
                "Failed to read FileStartResponse: {e}"
            )));
        }
    };

    match received.message {
        ClientMessage::FileStartResponse { size, sha256 } => {
            // If client has no local file, start from beginning
            if size == 0 {
                return Ok(0);
            }

            // If client reports size > server size, start from beginning
            if size > server_size {
                return Ok(0);
            }

            // Client must provide hash for resume
            let Some(client_hash) = sha256 else {
                // No hash provided - start from beginning
                return Ok(0);
            };

            // If sizes match, verify against complete file hash
            if size == server_size {
                if client_hash == server_sha256 {
                    // File is already complete
                    return Ok(server_size);
                }
                // Hash mismatch - start from beginning
                return Ok(0);
            }

            // Client has partial file - verify hash of first N bytes
            let partial_hash = compute_partial_sha256(file_path, size).await?;
            if client_hash == partial_hash {
                // Hash matches - resume from client's position
                Ok(size)
            } else {
                // Hash mismatch - start from beginning
                Ok(0)
            }
        }
        _ => Err(io::Error::other("Expected FileStartResponse message")),
    }
}

/// Compute SHA-256 hash of an entire file
async fn compute_file_sha256(path: &Path) -> io::Result<String> {
    compute_partial_sha256(path, u64::MAX).await
}

/// Compute SHA-256 hash of the first `max_bytes` of a file
///
/// If the file is smaller than `max_bytes`, hashes the entire file.
async fn compute_partial_sha256(path: &Path, max_bytes: u64) -> io::Result<String> {
    let mut file = File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; HASH_BUFFER_SIZE];
    let mut remaining = max_bytes;

    while remaining > 0 {
        let to_read = (remaining as usize).min(buffer.len());
        let bytes_read = file.read(&mut buffer[..to_read]).await?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
        remaining -= bytes_read as u64;
    }

    let hash = hasher.finalize();

    Ok(hex::encode(hash))
}

/// Generate a random transfer ID (8 hex chars, 32 bits)
///
/// This uses thread_rng() which is sufficient for log correlation
/// but is NOT cryptographically secure. Do not use for security-sensitive
/// purposes like authentication tokens.
fn generate_transfer_id() -> String {
    use rand::Rng;
    let bytes: [u8; 4] = rand::rng().random();
    hex::encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use tempfile::TempDir;
    use tokio::fs;

    // ==========================================================================
    // can_access_for_download tests
    // ==========================================================================

    #[test]
    fn test_can_access_default_folder() {
        let path = Path::new("/files/shared/Documents/readme.txt");
        assert!(can_access_for_download(path, "alice", false));
        assert!(can_access_for_download(path, "bob", false));
        assert!(can_access_for_download(path, "admin", true));
    }

    #[test]
    fn test_can_access_upload_folder() {
        let path = Path::new("/files/shared/Uploads [NEXUS-UL]/file.zip");
        assert!(can_access_for_download(path, "alice", false));
        assert!(can_access_for_download(path, "bob", false));
        assert!(can_access_for_download(path, "admin", true));
    }

    #[test]
    fn test_cannot_access_dropbox_non_admin() {
        let path = Path::new("/files/shared/Submissions [NEXUS-DB]/secret.txt");
        assert!(!can_access_for_download(path, "alice", false));
        assert!(!can_access_for_download(path, "bob", false));
    }

    #[test]
    fn test_admin_can_access_dropbox() {
        let path = Path::new("/files/shared/Submissions [NEXUS-DB]/secret.txt");
        assert!(can_access_for_download(path, "admin", true));
    }

    #[test]
    fn test_user_can_access_own_user_dropbox() {
        let path = Path::new("/files/shared/For Alice [NEXUS-DB-alice]/file.txt");
        assert!(can_access_for_download(path, "alice", false));
        assert!(can_access_for_download(path, "ALICE", false)); // case insensitive
    }

    #[test]
    fn test_user_cannot_access_other_user_dropbox() {
        let path = Path::new("/files/shared/For Alice [NEXUS-DB-alice]/file.txt");
        assert!(!can_access_for_download(path, "bob", false));
        assert!(!can_access_for_download(path, "charlie", false));
    }

    #[test]
    fn test_admin_can_access_any_user_dropbox() {
        let path = Path::new("/files/shared/For Alice [NEXUS-DB-alice]/file.txt");
        assert!(can_access_for_download(path, "admin", true));
    }

    #[test]
    fn test_nested_dropbox_blocks_access() {
        // File is in a regular folder, but parent is a dropbox
        let path = Path::new("/files/shared/Submissions [NEXUS-DB]/subfolder/file.txt");
        assert!(!can_access_for_download(path, "alice", false));
        assert!(can_access_for_download(path, "admin", true));
    }

    // ==========================================================================
    // compute_partial_sha256 tests
    // ==========================================================================

    #[tokio::test]
    async fn test_compute_full_file_hash() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, b"Hello, World!").await.unwrap();

        let hash = compute_file_sha256(&file_path).await.unwrap();

        // Known SHA-256 of "Hello, World!"
        assert_eq!(
            hash,
            "dffd6021bb2bd5b0af676290809ec3a53191dd81c7f70a4b28688a362182986f"
        );
    }

    #[tokio::test]
    async fn test_compute_partial_hash() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, b"Hello, World!").await.unwrap();

        // Hash of just "Hello" (5 bytes)
        let hash = compute_partial_sha256(&file_path, 5).await.unwrap();

        // Known SHA-256 of "Hello"
        assert_eq!(
            hash,
            "185f8db32271fe25f561a6fc938b2e264306ec304eda518007d1764826381969"
        );
    }

    #[tokio::test]
    async fn test_compute_partial_hash_larger_than_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, b"Hi").await.unwrap();

        // Request more bytes than file contains - should hash entire file
        let hash = compute_partial_sha256(&file_path, 1000).await.unwrap();

        // Known SHA-256 of "Hi"
        assert_eq!(
            hash,
            "3639efcd08abb273b1619e82e78c29a7df02c1051b1820e99fc395dcaa3326b8"
        );
    }

    #[tokio::test]
    async fn test_compute_hash_empty_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("empty.txt");
        fs::write(&file_path, b"").await.unwrap();

        let hash = compute_file_sha256(&file_path).await.unwrap();

        // Known SHA-256 of empty string
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[tokio::test]
    async fn test_compute_hash_file_not_found() {
        let result = compute_file_sha256(Path::new("/nonexistent/file.txt")).await;
        assert!(result.is_err());
    }

    // ==========================================================================
    // generate_transfer_id tests
    // ==========================================================================

    #[test]
    fn test_transfer_id_format() {
        let id = generate_transfer_id();
        assert_eq!(id.len(), 8);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_transfer_id_uniqueness() {
        let mut ids = HashSet::new();
        for _ in 0..100 {
            let id = generate_transfer_id();
            assert!(ids.insert(id), "Duplicate transfer ID generated");
        }
    }

    // ==========================================================================
    // FileInfo tests
    // ==========================================================================

    #[test]
    fn test_file_info_creation() {
        let info = FileInfo {
            relative_path: "Games/app.zip".to_string(),
            absolute_path: PathBuf::from("/files/shared/Games/app.zip"),
            size: 1024,
        };
        assert_eq!(info.relative_path, "Games/app.zip");
        assert_eq!(info.size, 1024);
    }

    // ==========================================================================
    // AuthenticatedUser tests
    // ==========================================================================

    #[test]
    fn test_authenticated_user_admin() {
        let user = AuthenticatedUser {
            username: "admin".to_string(),
            is_admin: true,
            permissions: HashSet::new(),
        };
        assert!(user.is_admin);
        assert!(user.permissions.is_empty()); // Admins don't need explicit permissions
    }

    #[test]
    fn test_authenticated_user_with_permissions() {
        let mut permissions = HashSet::new();
        permissions.insert(Permission::FileDownload);
        permissions.insert(Permission::FileList);

        let user = AuthenticatedUser {
            username: "alice".to_string(),
            is_admin: false,
            permissions,
        };
        assert!(!user.is_admin);
        assert!(user.permissions.contains(&Permission::FileDownload));
        assert!(user.permissions.contains(&Permission::FileList));
        assert!(!user.permissions.contains(&Permission::FileRoot));
    }
}
