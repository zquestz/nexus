//! Nexus BBS Server

use std::fs;
use std::future::Future;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use clap::Parser;
use nexus_common::names::fold_name;
use nexus_common::rate_limiter::RateLimiter;
use nexus_common::validators::{
    DEFAULT_BANDWIDTH_CHUNK_SIZE, MAX_BANDWIDTH_CHUNK_SIZE, MIN_BANDWIDTH_CHUNK_SIZE,
    validate_bandwidth_chunk_size,
};
use nexus_server::{
    args, channels, connection, connection_tracker, constants, db, egress, files, flood, handlers,
    ip_rule_cache, paths, scheduler, tracker, transfers, upnp, users, voice, websocket,
};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsAcceptor;
use tracing::{debug, error, info, warn};

use args::Cli;
use channels::{Channel, ChannelManager};
use connection::ConnectionParams;
use connection_tracker::ConnectionTracker;
use constants::*;
use files::{FileActivityMap, FileIndex};
use flood::FloodConfig;
use ip_rule_cache::{IpRuleCache, IpRuleState};
use nexus_common::address::normalize_socket_addr;
use nexus_common::logging::{self, LogInitParams, LogLevel};
use transfers::{TransferParams, TransferRegistry};
use users::UserManager;
use voice::{VoiceControlHandle, VoiceRegistry, VoiceUdpServer, create_voice_listener};

#[tokio::main]
async fn main() {
    // Must install before any TLS/DTLS op: tokio-rustls and dtls share
    // rustls 0.23, which needs an explicit crypto provider selection.
    tokio_rustls::rustls::crypto::ring::default_provider()
        .install_default()
        .expect(ERR_RUSTLS_PROVIDER);

    let mut cli = Cli::parse();

    let data_dir = paths::resolve_data_dir(cli.data_dir.take());

    if let Err(e) = logging::init(LogInitParams {
        data_dir: &data_dir,
        level: cli.log_level,
        retention: cli.log_retention,
        no_timestamps: cli.no_log_timestamps,
        log_file_prefix: LOG_FILE_PREFIX,
    }) {
        // `init` installs a stderr-only fallback before returning Err,
        // so this warning still reaches the user.
        warn!(err = %e, "{}", LOG_LOGGING_INIT_FAILED);
    }

    if let Err(e) = ensure_data_dir(&data_dir) {
        error!("{}", e);
        std::process::exit(1);
    }

    let log_dir = logging::log_dir(&data_dir);

    // `purge_old_logs` no-ops when retention is zero, so no outer gate needed.
    logging::purge_old_logs(&log_dir, cli.log_retention, LOG_FILE_PREFIX);

    info!("{}{}", MSG_BANNER, env!("CARGO_PKG_VERSION"));
    info!("{}{}", MSG_LOG_LEVEL, cli.log_level);
    if cli.log_level != LogLevel::None && cli.log_retention > std::time::Duration::ZERO {
        info!("{}{}", MSG_LOG_DIR, log_dir.display());
    }

    let db_path = db::database_path(&data_dir);
    let (database, user_manager) = setup_db(&db_path).await;

    // IP rule cache: cleanup expired entries, then load active ones.
    let expired_bans = database
        .bans
        .cleanup_expired_bans()
        .await
        .unwrap_or_else(|e| {
            error!(err = %e, "{}", LOG_CLEANUP_EXPIRED_BANS_FAILED);
            0
        });
    let expired_trusts = database
        .trusts
        .cleanup_expired_trusts()
        .await
        .unwrap_or_else(|e| {
            error!(err = %e, "{}", LOG_CLEANUP_EXPIRED_TRUSTS_FAILED);
            0
        });
    debug!(
        bans = expired_bans,
        trusts = expired_trusts,
        "{}",
        LOG_CLEANUP_EXPIRED
    );

    let ban_records = database
        .bans
        .load_all_active_bans()
        .await
        .unwrap_or_else(|e| {
            error!(err = %e, "{}", LOG_LOAD_BANS_FAILED);
            Vec::new()
        });
    let trust_records = database
        .trusts
        .load_all_active_trusts()
        .await
        .unwrap_or_else(|e| {
            error!(err = %e, "{}", LOG_LOAD_TRUSTS_FAILED);
            Vec::new()
        });
    let ban_count = ban_records.len();
    let trust_count = trust_records.len();
    let ip_rule_cache = Arc::new(IpRuleState::new(IpRuleCache::from_records(
        ban_records,
        trust_records,
    )));
    debug!(
        bans = ban_count,
        trusts = trust_count,
        "{}",
        LOG_LOADED_CACHE
    );

    let file_root = setup_file_area(cli.file_root, &data_dir);

    let websocket_enabled = cli.websocket;
    let (
        listener,
        transfer_listener,
        ws_listener,
        ws_transfer_listener,
        (tls_acceptor, fingerprint),
    ) = setup_network(
        cli.bind,
        cli.port,
        cli.transfer_port,
        if websocket_enabled {
            Some(cli.websocket_port)
        } else {
            None
        },
        if websocket_enabled {
            Some(cli.transfer_websocket_port)
        } else {
            None
        },
        &data_dir,
    )
    .await;

    // Voice DTLS shares the TCP port (OS routes by protocol).
    let voice_addr = SocketAddr::new(cli.bind, cli.port);
    let cert_path = data_dir.join(CERT_FILENAME);
    let key_path = data_dir.join(KEY_FILENAME);
    let voice_listener = match create_voice_listener(voice_addr, &cert_path, &key_path).await {
        Ok(listener) => {
            info!("{}{}", MSG_VOICE_LISTENING, voice_addr);
            Some(listener)
        }
        Err(e) => {
            warn!(err = %e, "{}", LOG_VOICE_DTLS_FAILED);
            warn!("{}", LOG_VOICE_UNAVAILABLE);
            None
        }
    };

    let transfer_port = cli.transfer_port;
    let transfer_websocket_port = if websocket_enabled {
        Some(cli.transfer_websocket_port)
    } else {
        None
    };

    let upnp_handle = setup_upnp(
        cli.upnp,
        cli.bind,
        cli.port,
        transfer_port,
        if websocket_enabled {
            Some(cli.websocket_port)
        } else {
            None
        },
        transfer_websocket_port,
    )
    .await;

    let chat_burst_limit = database.config.get_chat_burst_limit().await;
    let chat_rate_limit = database.config.get_chat_rate_limit().await;
    let flood_config = Arc::new(FloodConfig::new(chat_burst_limit, chat_rate_limit));

    // Per-IP failed-login limiter, shared by the BBS and transfer ports.
    // Successes never debit; trusted IPs bypass it entirely.
    let login_limiter = Arc::new(
        RateLimiter::with_burst_and_refill(LOGIN_FAILURE_BURST, LOGIN_FAILURE_REFILL_PER_MINUTE)
            .key_ipv6_by_prefix(),
    );
    {
        let login_limiter = Arc::clone(&login_limiter);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(LOGIN_LIMITER_GC_INTERVAL);
            loop {
                interval.tick().await;
                login_limiter.gc(LOGIN_LIMITER_GC_IDLE_TTL);
            }
        });
    }
    let egress_config = database.config.get_all().await;
    let egress_chunk_size = resolve_egress_chunk_size(egress_config.scheduler_chunk_size);
    let (egress_handle, egress_task) = egress::task::EgressTask::channel_with_rate(
        egress::EgressManager::new(egress_chunk_size),
        egress_config.max_outbound_rate,
    );
    let _egress_task = tokio::spawn(egress_task.run());
    let egress_connection_ids = Arc::new(AtomicU64::new(1));

    // Leak to a 'static reference — lives for the program lifetime anyway.
    let fingerprint: &'static str = Box::leak(fingerprint.into_boxed_str());

    let max_connections_per_ip = database.config.get_max_connections_per_ip().await;
    let max_transfers_per_ip = database.config.get_max_transfers_per_ip().await;
    let connection_tracker = Arc::new(ConnectionTracker::new(
        max_connections_per_ip,
        max_transfers_per_ip,
    ));

    let shutdown_signal = setup_shutdown_signal();

    // Leak to a 'static reference — lives for the program lifetime anyway.
    let file_root: &'static Path = Box::leak(file_root.into_boxed_path());

    let file_index = Arc::new(FileIndex::new(&data_dir, file_root));
    file_index.trigger_reindex();

    let file_activity = Arc::new(FileActivityMap::new());

    let transfer_registry = Arc::new(TransferRegistry::new());
    let voice_registry = VoiceRegistry::new();
    let (voice_control, voice_control_rx) = VoiceControlHandle::channel();

    // `bootstrap()` loads enabled tracker rows and spawns one task per row;
    // task-level connection failures back off and retry, never fail startup.
    let tracker_context = Arc::new(tracker::TrackerContext {
        db: database.clone(),
        user_manager: user_manager.clone(),
        server_fingerprint: fingerprint,
        server_port: cli.port,
        server_websocket_port: if cli.websocket {
            Some(cli.websocket_port)
        } else {
            None
        },
    });
    let tracker_manager = Arc::new(tracker::TrackerManager::new(tracker_context));
    if let Err(e) = tracker_manager.bootstrap().await {
        error!(err = %e, "{}", ERR_TRACKER_BOOTSTRAP_FAILED);
        std::process::exit(1);
    }

    // Needed by the voice server for broadcasts.
    let channel_manager = ChannelManager::new(database.channels.clone(), user_manager.clone());

    // Dead-session reaper: drains sessions that broadcasts find unreachable and
    // runs the canonical disconnect teardown for them, off any handler's
    // `read_user_state` guard. See `handlers::reaper`.
    match user_manager.take_dead_session_rx() {
        Some(dead_session_rx) => {
            tokio::spawn(handlers::reaper::run_dead_session_reaper(
                dead_session_rx,
                user_manager.clone(),
                voice_registry.clone(),
                channel_manager.clone(),
            ));
        }
        None => error!("{}", LOG_DEAD_SESSION_REAPER_NO_RECEIVER),
    }

    let voice_server = voice_listener.map(|listener| {
        Arc::new(VoiceUdpServer::new(
            listener,
            voice_registry.clone(),
            ip_rule_cache.clone(),
            user_manager.clone(),
            channel_manager.clone(),
            connection_tracker.clone(),
            voice_control_rx,
        ))
    });

    let persistent_channels_config = database.config.get_persistent_channels().await;
    let channel_names = db::ConfigDb::parse_channel_list(&persistent_channels_config);
    if !channel_names.is_empty() {
        let mut channels_to_init = Vec::new();
        for name in &channel_names {
            match database.channels.get_channel_settings(name).await {
                Ok(Some(settings)) => {
                    let (topic, topic_set_by) = if settings.topic.is_empty() {
                        (None, None)
                    } else {
                        (Some(settings.topic), Some(settings.topic_set_by))
                    };
                    channels_to_init.push(Channel::with_settings(
                        name.to_string(),
                        topic,
                        topic_set_by,
                        settings.secret,
                    ));
                }
                Ok(None) => {
                    // In config but not in DB — create default settings.
                    if let Err(e) = database
                        .channels
                        .upsert_channel_settings(&db::channels::ChannelSettings {
                            name: name.to_string(),
                            topic: String::new(),
                            topic_set_by: String::new(),
                            secret: false,
                        })
                        .await
                    {
                        error!(channel = %name, err = %e, "{}", LOG_CHANNEL_SETTINGS_CREATE_FAILED);
                    }
                    channels_to_init.push(Channel::new(name.to_string()));
                }
                Err(e) => {
                    error!(channel = %name, err = %e, "{}", LOG_CHANNEL_SETTINGS_LOAD_FAILED);
                    channels_to_init.push(Channel::new(name.to_string()));
                }
            }
        }

        // Prune DB rows for channels no longer in config.
        if let Ok(all_settings) = database.channels.get_all_channel_settings().await {
            for settings in all_settings {
                let name_lower = fold_name(&settings.name);
                if !channel_names.iter().any(|n| fold_name(n) == name_lower) {
                    if let Err(e) = database
                        .channels
                        .delete_channel_settings(&settings.name)
                        .await
                    {
                        error!(channel = %settings.name, err = %e, "{}", LOG_CHANNEL_SETTINGS_DELETE_FAILED);
                    } else {
                        debug!(channel = %settings.name, "{}", LOG_CHANNEL_SETTINGS_PRUNED);
                    }
                }
            }
        }

        channel_manager
            .initialize_persistent_channels(channels_to_init)
            .await;
        debug!(count = channel_names.len(), "{}", LOG_CHANNELS_INITIALIZED);
    }

    let file_index_for_timer = file_index.clone();
    let database_for_timer = database.clone();

    // Daily log-retention purge. `None` (no task) when file logging is disabled.
    let log_purge_task = logging::spawn_purge_task(
        data_dir.clone(),
        cli.log_level,
        cli.log_retention,
        LOG_FILE_PREFIX.to_string(),
    );

    // Params builders shared by the TCP and WebSocket arms of each port.
    // By-ref captures make these closures `Copy`, so both arms of a port
    // take their own copy; each accepted connection clones the shared
    // handles and allocates its egress connection id here.
    let make_connection_params = |peer_addr| ConnectionParams {
        peer_addr,
        user_manager: user_manager.clone(),
        db: database.clone(),
        file_root: Some(file_root),
        transfer_port,
        transfer_websocket_port,
        connection_tracker: connection_tracker.clone(),
        ip_rule_cache: ip_rule_cache.clone(),
        file_index: file_index.clone(),
        file_activity: file_activity.clone(),
        channel_manager: channel_manager.clone(),
        transfer_registry: transfer_registry.clone(),
        voice_registry: voice_registry.clone(),
        voice_control: voice_control.clone(),
        tracker_manager: tracker_manager.clone(),
        fingerprint,
        flood_config: flood_config.clone(),
        login_limiter: login_limiter.clone(),
        egress: egress_handle.clone(),
        egress_connection_id: allocate_egress_connection_id(&egress_connection_ids),
        lan_egress_bypass_enabled: true,
    };
    let make_transfer_params = |peer_addr| TransferParams {
        peer_addr,
        db: database.clone(),
        file_root: Some(file_root),
        file_index: file_index.clone(),
        file_activity: file_activity.clone(),
        transfer_registry: transfer_registry.clone(),
        ip_rule_cache: ip_rule_cache.clone(),
        login_limiter: login_limiter.clone(),
        user_manager: user_manager.clone(),
        fingerprint,
        egress: egress_handle.clone(),
        egress_connection_id: allocate_egress_connection_id(&egress_connection_ids),
        lan_egress_bypass_enabled: true,
    };

    // Background tasks needing explicit teardown. The `select!` arms
    // (accept loops, voice server, reindex timer) auto-drop on return,
    // so they aren't bundled here.
    let handles = DaemonHandles {
        log_purge: log_purge_task,
        tracker: tracker_manager.clone(),
        upnp: upnp_handle,
    };

    tokio::select! {
        _ = shutdown_signal => {
            info!("{}", MSG_SHUTDOWN_RECEIVED);
            handles.shutdown().await;
        }
        // Main BBS port accept loop.
        _ = run_accept_loop(
            &listener,
            &tls_acceptor,
            &ip_rule_cache,
            |ip| connection_tracker.try_acquire(ip),
            make_connection_params,
            connection::handle_connection,
            LOG_REJECTED_BANNED_IP,
        ) => {}
        // Transfer port accept loop.
        _ = run_accept_loop(
            &transfer_listener,
            &tls_acceptor,
            &ip_rule_cache,
            |ip| connection_tracker.try_acquire_transfer(ip),
            make_transfer_params,
            transfers::handle_transfer_connection,
            LOG_REJECTED_BANNED_IP_TRANSFER,
        ) => {}
        // WebSocket BBS port accept loop (only if enabled).
        _ = async {
            match &ws_listener {
                // Disabled: park forever so this select! arm never fires.
                None => std::future::pending::<()>().await,
                Some(ws_listener) => {
                    run_accept_loop(
                        ws_listener,
                        &tls_acceptor,
                        &ip_rule_cache,
                        |ip| connection_tracker.try_acquire(ip),
                        make_connection_params,
                        websocket::handle_websocket_connection,
                        LOG_REJECTED_BANNED_IP_WS,
                    )
                    .await
                }
            }
        } => {}
        // WebSocket transfer port accept loop (only if enabled).
        _ = async {
            match &ws_transfer_listener {
                // Disabled: park forever so this select! arm never fires.
                None => std::future::pending::<()>().await,
                Some(ws_transfer_listener) => {
                    run_accept_loop(
                        ws_transfer_listener,
                        &tls_acceptor,
                        &ip_rule_cache,
                        |ip| connection_tracker.try_acquire_transfer(ip),
                        make_transfer_params,
                        websocket::handle_websocket_transfer_connection,
                        LOG_REJECTED_BANNED_IP_WS_TRANSFER,
                    )
                    .await
                }
            }
        } => {}
        // Voice UDP server (DTLS).
        _ = async {
            let Some(server) = voice_server else {
                // Listener failed to create: park forever so this arm never fires.
                std::future::pending::<()>().await;
                return;
            };
            server.run().await;
        } => {}
        // File reindex timer task.
        _ = async {
            loop {
                // Re-read interval each cycle so runtime config changes apply.
                let interval_minutes = database_for_timer.config.get_file_reindex_interval().await;

                if interval_minutes == 0 {
                    // Disabled: re-check in a minute.
                    tokio::time::sleep(Duration::from_secs(60)).await;
                    continue;
                }

                tokio::time::sleep(Duration::from_secs(u64::from(interval_minutes) * 60)).await;

                // Reindex if dirty or stale from external changes.
                if !file_index_for_timer.is_reindexing() {
                    if file_index_for_timer.is_dirty() {
                        debug!("{}", LOG_FILE_INDEX_DIRTY);
                        file_index_for_timer.trigger_reindex();
                    } else if file_index_for_timer.is_stale(FILE_INDEX_MAX_AGE) {
                        debug!("{}", LOG_FILE_INDEX_STALE);
                        file_index_for_timer.trigger_reindex();
                    }
                }
            }
        } => {}
    }
}

/// Create the data directory and lock it owner-only on Unix — it hosts the
/// database, TLS key, and logs, so a loose parent undercuts per-file modes.
///
/// `DirBuilder::mode` sets the mode atomically at creation (no world-readable
/// window); the later `set_permissions` corrects a pre-existing wrong mode.
fn ensure_data_dir(data_dir: &Path) -> Result<(), String> {
    let create_result = {
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            let mut builder = fs::DirBuilder::new();
            builder.recursive(true);
            builder.mode(nexus_common::DATA_DIR_MODE);
            builder.create(data_dir)
        }
        #[cfg(not(unix))]
        {
            fs::create_dir_all(data_dir)
        }
    };
    create_result.map_err(|e| format!("{}{}", ERR_CREATE_DATA_DIR, e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            data_dir,
            fs::Permissions::from_mode(nexus_common::DATA_DIR_MODE),
        )
        .map_err(|e| format!("{}{}", ERR_SET_DATA_DIR_PERMS, e))?;
    }
    Ok(())
}

async fn setup_db(db_path: &Path) -> (db::Database, UserManager) {
    let pool = match db::init_db(db_path).await {
        Ok(pool) => pool,
        Err(e) => {
            error!("{}{}", ERR_DATABASE_INIT, e);
            std::process::exit(1);
        }
    };
    info!("{}{}", MSG_DATABASE, db_path.display());

    // SQLx creates the file with the process umask; chmod down to owner-only.
    // Unix only — Windows uses NTFS ACLs.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = fs::set_permissions(
            db_path,
            fs::Permissions::from_mode(nexus_common::secure_file::SECURE_FILE_MODE),
        ) {
            error!("{}{}", ERR_SET_PERMISSIONS, e);
            std::process::exit(1);
        }
    }

    let database = db::Database::new(pool);
    let user_manager = UserManager::new();

    (database, user_manager)
}

fn resolve_egress_chunk_size(configured: u32) -> NonZeroUsize {
    if validate_bandwidth_chunk_size(configured).is_ok()
        && let Some(chunk_size) = NonZeroUsize::new(configured as usize)
    {
        return chunk_size;
    }

    warn!(
        configured,
        min = MIN_BANDWIDTH_CHUNK_SIZE,
        max = MAX_BANDWIDTH_CHUNK_SIZE,
        "{}",
        LOG_EGRESS_CHUNK_SIZE_INVALID
    );
    NonZeroUsize::new(DEFAULT_BANDWIDTH_CHUNK_SIZE as usize).unwrap_or(NonZeroUsize::MIN)
}

fn allocate_egress_connection_id(counter: &AtomicU64) -> scheduler::ConnectionId {
    scheduler::ConnectionId::new(counter.fetch_add(1, Ordering::Relaxed))
}

/// Standalone-spawned background tasks bundled so shutdown is one
/// `handles.shutdown().await` call. The `select!` arms (accept loops, voice
/// server, reindex timer) auto-drop on return and need no handle.
struct DaemonHandles {
    /// `None` when file logging is disabled (level == None or retention == 0).
    log_purge: Option<tokio::task::JoinHandle<()>>,
    /// Shared with every connection (handlers call in); shutdown aborts and
    /// joins every per-tracker task.
    tracker: Arc<tracker::TrackerManager>,
    /// Gateway + lease-renewal task. `None` without `--upnp` or on setup
    /// failure. Last because port-mapping removal is shutdown's only
    /// async/fallible step.
    upnp: Option<(Arc<upnp::Gateway>, tokio::task::JoinHandle<()>)>,
}

impl DaemonHandles {
    /// Stop every task and release external resources (UPnP mapping).
    /// Teardown is best-effort — the daemon is going down anyway.
    async fn shutdown(self) {
        if let Some(handle) = self.log_purge {
            handle.abort();
        }
        self.tracker.shutdown().await;
        if let Some((gateway, renewal_task)) = self.upnp {
            renewal_task.abort();
            if let Err(e) = gateway.remove_port_mapping().await {
                warn!(err = %e, "{}", LOG_UPNP_REMOVE_FAILED);
            }
        }
    }
}

async fn setup_upnp(
    enabled: bool,
    bind: std::net::IpAddr,
    main_port: u16,
    transfer_port: u16,
    websocket_port: Option<u16>,
    transfer_websocket_port: Option<u16>,
) -> Option<(Arc<upnp::Gateway>, tokio::task::JoinHandle<()>)> {
    if !enabled {
        return None;
    }

    match upnp::setup(
        bind,
        main_port,
        transfer_port,
        websocket_port,
        transfer_websocket_port,
    )
    .await
    {
        Ok(gateway) => {
            let gateway_arc = Arc::new(gateway);
            let renewal_task = upnp::spawn_lease_renewal_task(gateway_arc.clone());
            Some((gateway_arc, renewal_task))
        }
        Err(e) => {
            warn!(err = %e, "{}", LOG_UPNP_SETUP_FAILED);
            warn!("{}", MSG_UPNP_CONTINUE);
            warn!("{}", MSG_UPNP_MANUAL);
            None
        }
    }
}

async fn setup_network(
    bind: std::net::IpAddr,
    port: u16,
    transfer_port: u16,
    websocket_port: Option<u16>,
    transfer_websocket_port: Option<u16>,
    data_dir: &Path,
) -> (
    TcpListener,
    TcpListener,
    Option<TcpListener>,
    Option<TcpListener>,
    (TlsAcceptor, String),
) {
    // Generate a self-signed cert/key on first run and compute the SHA-256
    // fingerprint reported in the handshake.
    let tls_config = nexus_common::tls::TlsCertConfig {
        cert_filename: CERT_FILENAME,
        key_filename: KEY_FILENAME,
        common_name: TLS_CERT_COMMON_NAME,
    };
    let fingerprint = match nexus_common::tls::ensure_cert(data_dir, tls_config) {
        Ok(fp) => fp,
        Err(e) => {
            error!("{}{}", ERR_TLS_INIT, e);
            std::process::exit(1);
        }
    };
    let tls_acceptor = match nexus_common::tls::build_acceptor(data_dir, tls_config) {
        Ok(acceptor) => acceptor,
        Err(e) => {
            error!("{}{}", ERR_TLS_INIT, e);
            std::process::exit(1);
        }
    };
    info!("{}{}", MSG_CERTIFICATES, data_dir.display());

    let addr = SocketAddr::new(bind, port);
    let listener = match TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(e) => {
            error!("{}{}: {}", ERR_BIND_FAILED, addr, e);
            std::process::exit(1);
        }
    };
    info!("{}{}", MSG_LISTENING, addr);

    let transfer_addr = SocketAddr::new(bind, transfer_port);
    let transfer_listener = match TcpListener::bind(transfer_addr).await {
        Ok(listener) => listener,
        Err(e) => {
            error!("{}{}: {}", ERR_BIND_FAILED, transfer_addr, e);
            std::process::exit(1);
        }
    };
    info!("{}{}", MSG_TRANSFER_LISTENING, transfer_addr);

    let (ws_listener, ws_transfer_listener) = if let (Some(ws_port), Some(ws_transfer_port)) =
        (websocket_port, transfer_websocket_port)
    {
        let ws_addr = SocketAddr::new(bind, ws_port);
        let ws_listener = match TcpListener::bind(ws_addr).await {
            Ok(listener) => listener,
            Err(e) => {
                error!("{}{}: {}", ERR_BIND_FAILED, ws_addr, e);
                std::process::exit(1);
            }
        };
        info!("{}{}", MSG_WS_LISTENING, ws_addr);

        let ws_transfer_addr = SocketAddr::new(bind, ws_transfer_port);
        let ws_transfer_listener = match TcpListener::bind(ws_transfer_addr).await {
            Ok(listener) => listener,
            Err(e) => {
                error!("{}{}: {}", ERR_BIND_FAILED, ws_transfer_addr, e);
                std::process::exit(1);
            }
        };
        info!("{}{}", MSG_WS_TRANSFER_LISTENING, ws_transfer_addr);

        (Some(ws_listener), Some(ws_transfer_listener))
    } else {
        (None, None)
    };

    (
        listener,
        transfer_listener,
        ws_listener,
        ws_transfer_listener,
        (tls_acceptor, fingerprint),
    )
}

/// Returns the canonicalized file area root, ready for `resolve_path()` and
/// other security-sensitive operations.
fn setup_file_area(file_root: Option<std::path::PathBuf>, data_dir: &Path) -> std::path::PathBuf {
    let root = file_root.unwrap_or_else(|| files::default_file_root(data_dir));

    if let Err(e) = files::init_file_area(&root) {
        error!("{}{}", ERR_INIT_FILE_AREA, e);
        std::process::exit(1);
    }

    // Canonicalize for security: resolves symlinks to an absolute path for
    // the `starts_with()` checks in `resolve_path()`.
    let canonical_root = match root.canonicalize() {
        Ok(path) => path,
        Err(e) => {
            error!("{}{}", ERR_FILE_ROOT_CANONICALIZE, e);
            std::process::exit(1);
        }
    };

    info!("{}{}", MSG_FILE_ROOT, canonical_root.display());

    canonical_root
}

/// One admission pipeline for every listener port: accept → fold
/// IPv4-mapped IPv6 to IPv4 (so all downstream consumers — sessions,
/// tracker, cache — see one form) → per-IP guard or drop → build params →
/// spawn { hold the guard for the connection's life, pre-TLS IP-rule check
/// (trust bypasses ban), run the handler, classify the error } →
/// accept-error backoff. The four ports differ only in the pieces passed
/// in, so the security-critical sequence lives exactly once.
async fn run_accept_loop<G, P, MakeParams, Handler, Fut>(
    listener: &TcpListener,
    tls_acceptor: &TlsAcceptor,
    ip_rule_cache: &Arc<IpRuleState>,
    acquire_guard: impl Fn(IpAddr) -> Option<G>,
    make_params: MakeParams,
    handler: Handler,
    rejected_log: &'static str,
) where
    G: Send + 'static,
    P: Send + 'static,
    MakeParams: Fn(SocketAddr) -> P,
    Handler: Fn(TcpStream, TlsAcceptor, P) -> Fut + Copy + Send + 'static,
    Fut: Future<Output = io::Result<()>> + Send + 'static,
{
    loop {
        match listener.accept().await {
            Ok((socket, peer_addr)) => {
                let peer_addr = normalize_socket_addr(peer_addr);
                let Some(guard) = acquire_guard(peer_addr.ip()) else {
                    debug!(ip = %peer_addr.ip(), "{}", LOG_CONNECTION_LIMIT);
                    // Drop the socket — client sees a connection reset.
                    continue;
                };

                let params = make_params(peer_addr);
                let tls_acceptor = tls_acceptor.clone();
                let ip_rule_cache = ip_rule_cache.clone();

                tokio::spawn(async move {
                    // Hold guard until connection ends.
                    let _guard = guard;

                    // Check IP rules before the TLS handshake (trust bypasses ban).
                    if !ip_rule_cache.should_allow(peer_addr.ip()).await {
                        debug!(ip = %peer_addr.ip(), "{}", rejected_log);
                        return;
                    }

                    if let Err(e) = handler(socket, tls_acceptor, params).await {
                        log_connection_error(&e, peer_addr);
                    }
                });
            }
            Err(e) => {
                warn!(err = %e, "{}", LOG_ACCEPT_ERROR);
                tokio::time::sleep(nexus_common::ACCEPT_ERROR_BACKOFF).await;
            }
        }
    }
}

/// Log connection errors, dropping benign noise (abrupt close_notify) and
/// demoting TLS/WebSocket handshake failures (scanners, probes) to debug.
fn log_connection_error(error: &io::Error, peer_addr: SocketAddr) {
    let error_msg = error.to_string();

    // Benign: client disconnected abruptly.
    if error_msg.contains(TLS_CLOSE_NOTIFY_MSG) {
        return;
    }

    if error_msg.contains(nexus_common::TLS_HANDSHAKE_FAILED_PREFIX) {
        debug!(ip = %peer_addr, err = %error, "{}", LOG_CONNECTION_ERROR_TLS);
        return;
    }

    if error_msg.contains(nexus_common::WS_HANDSHAKE_FAILED_PREFIX) {
        debug!(ip = %peer_addr, err = %error, "{}", LOG_CONNECTION_ERROR_WS);
        return;
    }

    error!(ip = %peer_addr, err = %error, "{}", LOG_CONNECTION_ERROR);
}

/// Wait for a graceful shutdown signal (SIGTERM/SIGINT on Unix, Ctrl+C elsewhere).
async fn setup_shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};

        let mut sigterm = signal(SignalKind::terminate()).expect(ERR_SIGNAL_SIGTERM);
        let mut sigint = signal(SignalKind::interrupt()).expect(ERR_SIGNAL_SIGINT);

        tokio::select! {
            _ = sigterm.recv() => {},
            _ = sigint.recv() => {},
        }
    }

    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c().await.expect(ERR_SIGNAL_CTRLC);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_egress_chunk_size_accepts_valid_bounds() {
        assert_eq!(
            resolve_egress_chunk_size(MIN_BANDWIDTH_CHUNK_SIZE).get(),
            MIN_BANDWIDTH_CHUNK_SIZE as usize
        );
        assert_eq!(
            resolve_egress_chunk_size(DEFAULT_BANDWIDTH_CHUNK_SIZE).get(),
            DEFAULT_BANDWIDTH_CHUNK_SIZE as usize
        );
        assert_eq!(
            resolve_egress_chunk_size(MAX_BANDWIDTH_CHUNK_SIZE).get(),
            MAX_BANDWIDTH_CHUNK_SIZE as usize
        );
    }

    #[test]
    fn resolve_egress_chunk_size_rejects_invalid_values() {
        for configured in [
            0,
            MIN_BANDWIDTH_CHUNK_SIZE - 1,
            MAX_BANDWIDTH_CHUNK_SIZE + 1,
            u32::MAX,
        ] {
            assert_eq!(
                resolve_egress_chunk_size(configured).get(),
                DEFAULT_BANDWIDTH_CHUNK_SIZE as usize
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn test_ensure_data_dir_creates_fresh_with_owner_only_mode() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().expect("create tempdir");
        let data_dir = tmp.path().join("data");

        ensure_data_dir(&data_dir).expect("ensure_data_dir");

        let mode = std::fs::metadata(&data_dir)
            .expect("read metadata")
            .permissions()
            .mode();
        assert_eq!(
            mode & 0o777,
            nexus_common::DATA_DIR_MODE,
            "fresh data dir should be created with 0o700"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_ensure_data_dir_corrects_pre_existing_loose_mode() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().expect("create tempdir");
        let data_dir = tmp.path().join("data");

        // Pre-create with world-readable perms to simulate a wrongly-
        // permissioned data directory left over from a previous run.
        std::fs::create_dir(&data_dir).expect("pre-create dir");
        std::fs::set_permissions(&data_dir, std::fs::Permissions::from_mode(0o755))
            .expect("set initial perms");

        ensure_data_dir(&data_dir).expect("ensure_data_dir");

        let mode = std::fs::metadata(&data_dir)
            .expect("read metadata")
            .permissions()
            .mode();
        assert_eq!(
            mode & 0o777,
            nexus_common::DATA_DIR_MODE,
            "pre-existing loose data dir should be corrected to 0o700"
        );
    }
}
