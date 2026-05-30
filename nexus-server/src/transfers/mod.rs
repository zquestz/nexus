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
use tokio::sync::oneshot;
use tokio_rustls::TlsAcceptor;
use tracing::{debug, error};

use nexus_common::framing::{FrameReader, FrameWriter};
use nexus_common::names::fold_name;
use nexus_common::tls::accept_tls_with_timeout;

use crate::constants::*;
use crate::db::sql::GUEST_USERNAME;
use crate::db::{Database, Permission};
use crate::files::resolve_user_area;
use crate::handlers::duration::format_duration_remaining;
use crate::handlers::{
    err_account_disabled, err_authentication, err_banned_permanent, err_banned_with_expiry,
    err_database, err_file_area_not_configured, err_guest_disabled,
};
use crate::ip_rule_cache::IpAdmission;
use crate::users::UserManager;

use auth::{handle_transfer_handshake, handle_transfer_login, handle_transfer_request};
use download::handle_download;
use helpers::send_error_and_close;
use registry::{ActiveTransfer, TransferDirection, TransferRegistration};
use transfer::{Transfer, TransferContext};
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
    let user =
        match handle_transfer_login(&mut frame_reader, &mut frame_writer, &db, &mut locale).await {
            Ok(user) => user,
            Err(e) => {
                debug!(ip = %peer_addr, err = %e, "{}", LOG_TRANSFER_LOGIN_FAILED);
                let _ = frame_writer.get_mut().shutdown().await;
                return Ok(());
            }
        };

    debug!(user = %user.username, ip = %peer_addr, "{}", LOG_TRANSFER_AUTHENTICATED);

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

    use crate::db::testing::create_test_db;
    use crate::db::{
        CreateUserParams, Database, Permission, PermissionWriteScope, Permissions,
        UpdateUserParams, UserAccount,
    };
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
