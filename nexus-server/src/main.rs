//! Nexus BBS Server

mod args;
mod channels;
mod connection;
mod connection_tracker;
mod constants;
mod db;
mod files;
mod flood;
mod handlers;
mod i18n;
mod ip_rule_cache;
mod tracker;
mod transfers;
mod upnp;
mod users;
mod voice;
mod websocket;

use std::fs;
use std::io;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use clap::Parser;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tracing::{debug, error, info, warn};

use args::Cli;
use channels::{Channel, ChannelManager};
use connection::ConnectionParams;
use connection_tracker::ConnectionTracker;
use constants::*;
use files::{FileIndex, PathLockMap};
use flood::FloodConfig;
use ip_rule_cache::IpRuleCache;
use nexus_common::address::normalize_socket_addr;
use nexus_common::logging::{self, LogInitParams, LogLevel};
use transfers::{TransferParams, TransferRegistry};
use users::UserManager;
use voice::{VoiceRegistry, VoiceUdpServer, create_voice_listener};

#[tokio::main]
async fn main() {
    // Install rustls crypto provider before any TLS/DTLS operations
    // This is required because both tokio-rustls and dtls use rustls 0.23
    // which needs an explicit crypto provider selection
    tokio_rustls::rustls::crypto::ring::default_provider()
        .install_default()
        .expect(ERR_RUSTLS_PROVIDER);

    let mut cli = Cli::parse();

    // Resolve data directory (CLI override or platform default).
    let data_dir = resolve_data_dir(cli.data_dir.take());

    // Initialize logging (global log level + tracing subscriber).
    if let Err(e) = logging::init(LogInitParams {
        data_dir: &data_dir,
        level: cli.log_level,
        retention: cli.log_retention,
        no_timestamps: cli.no_log_timestamps,
        log_file_prefix: LOG_FILE_PREFIX,
    }) {
        // `init` installs a stderr-only fallback subscriber before
        // returning Err, so this warning still reaches the user.
        warn!(err = %e, "{}", LOG_LOGGING_INIT_FAILED);
    }

    // Create the data directory if needed and lock it to owner-only on Unix.
    if let Err(e) = ensure_data_dir(&data_dir) {
        error!("{}", e);
        std::process::exit(1);
    }

    // Resolve the log directory path once and reuse for purge, startup
    // info, and the daily timer task below.
    let log_dir = logging::log_dir(&data_dir);

    // Purge old log files on startup. `purge_old_logs` no-ops when retention
    // is zero, so no outer gate needed.
    logging::purge_old_logs(&log_dir, cli.log_retention, LOG_FILE_PREFIX);

    // Startup info
    info!("{}{}", MSG_BANNER, env!("CARGO_PKG_VERSION"));
    info!("{}{}", MSG_LOG_LEVEL, cli.log_level);
    if cli.log_level != LogLevel::None && cli.log_retention > std::time::Duration::ZERO {
        info!("{}{}", MSG_LOG_DIR, log_dir.display());
    }

    // Setup database
    let db_path = db::database_path(&data_dir);
    let (database, user_manager) = setup_db(&db_path).await;

    // Setup IP rule cache - cleanup expired entries, then load active ones
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
    let ip_rule_cache = Arc::new(RwLock::new(IpRuleCache::from_records(
        ban_records,
        trust_records,
    )));
    debug!(
        bans = ban_count,
        trusts = trust_count,
        "{}",
        LOG_LOADED_CACHE
    );

    // Setup file area
    let file_root = setup_file_area(cli.file_root, &data_dir);

    // Setup network (TCP listeners + TLS, optionally WebSocket listeners)
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

    // Setup voice DTLS listener (same port as TCP, OS routes by protocol).
    // Certificates live directly in the data directory.
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

    // Store transfer ports for ServerInfo
    let transfer_port = cli.transfer_port;
    let transfer_websocket_port = if websocket_enabled {
        Some(cli.transfer_websocket_port)
    } else {
        None
    };

    // Setup UPnP port forwarding if requested (forwards WS ports only if enabled)
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

    // Setup flood protection config (load limits from database)
    let chat_burst_limit = database.config.get_chat_burst_limit().await;
    let chat_rate_limit = database.config.get_chat_rate_limit().await;
    let flood_config = Arc::new(FloodConfig::new(chat_burst_limit, chat_rate_limit));

    // Leak fingerprint to get a 'static reference - it lives for the program lifetime anyway
    let fingerprint: &'static str = Box::leak(fingerprint.into_boxed_str());

    // Setup connection tracking for DoS protection (load limits from database)
    let max_connections_per_ip = database.config.get_max_connections_per_ip().await;
    let max_transfers_per_ip = database.config.get_max_transfers_per_ip().await;
    let connection_tracker = Arc::new(ConnectionTracker::new(
        max_connections_per_ip,
        max_transfers_per_ip,
    ));

    // Setup graceful shutdown handling
    let shutdown_signal = setup_shutdown_signal();

    // Leak the PathBuf to get a 'static reference - it lives for the program lifetime anyway
    let file_root: &'static Path = Box::leak(file_root.into_boxed_path());

    // Setup file index for searching
    let file_index = Arc::new(FileIndex::new(&data_dir, file_root));

    // Trigger initial index build in background
    file_index.trigger_reindex();

    // Shared between BBS handlers and transfer-port upload finalization.
    let file_mutation_locks = Arc::new(PathLockMap::new());

    // Create transfer registry for tracking active transfers (enables ban disconnection)
    let transfer_registry = Arc::new(TransferRegistry::new());

    // Create voice registry for tracking active voice sessions (ephemeral, in-memory only)
    let voice_registry = VoiceRegistry::new();

    // Create the tracker registration manager. The bootstrap call below
    // loads enabled tracker rows from the DB and spawns one tracker
    // task per row. Connection failures inside tasks just transition
    // them to backoff/retry — they don't fail startup.
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

    // Create channel manager for multi-channel chat (needed by voice server for broadcasts)
    let channel_manager = ChannelManager::new(database.channels.clone(), user_manager.clone());

    // Create voice UDP server if listener was created successfully
    let voice_server = voice_listener.map(|listener| {
        Arc::new(VoiceUdpServer::new(
            listener,
            voice_registry.clone(),
            ip_rule_cache.clone(),
            user_manager.clone(),
            channel_manager.clone(),
        ))
    });

    // Initialize persistent channels from config and database
    let persistent_channels_config = database.config.get_persistent_channels().await;
    let channel_names = db::ConfigDb::parse_channel_list(&persistent_channels_config);
    if !channel_names.is_empty() {
        let mut channels_to_init = Vec::new();
        for name in &channel_names {
            // Load settings from DB if they exist, otherwise create defaults
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
                    // Channel in config but not in DB - create default settings
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

        // Prune channels from DB that are no longer in config
        if let Ok(all_settings) = database.channels.get_all_channel_settings().await {
            for settings in all_settings {
                let name_lower = settings.name.to_lowercase();
                if !channel_names.iter().any(|n| n.to_lowercase() == name_lower) {
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

    // Clone for the timer task
    let file_index_for_timer = file_index.clone();
    let database_for_timer = database.clone();

    // Spawn the daily log-retention purge task. Returns `None` (no task)
    // when file logging is disabled — purge is a no-op in that case.
    let log_purge_task = logging::spawn_purge_task(
        data_dir.clone(),
        cli.log_level,
        cli.log_retention,
        LOG_FILE_PREFIX.to_string(),
    );

    // Bundle every background task that needs explicit teardown into
    // `DaemonHandles`. Adding a new background task means adding one
    // field plus one line in `shutdown()` — no inlined if-let block in
    // the `tokio::select!` shutdown arm. Accept loops, the voice
    // server, and the file-reindex timer live inside the `select!` and
    // are auto-dropped when the select! returns, so they aren't
    // bundled here.
    let handles = DaemonHandles {
        log_purge: log_purge_task,
        tracker: tracker_manager.clone(),
        upnp: upnp_handle,
    };

    // Main server loops - accept incoming connections on both ports
    tokio::select! {
        _ = shutdown_signal => {
            info!("{}", MSG_SHUTDOWN_RECEIVED);
            handles.shutdown().await;
        }
        // Main BBS port accept loop
        _ = async {
            loop {
                match listener.accept().await {
                    Ok((socket, peer_addr)) => {
                        // Fold IPv4-mapped IPv6 to IPv4 once at the boundary so
                        // everything downstream (session storage, peer_addr
                        // comparisons, connection tracker, cache) sees one form.
                        let peer_addr = normalize_socket_addr(peer_addr);
                        // Check connection limit before accepting
                        let connection_guard = match connection_tracker.try_acquire(peer_addr.ip()) {
                            Some(guard) => guard,
                            None => {
                                debug!(ip = %peer_addr.ip(), "{}", LOG_CONNECTION_LIMIT);
                                // Just drop the socket - client will see connection reset
                                continue;
                            }
                        };

                        let params = ConnectionParams {
                            peer_addr,
                            user_manager: user_manager.clone(),
                            db: database.clone(),
                            file_root: Some(file_root),
                            transfer_port,
                            transfer_websocket_port,
                            connection_tracker: connection_tracker.clone(),
                            ip_rule_cache: ip_rule_cache.clone(),
                            file_index: file_index.clone(),
                            file_mutation_locks: file_mutation_locks.clone(),
                            channel_manager: channel_manager.clone(),
                            transfer_registry: transfer_registry.clone(),
                            voice_registry: voice_registry.clone(),
                            tracker_manager: tracker_manager.clone(),
                            fingerprint,
                            flood_config: flood_config.clone(),
                        };
                        let tls_acceptor = tls_acceptor.clone();

                        // Clone IP rule cache for pre-TLS check
                        let ip_rule_cache_for_check = ip_rule_cache.clone();

                        // Spawn a new task to handle this connection
                        tokio::spawn(async move {
                            // Hold guard until connection ends to track active connections
                            let _guard = connection_guard;

                            // Check IP rules BEFORE TLS handshake (saves resources)
                            // Trust list bypasses ban list
                            //
                            // Optimization: Use read lock for the check, only upgrade to
                            // write lock if expired entries need to be cleaned up.
                            let should_allow = {
                                let cache = ip_rule_cache_for_check
                                    .read()
                                    .expect(ERR_IP_CACHE_POISONED);
                                if cache.needs_rebuild() {
                                    // Drop read lock before acquiring write lock
                                    drop(cache);
                                    ip_rule_cache_for_check
                                        .write()
                                        .expect(ERR_IP_CACHE_POISONED)
                                        .should_allow(peer_addr.ip())
                                } else {
                                    cache.should_allow_read_only(peer_addr.ip())
                                }
                            };

                            if !should_allow {
                                // IP is banned (and not trusted) - silently close connection
                                // No TLS, no error message, no resources wasted
                                debug!(ip = %peer_addr.ip(), "{}", LOG_REJECTED_BANNED_IP);
                                return;
                            }

                            if let Err(e) =
                                connection::handle_connection(socket, tls_acceptor, params).await
                            {
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
        } => {}
        // Transfer port accept loop
        _ = async {
            loop {
                match transfer_listener.accept().await {
                    Ok((socket, peer_addr)) => {
                        let peer_addr = normalize_socket_addr(peer_addr);
                        // Check transfer connection limit before accepting
                        let transfer_guard = match connection_tracker.try_acquire_transfer(peer_addr.ip()) {
                            Some(guard) => guard,
                            None => {
                                debug!(ip = %peer_addr.ip(), "{}", LOG_CONNECTION_LIMIT);
                                // Just drop the socket - client will see connection reset
                                continue;
                            }
                        };

                        let params = TransferParams {
                            peer_addr,
                            db: database.clone(),
                            file_root: Some(file_root),
                            file_index: file_index.clone(),
                            file_mutation_locks: file_mutation_locks.clone(),
                            transfer_registry: transfer_registry.clone(),
                            fingerprint,
                        };
                        let tls_acceptor = tls_acceptor.clone();

                        // Clone IP rule cache for pre-TLS check
                        let ip_rule_cache_for_check = ip_rule_cache.clone();

                        tokio::spawn(async move {
                            let _guard = transfer_guard;

                            // Check IP rules BEFORE TLS handshake (saves resources)
                            // Trust list bypasses ban list
                            //
                            // Optimization: Use read lock for the check, only upgrade to
                            // write lock if expired entries need to be cleaned up.
                            let should_allow = {
                                let cache = ip_rule_cache_for_check
                                    .read()
                                    .expect(ERR_IP_CACHE_POISONED);
                                if cache.needs_rebuild() {
                                    // Drop read lock before acquiring write lock
                                    drop(cache);
                                    ip_rule_cache_for_check
                                        .write()
                                        .expect(ERR_IP_CACHE_POISONED)
                                        .should_allow(peer_addr.ip())
                                } else {
                                    cache.should_allow_read_only(peer_addr.ip())
                                }
                            };

                            if !should_allow {
                                // IP is banned (and not trusted) - silently close connection
                                debug!(ip = %peer_addr.ip(), "{}", LOG_REJECTED_BANNED_IP_TRANSFER);
                                return;
                            }

                            if let Err(e) =
                                transfers::handle_transfer_connection(socket, tls_acceptor, params)
                                    .await
                            {
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
        } => {}
        // WebSocket BBS port accept loop (only if enabled)
        _ = async {
            let Some(ref ws_listener) = ws_listener else {
                // WebSocket disabled, just wait forever
                std::future::pending::<()>().await;
                return;
            };
            loop {
                match ws_listener.accept().await {
                    Ok((socket, peer_addr)) => {
                        let peer_addr = normalize_socket_addr(peer_addr);
                        // Check connection limit before accepting (same limit as TCP)
                        let connection_guard = match connection_tracker.try_acquire(peer_addr.ip()) {
                            Some(guard) => guard,
                            None => {
                                debug!(ip = %peer_addr.ip(), "{}", LOG_CONNECTION_LIMIT);
                                continue;
                            }
                        };

                        let params = ConnectionParams {
                            peer_addr,
                            user_manager: user_manager.clone(),
                            db: database.clone(),
                            file_root: Some(file_root),
                            transfer_port,
                            transfer_websocket_port,
                            connection_tracker: connection_tracker.clone(),
                            ip_rule_cache: ip_rule_cache.clone(),
                            file_index: file_index.clone(),
                            file_mutation_locks: file_mutation_locks.clone(),
                            channel_manager: channel_manager.clone(),
                            transfer_registry: transfer_registry.clone(),
                            voice_registry: voice_registry.clone(),
                            tracker_manager: tracker_manager.clone(),
                            fingerprint,
                            flood_config: flood_config.clone(),
                        };
                        let tls_acceptor = tls_acceptor.clone();
                        let ip_rule_cache_for_check = ip_rule_cache.clone();

                        tokio::spawn(async move {
                            let _guard = connection_guard;

                            // Check IP rules BEFORE TLS handshake (same as TCP)
                            let should_allow = {
                                let cache = ip_rule_cache_for_check
                                    .read()
                                    .expect(ERR_IP_CACHE_POISONED);
                                if cache.needs_rebuild() {
                                    drop(cache);
                                    ip_rule_cache_for_check
                                        .write()
                                        .expect(ERR_IP_CACHE_POISONED)
                                        .should_allow(peer_addr.ip())
                                } else {
                                    cache.should_allow_read_only(peer_addr.ip())
                                }
                            };

                            if !should_allow {
                                debug!(ip = %peer_addr.ip(), "{}", LOG_REJECTED_BANNED_IP_WS);
                                return;
                            }

                            if let Err(e) =
                                websocket::handle_websocket_connection(socket, tls_acceptor, params)
                                    .await
                            {
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
        } => {}
        // WebSocket transfer port accept loop (only if enabled)
        _ = async {
            let Some(ref ws_transfer_listener) = ws_transfer_listener else {
                // WebSocket disabled, just wait forever
                std::future::pending::<()>().await;
                return;
            };
            loop {
                match ws_transfer_listener.accept().await {
                    Ok((socket, peer_addr)) => {
                        let peer_addr = normalize_socket_addr(peer_addr);
                        // Check transfer connection limit before accepting (same limit as TCP)
                        let transfer_guard = match connection_tracker.try_acquire_transfer(peer_addr.ip()) {
                            Some(guard) => guard,
                            None => {
                                debug!(ip = %peer_addr.ip(), "{}", LOG_CONNECTION_LIMIT);
                                continue;
                            }
                        };

                        let params = TransferParams {
                            peer_addr,
                            db: database.clone(),
                            file_root: Some(file_root),
                            file_index: file_index.clone(),
                            file_mutation_locks: file_mutation_locks.clone(),
                            transfer_registry: transfer_registry.clone(),
                            fingerprint,
                        };
                        let tls_acceptor = tls_acceptor.clone();
                        let ip_rule_cache_for_check = ip_rule_cache.clone();

                        tokio::spawn(async move {
                            let _guard = transfer_guard;

                            // Check IP rules BEFORE TLS handshake (same as TCP)
                            let should_allow = {
                                let cache = ip_rule_cache_for_check
                                    .read()
                                    .expect(ERR_IP_CACHE_POISONED);
                                if cache.needs_rebuild() {
                                    drop(cache);
                                    ip_rule_cache_for_check
                                        .write()
                                        .expect(ERR_IP_CACHE_POISONED)
                                        .should_allow(peer_addr.ip())
                                } else {
                                    cache.should_allow_read_only(peer_addr.ip())
                                }
                            };

                            if !should_allow {
                                debug!(ip = %peer_addr.ip(), "{}", LOG_REJECTED_BANNED_IP_WS_TRANSFER);
                                return;
                            }

                            if let Err(e) =
                                websocket::handle_websocket_transfer_connection(
                                    socket,
                                    tls_acceptor,
                                    params,
                                )
                                .await
                            {
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
        } => {}
        // Voice UDP server (DTLS)
        _ = async {
            let Some(server) = voice_server else {
                // Voice listener failed to create, just wait forever
                std::future::pending::<()>().await;
                return;
            };
            server.run().await;
        } => {}
        // File reindex timer task - checks config each minute
        _ = async {
            loop {
                // Re-read interval from DB each cycle (allows runtime changes)
                let interval_minutes = database_for_timer.config.get_file_reindex_interval().await;

                if interval_minutes == 0 {
                    // Disabled - sleep for 1 minute then check again
                    tokio::time::sleep(Duration::from_secs(60)).await;
                    continue;
                }

                // Sleep for the configured interval
                tokio::time::sleep(Duration::from_secs(u64::from(interval_minutes) * 60)).await;

                // Check if dirty (or stale from external changes) and not already reindexing
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

/// Resolve the server data directory, preferring the CLI override when set
/// and otherwise falling back to the platform default.
///
/// Panics only if the platform itself cannot supply a data directory
/// (`dirs::data_dir()` returns `None` — e.g., Windows without `%APPDATA%`,
/// Linux without `HOME`). This is platform-broken territory, not an
/// operator-actionable error.
fn resolve_data_dir(override_path: Option<std::path::PathBuf>) -> std::path::PathBuf {
    if let Some(p) = override_path {
        return p;
    }
    dirs::data_dir()
        .map(|d| d.join(DATA_DIR_NAME))
        .expect(ERR_NO_DATA_DIR)
}

/// Create the data directory if it doesn't already exist and lock it to
/// owner-only permissions (`nexus_common::DATA_DIR_MODE`) on Unix. The directory hosts
/// the database, TLS private key, and (by default) log files, so a
/// permissive parent directory undercuts the per-file protections inside.
///
/// On Unix, the mode is set atomically at creation via `DirBuilder::mode`
/// — there is no window where a fresh data directory is world-readable.
/// `set_permissions` is then applied to handle the case where the
/// directory pre-existed with the wrong mode.
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

/// Setup database connection and initialize user manager
async fn setup_db(db_path: &Path) -> (db::Database, UserManager) {
    // Initialize database connection pool and run migrations
    let pool = match db::init_db(db_path).await {
        Ok(pool) => pool,
        Err(e) => {
            error!("{}{}", ERR_DATABASE_INIT, e);
            std::process::exit(1);
        }
    };
    info!("{}{}", MSG_DATABASE, db_path.display());

    // Set secure permissions on the database file. SQLx creates it
    // through `init_db` above, but with the process's umask (typically
    // `0o644`); chmod down to owner-only here. Unix only — Windows
    // uses NTFS ACLs.
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

    // Create database and user manager instances
    // Note: SqlitePool uses Arc internally, so clone() is cheap
    let database = db::Database::new(pool);
    let user_manager = UserManager::new();

    (database, user_manager)
}

/// Background tasks and resources owned by the daemon for the lifetime
/// of the main `tokio::select!`. Bundled so the shutdown branch has a
/// single `handles.shutdown().await` call instead of inlined teardown
/// for each task.
///
/// Only standalone-spawned tasks live here. Accept loops, the voice
/// server, and the file-reindex timer run as `_ = async { … } => {}`
/// arms inside the `select!` and are auto-dropped when the `select!`
/// returns, so they don't need a handle.
struct DaemonHandles {
    /// Daily log-retention purge task. `None` when file logging is
    /// disabled (level == None or retention == 0).
    log_purge: Option<tokio::task::JoinHandle<()>>,
    /// Tracker task supervisor. Shared with every accepted
    /// connection (handlers call into it). On shutdown, aborts every
    /// per-tracker task and awaits the joins.
    tracker: Arc<tracker::TrackerManager>,
    /// UPnP gateway plus its lease-renewal task. `None` when `--upnp`
    /// is not set or setup failed. Listed last because shutdown's
    /// only async / fallible step is the port-mapping removal here.
    upnp: Option<(Arc<upnp::Gateway>, tokio::task::JoinHandle<()>)>,
}

impl DaemonHandles {
    /// Stop every background task and release every external resource
    /// (UPnP port mapping). Failures during teardown are best-effort —
    /// the daemon is going down anyway.
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

/// Setup UPnP port forwarding if enabled
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
            // Spawn background task to renew UPnP lease periodically
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

/// Setup network: TCP listeners (main + transfer + optionally WebSocket) and TLS acceptor
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
    // Ensure cert + key exist (generates a fresh self-signed pair on
    // first run) and compute the SHA-256 fingerprint for handshake
    // reporting. Cert + key live directly in the data directory.
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

    // Create main BBS listener
    let addr = SocketAddr::new(bind, port);
    let listener = match TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(e) => {
            error!("{}{}: {}", ERR_BIND_FAILED, addr, e);
            std::process::exit(1);
        }
    };
    info!("{}{}", MSG_LISTENING, addr);

    // Create transfer port listener
    let transfer_addr = SocketAddr::new(bind, transfer_port);
    let transfer_listener = match TcpListener::bind(transfer_addr).await {
        Ok(listener) => listener,
        Err(e) => {
            error!("{}{}: {}", ERR_BIND_FAILED, transfer_addr, e);
            std::process::exit(1);
        }
    };
    info!("{}{}", MSG_TRANSFER_LISTENING, transfer_addr);

    // Create WebSocket listeners if enabled
    let (ws_listener, ws_transfer_listener) = if let (Some(ws_port), Some(ws_transfer_port)) =
        (websocket_port, transfer_websocket_port)
    {
        // Create WebSocket BBS listener
        let ws_addr = SocketAddr::new(bind, ws_port);
        let ws_listener = match TcpListener::bind(ws_addr).await {
            Ok(listener) => listener,
            Err(e) => {
                error!("{}{}: {}", ERR_BIND_FAILED, ws_addr, e);
                std::process::exit(1);
            }
        };
        info!("{}{}", MSG_WS_LISTENING, ws_addr);

        // Create WebSocket transfer listener
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

/// Setup file area directories
///
/// Returns the canonicalized path to the file area root, ready for use
/// with `resolve_path()` and other security-sensitive operations.
fn setup_file_area(file_root: Option<std::path::PathBuf>, data_dir: &Path) -> std::path::PathBuf {
    // Determine file root path (use provided path or default under data dir)
    let root = file_root.unwrap_or_else(|| files::default_file_root(data_dir));

    // Initialize file area directories (creates them if needed)
    if let Err(e) = files::init_file_area(&root) {
        error!("{}{}", ERR_INIT_FILE_AREA, e);
        std::process::exit(1);
    }

    // Canonicalize the path for security - this resolves symlinks and
    // ensures we have an absolute path for starts_with() checks in resolve_path()
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

/// Log connection errors, filtering out benign TLS warnings
///
/// Filters out:
/// - TLS close_notify warnings (clients disconnecting abruptly)
/// - TLS handshake failures (only logged at debug level)
/// - WebSocket handshake failures (HTTP-only crawlers, slow upgrades,
///   timeout-elapsed peers — debug level)
fn log_connection_error(error: &io::Error, peer_addr: SocketAddr) {
    let error_msg = error.to_string();

    // Filter out benign TLS close_notify warnings (clients disconnecting abruptly)
    if error_msg.contains(TLS_CLOSE_NOTIFY_MSG) {
        return;
    }

    // TLS handshake failures are debug-only (scanners, incompatible clients)
    if error_msg.contains(nexus_common::TLS_HANDSHAKE_FAILED_PREFIX) {
        debug!(ip = %peer_addr, err = %error, "{}", LOG_CONNECTION_ERROR_TLS);
        return;
    }

    // WebSocket upgrade failures are debug-only (same noise profile as
    // TLS scanner traffic — HTTP-only probes, half-spoken upgrades,
    // slowloris timeouts).
    if error_msg.contains(nexus_common::WS_HANDSHAKE_FAILED_PREFIX) {
        debug!(ip = %peer_addr, err = %error, "{}", LOG_CONNECTION_ERROR_WS);
        return;
    }

    error!(ip = %peer_addr, err = %error, "{}", LOG_CONNECTION_ERROR);
}

/// Setup graceful shutdown signal handling (Ctrl+C)
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
    fn test_resolve_data_dir_override_returned_verbatim() {
        let override_path = std::path::PathBuf::from("/var/lib/nexusd-custom");
        assert_eq!(resolve_data_dir(Some(override_path.clone())), override_path);
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
