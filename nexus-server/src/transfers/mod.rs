//! File download/upload handler on port 7501. Same TLS cert and framing as the
//! BBS port, with a simplified flow. Per-file streaming is
//! `FileStart → FileStartResponse → [FileData →] FileHash` (FileData omitted for
//! zero-byte/already-complete files); the sender drives downloads, the client
//! drives uploads.
//!
//! Both directions: Handshake → Login (success/error only) → FileDownload or
//! FileUpload → per-file loop → TransferComplete → server closes.

mod auth;
mod download;
mod hashing;
mod helpers;
pub mod registry;
#[cfg(test)]
mod test_helpers;
mod transfer;
mod types;
mod upload;

use std::{collections::HashSet, io, net::SocketAddr, sync::Arc};

use tokio::io::{AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot};
use tokio_rustls::TlsAcceptor;
use tracing::{debug, error, warn};

use nexus_common::address::is_private_network;
use nexus_common::framing::{FrameReader, FrameWriter, MessageId};
use nexus_common::io::send_server_message_with_id;
use nexus_common::names::fold_name;
use nexus_common::tls::accept_tls_with_timeout;

use crate::constants::*;
use crate::db::sql::GUEST_USERNAME;
use crate::db::{Database, LoginSnapshotError, Permission};
use crate::files::resolve_user_area;
use crate::handlers::duration::format_duration_remaining;
use crate::handlers::{
    err_account_disabled, err_authentication, err_banned_permanent, err_banned_with_expiry,
    err_database, err_file_area_not_configured, err_guest_disabled,
};
use crate::ip_rule_cache::IpAdmission;
use crate::scheduler::{ConnectionClass, ConnectionId};
use crate::users::UserManager;
use crate::{egress, egress::task::EgressHandle};

use auth::{
    TransferLoginAuth, TransferLoginSuccess, handle_transfer_handshake, handle_transfer_login,
    handle_transfer_request, send_transfer_login_success,
};
use download::handle_download;
use helpers::{login_error_response, send_error_and_close};
use registry::{ActiveTransfer, TransferDirection, TransferRegistration};
use transfer::{Transfer, TransferContext, TransferEgress};
use types::{AuthenticatedUser, TransferRequest};
use upload::handle_upload;

pub use registry::TransferRegistry;
pub use types::TransferParams;

/// Handle a transfer connection (downloads and uploads on port 7501).
pub async fn handle_transfer_connection(
    socket: TcpStream,
    tls_acceptor: TlsAcceptor,
    params: TransferParams,
) -> io::Result<()> {
    // Mandatory TLS (same cert as main port) with slowloris-defense timeout.
    let tls_stream = accept_tls_with_timeout(&tls_acceptor, socket).await?;

    handle_transfer_connection_inner(tls_stream, params).await
}

/// Inner handler over any byte stream, shared by TCP and WebSocket connections.
pub async fn handle_transfer_connection_inner<S>(
    socket: S,
    params: TransferParams,
) -> io::Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let peer_addr = params.peer_addr;
    let egress = params.egress.clone();
    let egress_connection_id = params.egress_connection_id;
    let egress_enabled = !params.lan_egress_bypass_enabled || !is_private_network(peer_addr.ip());
    let mut egress_registered = false;
    let mut egress_dispatch_rx = None;

    if egress_enabled {
        let (egress_dispatch_tx, rx) = mpsc::channel(egress::EGRESS_DISPATCH_QUEUE_CAPACITY);

        match egress
            .register_anon(
                egress_connection_id,
                ConnectionClass::Bulk,
                egress_dispatch_tx,
            )
            .await
        {
            Ok(true) => {
                egress_registered = true;
                egress_dispatch_rx = Some(rx);
            }
            Ok(false) => {
                warn!(
                    ip = %peer_addr,
                    egress_connection_id = egress_connection_id.get(),
                    "{}",
                    LOG_EGRESS_REGISTER_REJECTED
                );
            }
            Err(e) => {
                warn!(
                    ip = %peer_addr,
                    egress_connection_id = egress_connection_id.get(),
                    err = ?e,
                    "{}",
                    LOG_EGRESS_REGISTER_FAILED
                );
            }
        }
    }

    let result =
        handle_transfer_connection_inner_registered(socket, params, egress_dispatch_rx).await;

    if egress_registered {
        match egress.unregister(egress_connection_id).await {
            Ok(()) => {}
            Err(e) => {
                warn!(
                    ip = %peer_addr,
                    egress_connection_id = egress_connection_id.get(),
                    err = ?e,
                    "{}",
                    LOG_EGRESS_UNREGISTER_FAILED
                );
            }
        }
    }

    result
}

async fn handle_transfer_connection_inner_registered<S>(
    socket: S,
    params: TransferParams,
    egress_dispatch_rx: Option<egress::EgressDispatchRx>,
) -> io::Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let TransferParams {
        peer_addr,
        db,
        file_root,
        file_index,
        file_activity,
        transfer_registry,
        ip_rule_cache,
        user_manager,
        fingerprint,
        egress,
        egress_connection_id,
        lan_egress_bypass_enabled: _,
    } = params;

    debug!(ip = %peer_addr, "{}", LOG_TRANSFER_CONNECTION);

    let (reader, writer) = tokio::io::split(socket);
    let buf_reader = BufReader::new(reader);
    let mut frame_reader = FrameReader::new(buf_reader);
    let mut frame_writer = FrameWriter::new(writer);

    // Default locale for errors sent before login.
    let mut locale = DEFAULT_LOCALE.to_string();

    // Phase 1: Handshake
    let handshake_result =
        handle_transfer_handshake(&mut frame_reader, &mut frame_writer, &locale, fingerprint).await;
    if let Err(e) = handshake_result {
        debug!(ip = %peer_addr, err = %e, "{}", LOG_TRANSFER_HANDSHAKE_FAILED);
        let _ = frame_writer.get_mut().shutdown().await;
        return Ok(());
    }

    // Phase 2: Login (authentication only)
    let login_auth =
        match handle_transfer_login(&mut frame_reader, &mut frame_writer, &db, &mut locale).await {
            Ok(login) => login,
            Err(e) => {
                debug!(ip = %peer_addr, err = %e, "{}", LOG_TRANSFER_LOGIN_FAILED);
                let _ = frame_writer.get_mut().shutdown().await;
                return Ok(());
            }
        };

    let login_success = match complete_transfer_login(
        TransferLoginContext {
            db: &db,
            user_manager: &user_manager,
            egress: &egress,
            egress_connection_id,
            egress_connection_registered: egress_dispatch_rx.is_some(),
            locale: &locale,
            peer_addr,
        },
        login_auth,
    )
    .await
    {
        Ok(success) => success,
        Err((message_id, error)) => {
            let response = login_error_response(error);
            let _ = send_server_message_with_id(&mut frame_writer, &response, message_id).await;
            let _ = frame_writer.get_mut().shutdown().await;
            return Ok(());
        }
    };

    if let Err(e) = send_transfer_login_success(&mut frame_writer, login_success.message_id).await {
        debug!(ip = %peer_addr, err = %e, "{}", LOG_TRANSFER_LOGIN_FAILED);
        let _ = frame_writer.get_mut().shutdown().await;
        return Ok(());
    }

    let bandwidth_weight = login_success.bandwidth_weight;
    let user = login_success.user;

    debug!(user = %user.username, ip = %peer_addr, bandwidth_weight, "{}", LOG_TRANSFER_AUTHENTICATED);

    // Phase 3: Transfer request (FileDownload or FileUpload)
    let Some(file_root) = file_root else {
        // Generic error: direction (download/upload) isn't known yet.
        return send_error_and_close(&mut frame_writer, &err_file_area_not_configured(&locale))
            .await;
    };

    let request = match handle_transfer_request(&mut frame_reader, &mut frame_writer, &locale).await
    {
        Ok(req) => req,
        Err(e) => {
            debug!(user = %user.username, ip = %peer_addr, err = %e, "{}", LOG_TRANSFER_REQUEST_FAILED);
            let _ = frame_writer.get_mut().shutdown().await;
            return Ok(());
        }
    };

    // Registry metadata. Download size is unknown until path resolution (0 now,
    // updated later); upload size is known up front.
    let (direction, path, total_size, use_root) = match &request {
        TransferRequest::Download(p) => (TransferDirection::Download, p.path.clone(), 0, p.root),
        TransferRequest::Upload(p) => (
            TransferDirection::Upload,
            p.destination.clone(),
            p.total_size,
            p.root,
        ),
    };
    // Register after reading the request so the connection monitor can show
    // direction/path, but refresh identity by immutable user id under the same
    // rename-serialization guard used by BBS login.
    let (user, info, ban_rx, user_area_root) = match register_transfer_with_current_identity(
        TransferRegistrationContext {
            db: &db,
            user_manager: &user_manager,
            transfer_registry: &transfer_registry,
            locale: &locale,
            file_root,
        },
        user,
        PendingTransferRegistration {
            peer_addr,
            direction,
            path,
            total_size,
            use_root,
        },
    )
    .await
    {
        Ok(registered) => registered,
        Err(error) => return send_error_and_close(&mut frame_writer, &error).await,
    };

    if let IpAdmission::Banned { expires_at } = ip_rule_cache.check_admission(peer_addr.ip()).await
    {
        transfer_registry.unregister(info.id);
        return send_error_and_close(&mut frame_writer, &late_ban_error(&locale, expires_at)).await;
    }

    // Owns the connection and ban handling; unregisters on drop via RAII guard.
    // The user snapshot below is post-refresh. Later identity/permission changes
    // update connection-monitor display, but do not re-authorize an in-flight transfer.
    let mut transfer = Transfer::new(
        frame_reader,
        frame_writer,
        ban_rx,
        info,
        TransferContext {
            user,
            locale,
            file_root,
            file_index: &file_index,
            file_activity: &file_activity,
            user_area_root,
            registry: &transfer_registry,
            egress: egress_dispatch_rx
                .map(|rx| TransferEgress::new(egress.clone(), egress_connection_id, rx)),
        },
    );

    let result = match request {
        TransferRequest::Download(params) => handle_download(&mut transfer, params).await,
        TransferRequest::Upload(params) => handle_upload(&mut transfer, params).await,
    };

    let elapsed = transfer.elapsed();
    let bytes = transfer.bytes_transferred();
    debug!(
        id = %transfer.id(),
        user = %transfer.user().username,
        ip = %peer_addr,
        bytes = %bytes,
        elapsed_secs = elapsed.as_secs_f64(),
        "{}", LOG_TRANSFER_COMPLETE
    );

    result
}

fn late_ban_error(locale: &str, expires_at: Option<i64>) -> String {
    match expires_at {
        Some(expiry) => err_banned_with_expiry(locale, &format_duration_remaining(expiry)),
        None => err_banned_permanent(locale),
    }
}

struct TransferLoginContext<'a> {
    db: &'a Database,
    user_manager: &'a UserManager,
    egress: &'a EgressHandle,
    egress_connection_id: ConnectionId,
    egress_connection_registered: bool,
    locale: &'a str,
    peer_addr: SocketAddr,
}

async fn complete_transfer_login(
    ctx: TransferLoginContext<'_>,
    mut login: TransferLoginAuth,
) -> Result<TransferLoginSuccess, (MessageId, String)> {
    let _user_state = ctx.user_manager.read_user_state().await;
    let snapshot = match ctx.db.get_login_session_snapshot(login.user.user_id).await {
        Ok(Some(snapshot)) if snapshot.account.enabled => snapshot,
        Ok(Some(snapshot)) => {
            let error = if fold_name(&snapshot.account.username) == GUEST_USERNAME {
                err_guest_disabled(ctx.locale)
            } else {
                err_account_disabled(ctx.locale, &snapshot.account.username)
            };
            return Err((login.message_id, error));
        }
        Ok(None) => return Err((login.message_id, err_authentication(ctx.locale))),
        Err(e) => {
            return Err((
                login.message_id,
                handle_transfer_login_snapshot_error(
                    &e,
                    login.user.user_id,
                    ctx.peer_addr,
                    ctx.locale,
                ),
            ));
        }
    };

    login.user.username = snapshot.account.username.clone();
    login.user.is_admin = snapshot.account.is_admin;
    login.user.is_shared = snapshot.account.is_shared;
    login.user.permissions = snapshot.permissions.permissions;
    if !snapshot.account.is_shared {
        login.user.nickname = snapshot.account.username.clone();
    }

    if ctx.egress_connection_registered {
        transition_transfer_login_to_user_flow(
            ctx.egress,
            ctx.egress_connection_id,
            login.user.user_id,
            snapshot.resolved_bandwidth_weight,
            ctx.peer_addr,
        );
    }

    Ok(TransferLoginSuccess {
        user: login.user,
        message_id: login.message_id,
        bandwidth_weight: snapshot.resolved_bandwidth_weight,
    })
}

fn transition_transfer_login_to_user_flow(
    egress: &EgressHandle,
    egress_connection_id: ConnectionId,
    user_id: i64,
    weight: u16,
    peer_addr: SocketAddr,
) {
    match egress.transition_to_user(egress_connection_id, user_id, weight) {
        Ok(()) => {}
        Err(e) => {
            warn!(
                ip = %peer_addr,
                egress_connection_id = egress_connection_id.get(),
                user_id,
                weight,
                err = ?e,
                "{}",
                LOG_EGRESS_TRANSITION_FAILED
            );
        }
    }
}

fn handle_transfer_login_snapshot_error(
    err: &LoginSnapshotError,
    user_id: i64,
    peer_addr: SocketAddr,
    locale: &str,
) -> String {
    let (message, source) = match err {
        LoginSnapshotError::User(e) => (LOG_TRANSFER_REGISTRATION_DB_ERROR, e),
        LoginSnapshotError::Permissions(e) => (LOG_TRANSFER_REGISTRATION_DB_ERROR, e),
        LoginSnapshotError::Group(e) => (LOG_TRANSFER_REGISTRATION_DB_ERROR, e),
        LoginSnapshotError::BandwidthWeight(e) => (LOG_BANDWIDTH_WEIGHT_RESOLVE_FAILED, e),
    };
    error!(user_id, ip = %peer_addr, err = %source, "{}", message);
    err_database(locale)
}

struct TransferRegistrationContext<'a> {
    db: &'a Database,
    user_manager: &'a UserManager,
    transfer_registry: &'a TransferRegistry,
    locale: &'a str,
    file_root: &'a std::path::Path,
}

struct PendingTransferRegistration {
    peer_addr: SocketAddr,
    direction: TransferDirection,
    path: String,
    total_size: u64,
    use_root: bool,
}

async fn register_transfer_with_current_identity(
    ctx: TransferRegistrationContext<'_>,
    mut user: AuthenticatedUser,
    pending: PendingTransferRegistration,
) -> Result<
    (
        AuthenticatedUser,
        Arc<ActiveTransfer>,
        oneshot::Receiver<()>,
        Option<std::path::PathBuf>,
    ),
    String,
> {
    let _user_state = ctx.user_manager.read_user_state().await;

    let account = ctx
        .db
        .users
        .get_user_by_id(user.user_id)
        .await
        .map_err(|e| {
            error!(user_id = user.user_id, err = %e, "{}", LOG_TRANSFER_REGISTRATION_DB_ERROR);
            err_database(ctx.locale)
        })?
        .ok_or_else(|| err_authentication(ctx.locale))?;

    if !account.enabled {
        return Err(if fold_name(&account.username) == GUEST_USERNAME {
            err_guest_disabled(ctx.locale)
        } else {
            err_account_disabled(ctx.locale, &account.username)
        });
    }

    user.username = account.username.clone();
    user.is_admin = account.is_admin;
    user.is_shared = account.is_shared;
    user.permissions =
        current_transfer_permissions(ctx.db, account.id, account.is_admin, ctx.locale).await?;
    if !account.is_shared {
        user.nickname = account.username;
    }

    let user_area_root = if pending.use_root {
        None
    } else {
        Some(resolve_user_area(ctx.file_root, &user.username).await)
    };

    let (info, ban_rx) = ctx.transfer_registry.register(TransferRegistration {
        user_id: user.user_id,
        peer_addr: pending.peer_addr,
        nickname: user.nickname.clone(),
        username: user.username.clone(),
        is_admin: user.is_admin,
        is_shared: user.is_shared,
        direction: pending.direction,
        path: pending.path,
        total_size: pending.total_size,
    });

    Ok((user, info, ban_rx, user_area_root))
}

async fn current_transfer_permissions(
    db: &Database,
    user_id: i64,
    is_admin: bool,
    locale: &str,
) -> Result<HashSet<Permission>, String> {
    if is_admin {
        Ok(HashSet::new())
    } else {
        db.users
            .get_user_permissions(user_id)
            .await
            .map(|perms| perms.permissions)
            .map_err(|e| {
                error!(user_id, err = %e, "{}", LOG_TRANSFER_REGISTRATION_DB_ERROR);
                err_database(locale)
            })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::net::SocketAddr;
    use std::sync::Arc;

    use tokio::io::BufReader;
    use tokio::time::{self, Duration};

    use nexus_common::io::{read_server_message, send_client_message};
    use nexus_common::protocol::{ClientMessage, ServerMessage};

    use crate::db::testing::create_test_db;
    use crate::db::{
        CreateUserParams, Database, Permission, PermissionWriteScope, Permissions,
        UpdateUserParams, UserAccount,
    };
    use crate::egress::task::{
        DEFAULT_EGRESS_COMMAND_QUEUE_CAPACITY, EgressCommand, EgressHandle, EgressSettingsCommand,
    };
    use crate::files::{FileActivityMap, FileIndex};
    use crate::handlers::testing::{TEST_FINGERPRINT, get_cached_password_hash};
    use crate::ip_rule_cache::{IpRuleCache, IpRuleState};
    use crate::scheduler::ConnectionId;
    use crate::users::UserManager;
    use tempfile::TempDir;

    use super::*;

    async fn create_transfer_user(
        db: &Database,
        username: &str,
        is_shared: bool,
        permissions: &Permissions,
    ) -> UserAccount {
        create_transfer_user_with_enabled(db, username, is_shared, true, permissions).await
    }

    async fn create_transfer_user_with_enabled(
        db: &Database,
        username: &str,
        is_shared: bool,
        enabled: bool,
        permissions: &Permissions,
    ) -> UserAccount {
        db.users
            .create_user(CreateUserParams {
                username,
                hashed_password: "hash",
                is_admin: false,
                is_shared,
                enabled,
                permissions,
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap()
    }

    async fn create_transfer_user_with_weight(
        db: &Database,
        username: &str,
        weight: u16,
        permissions: &Permissions,
    ) -> UserAccount {
        db.users
            .create_user(CreateUserParams {
                username,
                hashed_password: &get_cached_password_hash("password"),
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions,
                group_id: None,
                revokes: &[],
                bandwidth_weight: Some(weight),
            })
            .await
            .unwrap()
    }

    fn egress_handle_for_test() -> (
        EgressHandle,
        tokio::sync::mpsc::Receiver<EgressCommand>,
        tokio::sync::mpsc::UnboundedReceiver<EgressSettingsCommand>,
    ) {
        let (egress_tx, egress_command_rx) =
            tokio::sync::mpsc::channel(DEFAULT_EGRESS_COMMAND_QUEUE_CAPACITY);
        let (egress_settings_tx, egress_settings_rx) = tokio::sync::mpsc::unbounded_channel();
        (
            EgressHandle::new(egress_tx, egress_settings_tx),
            egress_command_rx,
            egress_settings_rx,
        )
    }

    struct TransferHarness {
        params: TransferParams,
        _temp_dir: TempDir,
    }

    async fn transfer_harness(
        db: Database,
        user_manager: UserManager,
        egress: EgressHandle,
        egress_connection_id: ConnectionId,
    ) -> TransferHarness {
        let temp_dir = TempDir::new().unwrap();
        let file_index = Arc::new(FileIndex::new(temp_dir.path(), temp_dir.path()));
        let file_activity = Arc::new(FileActivityMap::new());
        TransferHarness {
            params: TransferParams {
                peer_addr: SocketAddr::from(([127, 0, 0, 1], 7501)),
                db,
                file_root: None,
                file_index,
                file_activity,
                transfer_registry: Arc::new(TransferRegistry::new()),
                ip_rule_cache: Arc::new(IpRuleState::new(IpRuleCache::new())),
                user_manager,
                fingerprint: TEST_FINGERPRINT,
                egress,
                egress_connection_id,
                lan_egress_bypass_enabled: false,
            },
            _temp_dir: temp_dir,
        }
    }

    async fn send_transfer_handshake_and_login(
        writer: &mut FrameWriter<tokio::io::WriteHalf<tokio::io::DuplexStream>>,
        reader: &mut FrameReader<BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>>,
        username: &str,
        password: &str,
    ) -> ServerMessage {
        send_client_message(
            writer,
            &ClientMessage::Handshake {
                version: nexus_common::PROTOCOL_VERSION.to_string(),
            },
        )
        .await
        .unwrap();
        let handshake = read_server_message(reader)
            .await
            .unwrap()
            .expect("handshake response");
        assert!(matches!(
            handshake.message,
            ServerMessage::HandshakeResponse { success: true, .. }
        ));

        send_client_message(
            writer,
            &ClientMessage::Login {
                username: username.to_string(),
                password: password.to_string(),
                features: vec![],
                locale: "en".to_string(),
                avatar: None,
                nickname: None,
            },
        )
        .await
        .unwrap();

        read_server_message(reader)
            .await
            .unwrap()
            .expect("login response")
            .message
    }

    async fn disable_user(db: &Database, username: &str) {
        db.users
            .update_user(UpdateUserParams {
                username,
                new_username: None,
                new_password_hash: None,
                is_admin: None,
                enabled: Some(false),
                permissions: None,
                revokes: None,
                remove_group: false,
                group_id: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: false,
                requester_is_admin: true,
                permission_write_scope: PermissionWriteScope::ReplaceAll,
                requester_bandwidth_max: None,
            })
            .await
            .unwrap();
    }

    fn stale_auth_user(
        user_id: i64,
        username: &str,
        nickname: &str,
        is_shared: bool,
    ) -> AuthenticatedUser {
        AuthenticatedUser {
            user_id,
            nickname: nickname.to_string(),
            username: username.to_string(),
            is_admin: false,
            is_shared,
            permissions: HashSet::new(),
        }
    }

    fn pending_registration() -> PendingTransferRegistration {
        PendingTransferRegistration {
            peer_addr: SocketAddr::from(([127, 0, 0, 1], 7501)),
            direction: TransferDirection::Download,
            path: "file.bin".to_string(),
            total_size: 0,
            use_root: false,
        }
    }

    fn registered_dispatch_rx() -> egress::EgressDispatchRx {
        let (_tx, rx) = mpsc::channel(egress::EGRESS_DISPATCH_QUEUE_CAPACITY);
        rx
    }

    #[tokio::test]
    async fn transfer_login_transitions_before_success_response() {
        let pool = create_test_db().await;
        let db = Database::new(pool);
        let user_manager = UserManager::new();
        let permissions = Permissions::from(&[Permission::FileDownload]);
        let account = create_transfer_user_with_weight(&db, "alice", 9, &permissions).await;
        let (egress, _command_rx, mut settings_rx) = egress_handle_for_test();
        let egress_connection_id = ConnectionId::new(9_001);
        let harness = transfer_harness(db, user_manager, egress, egress_connection_id).await;

        let (client, server) = tokio::io::duplex(4096);
        let (client_read, client_write) = tokio::io::split(client);
        let mut client_reader = FrameReader::new(BufReader::new(client_read));
        let mut client_writer = FrameWriter::new(client_write);
        let server_task = tokio::spawn(handle_transfer_connection_inner_registered(
            server,
            harness.params,
            Some(registered_dispatch_rx()),
        ));

        send_client_message(
            &mut client_writer,
            &ClientMessage::Handshake {
                version: nexus_common::PROTOCOL_VERSION.to_string(),
            },
        )
        .await
        .unwrap();
        let handshake = read_server_message(&mut client_reader)
            .await
            .unwrap()
            .expect("handshake response");
        assert!(matches!(
            handshake.message,
            ServerMessage::HandshakeResponse { success: true, .. }
        ));

        send_client_message(
            &mut client_writer,
            &ClientMessage::Login {
                username: "alice".to_string(),
                password: "password".to_string(),
                features: vec![],
                locale: "en".to_string(),
                avatar: None,
                nickname: None,
            },
        )
        .await
        .unwrap();

        let transition = time::timeout(Duration::from_secs(1), settings_rx.recv())
            .await
            .unwrap()
            .expect("transition should be queued before login response");
        match transition {
            EgressSettingsCommand::TransitionToUser {
                connection_id,
                user_id,
                weight,
            } => {
                assert_eq!(connection_id, egress_connection_id);
                assert_eq!(user_id, account.id);
                assert_eq!(weight, 9);
            }
            _ => panic!("expected transfer login transition"),
        }

        let login = read_server_message(&mut client_reader)
            .await
            .unwrap()
            .expect("login response");
        assert!(matches!(
            login.message,
            ServerMessage::LoginResponse { success: true, .. }
        ));

        drop(client_writer);
        server_task.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn failed_transfer_login_does_not_transition() {
        let pool = create_test_db().await;
        let db = Database::new(pool);
        let user_manager = UserManager::new();
        let permissions = Permissions::from(&[Permission::FileDownload]);
        create_transfer_user_with_weight(&db, "alice", 9, &permissions).await;
        let (egress, _command_rx, mut settings_rx) = egress_handle_for_test();
        let harness = transfer_harness(db, user_manager, egress, ConnectionId::new(9_002)).await;

        let (client, server) = tokio::io::duplex(4096);
        let (client_read, client_write) = tokio::io::split(client);
        let mut client_reader = FrameReader::new(BufReader::new(client_read));
        let mut client_writer = FrameWriter::new(client_write);
        let server_task = tokio::spawn(handle_transfer_connection_inner_registered(
            server,
            harness.params,
            Some(registered_dispatch_rx()),
        ));

        let login = send_transfer_handshake_and_login(
            &mut client_writer,
            &mut client_reader,
            "alice",
            "wrong-password",
        )
        .await;
        assert!(matches!(
            login,
            ServerMessage::LoginResponse { success: false, .. }
        ));
        assert!(settings_rx.try_recv().is_err());

        drop(client_writer);
        server_task.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn transfer_login_succeeds_when_egress_settings_channel_closed() {
        let pool = create_test_db().await;
        let db = Database::new(pool);
        let user_manager = UserManager::new();
        let permissions = Permissions::from(&[Permission::FileDownload]);
        create_transfer_user_with_weight(&db, "alice", 9, &permissions).await;
        let (egress, _command_rx, settings_rx) = egress_handle_for_test();
        drop(settings_rx);
        let harness = transfer_harness(db, user_manager, egress, ConnectionId::new(9_003)).await;

        let (client, server) = tokio::io::duplex(4096);
        let (client_read, client_write) = tokio::io::split(client);
        let mut client_reader = FrameReader::new(BufReader::new(client_read));
        let mut client_writer = FrameWriter::new(client_write);
        let server_task = tokio::spawn(handle_transfer_connection_inner_registered(
            server,
            harness.params,
            None,
        ));

        let login = send_transfer_handshake_and_login(
            &mut client_writer,
            &mut client_reader,
            "alice",
            "password",
        )
        .await;
        assert!(matches!(
            login,
            ServerMessage::LoginResponse { success: true, .. }
        ));

        drop(client_writer);
        server_task.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn transfer_login_transition_uses_fresh_weight_snapshot() {
        let pool = create_test_db().await;
        let db = Database::new(pool);
        let user_manager = UserManager::new();
        let permissions = Permissions::from(&[Permission::FileDownload]);
        let account = create_transfer_user_with_weight(&db, "alice", 1, &permissions).await;
        db.users
            .update_user(UpdateUserParams {
                username: "alice",
                new_username: None,
                new_password_hash: None,
                is_admin: None,
                enabled: None,
                permissions: None,
                revokes: None,
                remove_group: false,
                group_id: None,
                bandwidth_weight: Some(10),
                inherit_bandwidth_weight: false,
                requester_is_admin: true,
                permission_write_scope: PermissionWriteScope::ReplaceAll,
                requester_bandwidth_max: None,
            })
            .await
            .unwrap();
        let (egress, _command_rx, mut settings_rx) = egress_handle_for_test();
        let login = TransferLoginAuth {
            user: stale_auth_user(account.id, "alice", "alice", false),
            message_id: MessageId::new(),
        };

        let success = complete_transfer_login(
            TransferLoginContext {
                db: &db,
                user_manager: &user_manager,
                egress: &egress,
                egress_connection_id: ConnectionId::new(9_004),
                egress_connection_registered: true,
                locale: "en",
                peer_addr: SocketAddr::from(([127, 0, 0, 1], 7501)),
            },
            login,
        )
        .await
        .expect("transfer login should complete");

        assert_eq!(success.bandwidth_weight, 10);
        let transition = settings_rx.try_recv().expect("transition command");
        match transition {
            EgressSettingsCommand::TransitionToUser {
                user_id, weight, ..
            } => {
                assert_eq!(user_id, account.id);
                assert_eq!(weight, 10);
            }
            _ => panic!("expected transfer login transition"),
        }
    }

    #[tokio::test]
    async fn transfer_login_without_registered_egress_skips_transition() {
        let pool = create_test_db().await;
        let db = Database::new(pool);
        let user_manager = UserManager::new();
        let permissions = Permissions::from(&[Permission::FileDownload]);
        let account = create_transfer_user_with_weight(&db, "alice", 7, &permissions).await;
        let (egress, _command_rx, mut settings_rx) = egress_handle_for_test();
        let login = TransferLoginAuth {
            user: stale_auth_user(account.id, "alice", "alice", false),
            message_id: MessageId::new(),
        };

        let success = complete_transfer_login(
            TransferLoginContext {
                db: &db,
                user_manager: &user_manager,
                egress: &egress,
                egress_connection_id: ConnectionId::new(9_104),
                egress_connection_registered: false,
                locale: "en",
                peer_addr: SocketAddr::from(([127, 0, 0, 1], 7501)),
            },
            login,
        )
        .await
        .expect("transfer login should complete");

        assert_eq!(success.bandwidth_weight, 7);
        assert!(settings_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn transfer_egress_unregister_runs_after_inner_error() {
        let pool = create_test_db().await;
        let db = Database::new(pool);
        let user_manager = UserManager::new();
        let (egress, mut command_rx, _settings_rx) = egress_handle_for_test();
        let egress_connection_id = ConnectionId::new(9_005);
        let harness = transfer_harness(db, user_manager, egress, egress_connection_id).await;
        let (client, server) = tokio::io::duplex(4096);

        let server_task = tokio::spawn(handle_transfer_connection_inner(server, harness.params));
        drop(client);
        let register = command_rx.recv().await.expect("register command");
        match register {
            EgressCommand::RegisterAnon {
                connection_id,
                class,
                reply_tx,
                ..
            } => {
                assert_eq!(connection_id, egress_connection_id);
                assert_eq!(class, ConnectionClass::Bulk);
                reply_tx.send(true).unwrap();
            }
            _ => panic!("expected RegisterAnon"),
        }

        let unregister = command_rx.recv().await.expect("unregister command");
        match unregister {
            EgressCommand::Unregister { connection_id } => {
                assert_eq!(connection_id, egress_connection_id);
            }
            _ => panic!("expected Unregister"),
        }
        server_task.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn lan_peer_bypasses_transfer_egress_registration() {
        let pool = create_test_db().await;
        let db = Database::new(pool);
        let user_manager = UserManager::new();
        let (egress, mut command_rx, _settings_rx) = egress_handle_for_test();
        let mut harness =
            transfer_harness(db, user_manager, egress, ConnectionId::new(9_105)).await;
        harness.params.lan_egress_bypass_enabled = true;
        let (client, server) = tokio::io::duplex(4096);

        let server_task = tokio::spawn(handle_transfer_connection_inner(server, harness.params));
        time::sleep(Duration::from_millis(25)).await;
        assert!(command_rx.try_recv().is_err());

        drop(client);
        time::timeout(Duration::from_secs(2), server_task)
            .await
            .expect("transfer task should exit")
            .expect("transfer task should not panic")
            .expect("transfer task should not fail");
    }

    #[tokio::test]
    async fn yggdrasil_peer_uses_transfer_egress_registration_under_lan_bypass() {
        let pool = create_test_db().await;
        let db = Database::new(pool);
        let user_manager = UserManager::new();
        let (egress, mut command_rx, _settings_rx) = egress_handle_for_test();
        let egress_connection_id = ConnectionId::new(9_106);
        let mut harness = transfer_harness(db, user_manager, egress, egress_connection_id).await;
        harness.params.peer_addr = SocketAddr::from(([0x0200, 0, 0, 0, 0, 0, 0, 1], 7501));
        harness.params.lan_egress_bypass_enabled = true;
        let (client, server) = tokio::io::duplex(4096);

        let server_task = tokio::spawn(handle_transfer_connection_inner(server, harness.params));
        let register = time::timeout(Duration::from_secs(2), command_rx.recv())
            .await
            .expect("timed out waiting for Yggdrasil transfer registration")
            .expect("register command");
        match register {
            EgressCommand::RegisterAnon {
                connection_id,
                class,
                reply_tx,
                ..
            } => {
                assert_eq!(connection_id, egress_connection_id);
                assert_eq!(class, ConnectionClass::Bulk);
                reply_tx.send(true).unwrap();
            }
            _ => panic!("expected RegisterAnon"),
        }

        drop(client);
        let unregister = time::timeout(Duration::from_secs(2), command_rx.recv())
            .await
            .expect("timed out waiting for Yggdrasil transfer unregister")
            .expect("unregister command");
        match unregister {
            EgressCommand::Unregister { connection_id } => {
                assert_eq!(connection_id, egress_connection_id);
            }
            _ => panic!("expected Unregister"),
        }

        time::timeout(Duration::from_secs(2), server_task)
            .await
            .expect("transfer task should exit")
            .expect("transfer task should not panic")
            .expect("transfer task should not fail");
    }

    #[tokio::test]
    async fn test_register_transfer_refreshes_regular_identity_by_user_id() {
        let pool = create_test_db().await;
        let db = Database::new(pool);
        let user_manager = UserManager::new();
        let registry = TransferRegistry::new();
        let file_root = TempDir::new().unwrap();
        let permissions = Permissions::from(&[Permission::FileDownload]);
        let account = create_transfer_user(&db, "alicia", false, &permissions).await;

        let stale_user = stale_auth_user(account.id, "alice", "alice", false);

        let (registered_user, info, _ban_rx, _user_area_root) =
            register_transfer_with_current_identity(
                TransferRegistrationContext {
                    db: &db,
                    user_manager: &user_manager,
                    transfer_registry: &registry,
                    locale: "en",
                    file_root: file_root.path(),
                },
                stale_user,
                pending_registration(),
            )
            .await
            .unwrap();

        let transfer_info = info.to_transfer_info();
        assert_eq!(transfer_info.username, "alicia");
        assert_eq!(transfer_info.nickname, "alicia");
        assert_eq!(registered_user.username, "alicia");
        assert_eq!(registered_user.nickname, "alicia");
        assert!(
            registered_user
                .permissions
                .contains(&Permission::FileDownload)
        );
        assert_eq!(registry.snapshot().len(), 1);
    }

    #[tokio::test]
    async fn test_register_transfer_shared_identity_keeps_auth_time_nickname() {
        let pool = create_test_db().await;
        let db = Database::new(pool);
        let user_manager = UserManager::new();
        let registry = TransferRegistry::new();
        let file_root = TempDir::new().unwrap();
        let permissions = Permissions::new();
        let account = create_transfer_user(&db, "shared2", true, &permissions).await;

        let stale_user = stale_auth_user(account.id, "shared", "GuestOne", true);

        let (registered_user, info, _ban_rx, _user_area_root) =
            register_transfer_with_current_identity(
                TransferRegistrationContext {
                    db: &db,
                    user_manager: &user_manager,
                    transfer_registry: &registry,
                    locale: "en",
                    file_root: file_root.path(),
                },
                stale_user,
                pending_registration(),
            )
            .await
            .unwrap();

        let transfer_info = info.to_transfer_info();
        assert_eq!(transfer_info.username, "shared2");
        assert_eq!(transfer_info.nickname, "GuestOne");
        assert_eq!(registered_user.username, "shared2");
        assert_eq!(registered_user.nickname, "GuestOne");
        assert!(registered_user.is_shared);
    }

    #[tokio::test]
    async fn test_register_transfer_rejects_deleted_account_by_user_id() {
        let pool = create_test_db().await;
        let db = Database::new(pool);
        let user_manager = UserManager::new();
        let registry = TransferRegistry::new();
        let file_root = TempDir::new().unwrap();
        let permissions = Permissions::new();
        let account = create_transfer_user(&db, "alice", false, &permissions).await;
        assert!(db.users.delete_user(account.id, true).await.unwrap());

        let stale_user = stale_auth_user(account.id, "alice", "alice", false);

        let error = match register_transfer_with_current_identity(
            TransferRegistrationContext {
                db: &db,
                user_manager: &user_manager,
                transfer_registry: &registry,
                locale: "en",
                file_root: file_root.path(),
            },
            stale_user,
            pending_registration(),
        )
        .await
        {
            Ok(_) => panic!("deleted account must not register a transfer"),
            Err(error) => error,
        };

        assert_eq!(error, err_authentication("en"));
        assert_eq!(registry.snapshot().len(), 0);
    }

    #[tokio::test]
    async fn test_register_transfer_rejects_disabled_regular_account() {
        let pool = create_test_db().await;
        let db = Database::new(pool);
        let user_manager = UserManager::new();
        let registry = TransferRegistry::new();
        let file_root = TempDir::new().unwrap();
        let permissions = Permissions::new();
        let account =
            create_transfer_user_with_enabled(&db, "alice", false, false, &permissions).await;

        let stale_user = stale_auth_user(account.id, "alice", "alice", false);

        let error = match register_transfer_with_current_identity(
            TransferRegistrationContext {
                db: &db,
                user_manager: &user_manager,
                transfer_registry: &registry,
                locale: "en",
                file_root: file_root.path(),
            },
            stale_user,
            pending_registration(),
        )
        .await
        {
            Ok(_) => panic!("disabled regular account must not register a transfer"),
            Err(error) => error,
        };

        assert_eq!(error, err_account_disabled("en", "alice"));
        assert_eq!(registry.snapshot().len(), 0);
    }

    #[tokio::test]
    async fn test_register_transfer_rejects_disabled_guest_account() {
        let pool = create_test_db().await;
        let db = Database::new(pool);
        let user_manager = UserManager::new();
        let registry = TransferRegistry::new();
        let file_root = TempDir::new().unwrap();
        disable_user(&db, GUEST_USERNAME).await;
        let guest = db
            .users
            .get_user_by_username(GUEST_USERNAME)
            .await
            .unwrap()
            .unwrap();

        let stale_user = stale_auth_user(guest.id, GUEST_USERNAME, "Visitor", true);

        let error = match register_transfer_with_current_identity(
            TransferRegistrationContext {
                db: &db,
                user_manager: &user_manager,
                transfer_registry: &registry,
                locale: "en",
                file_root: file_root.path(),
            },
            stale_user,
            pending_registration(),
        )
        .await
        {
            Ok(_) => panic!("disabled guest account must not register a transfer"),
            Err(error) => error,
        };

        assert_eq!(error, err_guest_disabled("en"));
        assert_eq!(registry.snapshot().len(), 0);
    }
}
