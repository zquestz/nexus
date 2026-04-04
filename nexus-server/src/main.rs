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
mod logging;
mod transfers;
mod upnp;
mod users;
mod voice;
mod websocket;

use std::fs;
use std::io::{self, BufReader};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use clap::Parser;
use sha2::{Digest, Sha256};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::rustls::pki_types::CertificateDer;
use tracing::{debug, error, info, warn};

use args::Args;
use channels::{Channel, ChannelManager};
use connection::ConnectionParams;
use connection_tracker::ConnectionTracker;
use constants::*;
use files::FileIndex;
use flood::FloodConfig;
use ip_rule_cache::IpRuleCache;
use logging::LogLevel;
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
        .expect("Failed to install rustls crypto provider");

    let args = Args::parse();

    // Initialize logging (global log level + tracing subscriber)
    if let Err(e) = logging::init(&args.log_level, args.log_retention, args.no_log_timestamps) {
        warn!(err = %e, "{}", LOG_LOGGING_INIT_FAILED);
    }

    // Purge old log files on startup
    if args.log_retention > std::time::Duration::ZERO && args.log_level != LogLevel::None {
        logging::purge_old_logs(args.log_retention);
    }

    // Startup info
    info!("{}{}", MSG_BANNER, env!("CARGO_PKG_VERSION"));
    info!("{}{}", MSG_LOG_LEVEL, args.log_level);
    if args.log_level != LogLevel::None && args.log_retention > std::time::Duration::ZERO {
        info!("{}{}", MSG_LOG_DIR, logging::log_dir().display());
    }

    // Setup database
    let (database, user_manager, db_path) = setup_db(args.database).await;

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
    let file_root = setup_file_area(args.file_root);

    // Setup network (TCP listeners + TLS, optionally WebSocket listeners)
    let websocket_enabled = args.websocket;
    let (
        listener,
        transfer_listener,
        ws_listener,
        ws_transfer_listener,
        (tls_acceptor, fingerprint),
        cert_dir,
    ) = setup_network(
        args.bind,
        args.port,
        args.transfer_port,
        if websocket_enabled {
            Some(args.websocket_port)
        } else {
            None
        },
        if websocket_enabled {
            Some(args.transfer_websocket_port)
        } else {
            None
        },
        &db_path,
    )
    .await;

    // Setup voice DTLS listener (same port as TCP, OS routes by protocol)
    let voice_addr = SocketAddr::new(args.bind, args.port);
    let cert_path = cert_dir.join(CERT_FILENAME);
    let key_path = cert_dir.join(KEY_FILENAME);
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
    let transfer_port = args.transfer_port;
    let transfer_websocket_port = if websocket_enabled {
        Some(args.transfer_websocket_port)
    } else {
        None
    };

    // Setup UPnP port forwarding if requested (forwards WS ports only if enabled)
    let upnp_handle = setup_upnp(
        args.upnp,
        args.bind,
        args.port,
        transfer_port,
        if websocket_enabled {
            Some(args.websocket_port)
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
    let data_dir = db_path
        .parent()
        .expect("database path should have parent directory");
    let file_index = Arc::new(FileIndex::new(data_dir, file_root));

    // Trigger initial index build in background
    file_index.trigger_reindex();

    // Create transfer registry for tracking active transfers (enables ban disconnection)
    let transfer_registry = Arc::new(TransferRegistry::new());

    // Create voice registry for tracking active voice sessions (ephemeral, in-memory only)
    let voice_registry = VoiceRegistry::new();

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

    // Capture values for the log purge timer
    let log_level = args.log_level;
    let log_retention = args.log_retention;

    // Main server loops - accept incoming connections on both ports
    tokio::select! {
        _ = shutdown_signal => {
            info!("{}", MSG_SHUTDOWN_RECEIVED);

            // Cleanup UPnP port forwarding if enabled
            if let Some((gateway, renewal_task)) = upnp_handle {
                renewal_task.abort();

                // Remove port mapping
                if let Err(e) = gateway.remove_port_mapping().await {
                    warn!("{}{}", WARN_UPNP_REMOVE_MAPPING_FAILED, e);
                }
            }
        }
        // Main BBS port accept loop
        _ = async {
            loop {
                match listener.accept().await {
                    Ok((socket, peer_addr)) => {
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
                            channel_manager: channel_manager.clone(),
                            transfer_registry: transfer_registry.clone(),
                            voice_registry: voice_registry.clone(),
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
                                    .expect("ip rule cache lock poisoned");
                                if cache.needs_rebuild() {
                                    // Drop read lock before acquiring write lock
                                    drop(cache);
                                    ip_rule_cache_for_check
                                        .write()
                                        .expect("ip rule cache lock poisoned")
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
                        error!(err = %e, "{}", LOG_ACCEPT_ERROR);
                    }
                }
            }
        } => {}
        // Transfer port accept loop
        _ = async {
            loop {
                match transfer_listener.accept().await {
                    Ok((socket, peer_addr)) => {
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
                            transfer_registry: transfer_registry.clone(),
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
                                    .expect("ip rule cache lock poisoned");
                                if cache.needs_rebuild() {
                                    // Drop read lock before acquiring write lock
                                    drop(cache);
                                    ip_rule_cache_for_check
                                        .write()
                                        .expect("ip rule cache lock poisoned")
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
                        error!(err = %e, "{}", LOG_ACCEPT_ERROR);
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
                            channel_manager: channel_manager.clone(),
                            transfer_registry: transfer_registry.clone(),
                            voice_registry: voice_registry.clone(),
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
                                    .expect("ip rule cache lock poisoned");
                                if cache.needs_rebuild() {
                                    drop(cache);
                                    ip_rule_cache_for_check
                                        .write()
                                        .expect("ip rule cache lock poisoned")
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
                        error!(err = %e, "{}", LOG_ACCEPT_ERROR);
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
                            transfer_registry: transfer_registry.clone(),
                        };
                        let tls_acceptor = tls_acceptor.clone();
                        let ip_rule_cache_for_check = ip_rule_cache.clone();

                        tokio::spawn(async move {
                            let _guard = transfer_guard;

                            // Check IP rules BEFORE TLS handshake (same as TCP)
                            let should_allow = {
                                let cache = ip_rule_cache_for_check
                                    .read()
                                    .expect("ip rule cache lock poisoned");
                                if cache.needs_rebuild() {
                                    drop(cache);
                                    ip_rule_cache_for_check
                                        .write()
                                        .expect("ip rule cache lock poisoned")
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
                        error!(err = %e, "{}", LOG_ACCEPT_ERROR);
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

                // Check if dirty and not already reindexing
                if file_index_for_timer.is_dirty() && !file_index_for_timer.is_reindexing() {
                    debug!("{}", LOG_FILE_INDEX_DIRTY);
                    file_index_for_timer.trigger_reindex();
                }
            }
        } => {}
        // Log retention purge timer - runs daily
        _ = async {
            if log_retention == std::time::Duration::ZERO || log_level == LogLevel::None {
                std::future::pending::<()>().await;
                return;
            }
            loop {
                tokio::time::sleep(Duration::from_secs(24 * 60 * 60)).await;
                logging::purge_old_logs(log_retention);
            }
        } => {}
    }
}

/// Load existing TLS configuration or generate new self-signed certificate
fn load_or_generate_tls_config(
    cert_dir: &std::path::Path,
) -> Result<(TlsAcceptor, String), String> {
    let cert_path = cert_dir.join(CERT_FILENAME);
    let key_path = cert_dir.join(KEY_FILENAME);

    // Check if certificate and key already exist
    if cert_path.exists() && key_path.exists() {
        // Load existing certificate
        let acceptor = load_tls_config(&cert_path, &key_path)?;
        let fingerprint = display_certificate_fingerprint(&cert_path)?;
        Ok((acceptor, fingerprint))
    } else {
        // Generate new self-signed certificate
        info!("{}", MSG_GENERATING_CERT);
        generate_self_signed_cert(&cert_path, &key_path)?;
        let acceptor = load_tls_config(&cert_path, &key_path)?;
        let fingerprint = display_certificate_fingerprint(&cert_path)?;
        Ok((acceptor, fingerprint))
    }
}

/// Generate a self-signed certificate and private key
fn generate_self_signed_cert(
    cert_path: &std::path::Path,
    key_path: &std::path::Path,
) -> Result<(), String> {
    use rcgen::{CertificateParams, KeyPair};

    // Generate key pair
    let key_pair = KeyPair::generate().map_err(|e| format!("{}{}", ERR_GENERATE_KEYPAIR, e))?;

    // Create certificate parameters
    let mut params =
        CertificateParams::new(vec![]).map_err(|e| format!("{}{}", ERR_CREATE_CERT_PARAMS, e))?;

    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, TLS_CERT_COMMON_NAME);

    // Generate certificate
    let cert = params
        .self_signed(&key_pair)
        .map_err(|e| format!("{}{}", ERR_GENERATE_CERT, e))?;

    // Write certificate to file
    fs::write(cert_path, cert.pem()).map_err(|e| format!("{}{}", ERR_WRITE_CERT_FILE, e))?;
    #[cfg(unix)]
    set_secure_permissions(cert_path).map_err(|e| format!("{}{}", ERR_SET_CERT_PERMISSIONS, e))?;

    // Write private key to file
    fs::write(key_path, key_pair.serialize_pem())
        .map_err(|e| format!("{}{}", ERR_WRITE_KEY_FILE, e))?;
    #[cfg(unix)]
    set_secure_permissions(key_path).map_err(|e| format!("{}{}", ERR_SET_KEY_PERMISSIONS, e))?;

    info!("{}{}", MSG_CERT_GENERATED, cert_path.display());
    info!("{}{}", MSG_KEY_GENERATED, key_path.display());

    Ok(())
}

/// Load TLS configuration from certificate and key files
fn load_tls_config(
    cert_path: &std::path::Path,
    key_path: &std::path::Path,
) -> Result<TlsAcceptor, String> {
    // Load certificate chain
    let cert_file =
        fs::File::open(cert_path).map_err(|e| format!("{}{}", ERR_OPEN_CERT_FILE, e))?;
    let mut cert_reader = BufReader::new(cert_file);
    let certs: Vec<CertificateDer> = rustls_pemfile::certs(&mut cert_reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("{}{}", ERR_PARSE_CERT, e))?;

    if certs.is_empty() {
        return Err(ERR_NO_CERTS_FOUND.to_string());
    }

    // Load private key
    let key_file = fs::File::open(key_path).map_err(|e| format!("{}{}", ERR_OPEN_KEY_FILE, e))?;
    let mut key_reader = BufReader::new(key_file);
    let private_key = rustls_pemfile::private_key(&mut key_reader)
        .map_err(|e| format!("{}{}", ERR_PARSE_KEY, e))?
        .ok_or(ERR_NO_KEY_FOUND)?;

    // Create TLS server configuration
    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, private_key)
        .map_err(|e| format!("{}{}", ERR_CREATE_TLS_CONFIG, e))?;

    Ok(TlsAcceptor::from(Arc::new(config)))
}

/// Set secure file permissions (0o600 - owner read/write only)
/// Unix only - Windows uses NTFS ACLs by default
#[cfg(unix)]
fn set_secure_permissions(path: &std::path::Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = fs::metadata(path).map_err(|e| format!("{}{}", ERR_READ_METADATA, e))?;
    let mut permissions = metadata.permissions();
    permissions.set_mode(0o600);
    fs::set_permissions(path, permissions).map_err(|e| format!("{}{}", ERR_SET_PERMS, e))?;
    Ok(())
}

/// Setup database connection and initialize user manager
async fn setup_db(
    database_path: Option<std::path::PathBuf>,
) -> (db::Database, UserManager, std::path::PathBuf) {
    // Determine database path (use provided path or platform default)
    let db_path = database_path.unwrap_or_else(|| match db::default_database_path() {
        Ok(path) => path,
        Err(e) => {
            error!("{}{}", ERR_GENERIC, e);
            std::process::exit(1);
        }
    });

    // Initialize database connection pool and run migrations
    let pool = match db::init_db(&db_path).await {
        Ok(pool) => pool,
        Err(e) => {
            error!("{}{}", ERR_DATABASE_INIT, e);
            std::process::exit(1);
        }
    };
    info!("{}{}", MSG_DATABASE, db_path.display());

    // Set secure permissions on database file (0o600) - Unix only
    #[cfg(unix)]
    if let Err(e) = set_secure_permissions(&db_path) {
        error!("{}{}", ERR_SET_PERMISSIONS, e);
        std::process::exit(1);
    }

    // Create database and user manager instances
    // Note: SqlitePool uses Arc internally, so clone() is cheap
    let database = db::Database::new(pool);
    let user_manager = UserManager::new();

    (database, user_manager, db_path)
}

/// Setup UPnP port forwarding if enabled
async fn setup_upnp(
    enabled: bool,
    bind: std::net::IpAddr,
    main_port: u16,
    transfer_port: u16,
    websocket_port: Option<u16>,
    transfer_websocket_port: Option<u16>,
) -> Option<(Arc<upnp::UpnpGateway>, tokio::task::JoinHandle<()>)> {
    if !enabled {
        return None;
    }

    match upnp::UpnpGateway::setup(
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
            warn!("{}{}", MSG_UPNP_WARNING, e);
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
    db_path: &std::path::Path,
) -> (
    TcpListener,
    TcpListener,
    Option<TcpListener>,
    Option<TcpListener>,
    (TlsAcceptor, String),
    std::path::PathBuf, // cert_dir for voice DTLS
) {
    // Get certificate directory (same parent as database)
    let cert_dir = db_path.parent().expect(ERR_DB_PATH_NO_PARENT).to_path_buf();

    // Load or generate TLS certificate
    let tls_acceptor = match load_or_generate_tls_config(&cert_dir) {
        Ok(acceptor) => acceptor,
        Err(e) => {
            error!("{}{}", ERR_TLS_INIT, e);
            std::process::exit(1);
        }
    };
    info!("{}{}", MSG_CERTIFICATES, cert_dir.display());

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
        tls_acceptor,
        cert_dir,
    )
}

/// Calculate and display certificate fingerprint (SHA-256)
fn display_certificate_fingerprint(cert_path: &std::path::Path) -> Result<String, String> {
    // Read certificate file
    let cert_pem =
        fs::read_to_string(cert_path).map_err(|e| format!("{}{}", ERR_OPEN_CERT_FILE, e))?;

    // Parse PEM to get DER-encoded certificate
    let cert_der = pem::parse(&cert_pem).map_err(|e| format!("{}{}", ERR_PARSE_CERT, e))?;

    // Calculate SHA-256 fingerprint
    let mut hasher = Sha256::new();
    hasher.update(cert_der.contents());
    let fingerprint = hasher.finalize();

    // Format as colon-separated hex string (uppercase)
    let hex_str = hex::encode_upper(fingerprint);
    let fingerprint_str = hex_str
        .as_bytes()
        .chunks(2)
        .map(|chunk| std::str::from_utf8(chunk).expect("hex encoding produces valid ASCII"))
        .collect::<Vec<_>>()
        .join(":");

    info!("{}{}", MSG_CERT_FINGERPRINT, fingerprint_str);
    Ok(fingerprint_str)
}

/// Setup file area directories
///
/// Returns the canonicalized path to the file area root, ready for use
/// with `resolve_path()` and other security-sensitive operations.
fn setup_file_area(file_root: Option<std::path::PathBuf>) -> std::path::PathBuf {
    // Determine file root path (use provided path or platform default)
    let root = file_root.unwrap_or_else(|| match files::default_file_root() {
        Ok(path) => path,
        Err(e) => {
            error!("{}{}", ERR_GENERIC, e);
            std::process::exit(1);
        }
    });

    // Initialize file area directories (creates them if needed)
    if let Err(e) = files::init_file_area(&root) {
        error!("{}{}", ERR_GENERIC, e);
        std::process::exit(1);
    }

    // Canonicalize the path for security - this resolves symlinks and
    // ensures we have an absolute path for starts_with() checks in resolve_path()
    let canonical_root = match root.canonicalize() {
        Ok(path) => path,
        Err(e) => {
            error!("{}{}{}", ERR_GENERIC, ERR_FILE_ROOT_CANONICALIZE, e);
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
fn log_connection_error(error: &io::Error, peer_addr: SocketAddr) {
    let error_msg = error.to_string();

    // Filter out benign TLS close_notify warnings (clients disconnecting abruptly)
    if error_msg.contains(TLS_CLOSE_NOTIFY_MSG) {
        return;
    }

    // TLS handshake failures are debug-only (scanners, incompatible clients)
    if error_msg.contains(TLS_HANDSHAKE_FAILED_PREFIX) {
        debug!(ip = %peer_addr, err = %error, "{}", LOG_CONNECTION_ERROR_TLS);
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
