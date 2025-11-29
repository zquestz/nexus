//! Nexus BBS Server

mod args;
mod connection;
mod constants;
mod db;
mod handlers;
mod i18n;
mod upnp;
mod users;

use args::Args;
use clap::Parser;
use constants::*;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::BufReader;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::rustls::pki_types::CertificateDer;
use users::UserManager;

#[tokio::main]
async fn main() {
    let args = Args::parse();

    // Print banner first
    println!("{}{}", MSG_BANNER, env!("CARGO_PKG_VERSION"));

    // Setup database
    let (database, user_manager, db_path) = setup_db(args.database).await;

    // Setup network (TCP listener + TLS)
    let (listener, tls_acceptor) = setup_network(args.bind, args.port, &db_path).await;

    // Setup UPnP port forwarding if requested
    let upnp_handle = setup_upnp(args.upnp, args.bind, args.port).await;

    // Setup graceful shutdown handling
    let shutdown_signal = setup_shutdown_signal();

    // Main server loop - accept incoming connections
    let debug = args.debug;
    tokio::select! {
        _ = shutdown_signal => {
            println!("{}", MSG_SHUTDOWN_RECEIVED);

            // Cleanup UPnP port forwarding if enabled
            if let Some((gateway, renewal_task)) = upnp_handle {
                renewal_task.abort();

                // Remove port mapping
                if let Err(e) = gateway.remove_port_mapping().await {
                    eprintln!("{}{}", WARN_UPNP_REMOVE_MAPPING_FAILED, e);
                }
            }
        }
        _ = async {
            loop {
                match listener.accept().await {
                    Ok((socket, peer_addr)) => {
                        let user_manager = user_manager.clone();
                        let database = database.clone();
                        let tls_acceptor = tls_acceptor.clone();

                        // Spawn a new task to handle this connection
                        tokio::spawn(async move {
                            if let Err(e) = connection::handle_connection(
                                socket,
                                peer_addr,
                                user_manager,
                                database,
                                debug,
                                tls_acceptor,
                            )
                            .await
                            {
                                let error_msg = e.to_string();

                                // Filter out benign TLS close_notify warnings (clients disconnecting abruptly)
                                if error_msg.contains(TLS_CLOSE_NOTIFY_MSG) {
                                    return;
                                }

                                // TLS handshake failures are debug-only (scanners, incompatible clients)
                                if error_msg.contains(TLS_HANDSHAKE_FAILED_PREFIX) {
                                    if debug {
                                        eprintln!("{}{}: {}", ERR_CONNECTION, peer_addr, e);
                                    }
                                    return;
                                }

                                eprintln!("{}{}: {}", ERR_CONNECTION, peer_addr, e);
                            }
                        });
                    }
                    Err(e) => {
                        eprintln!("{}{}", ERR_ACCEPT, e);
                    }
                }
            }
        } => {}
    }
}

/// Load existing TLS configuration or generate new self-signed certificate
fn load_or_generate_tls_config(cert_dir: &std::path::Path) -> Result<TlsAcceptor, String> {
    let cert_path = cert_dir.join(CERT_FILENAME);
    let key_path = cert_dir.join(KEY_FILENAME);

    // Check if certificate and key already exist
    if cert_path.exists() && key_path.exists() {
        // Load existing certificate
        let acceptor = load_tls_config(&cert_path, &key_path)?;
        display_certificate_fingerprint(&cert_path)?;
        Ok(acceptor)
    } else {
        // Generate new self-signed certificate
        println!("{}", MSG_GENERATING_CERT);
        generate_self_signed_cert(&cert_path, &key_path)?;
        let acceptor = load_tls_config(&cert_path, &key_path)?;
        display_certificate_fingerprint(&cert_path)?;
        Ok(acceptor)
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

    println!("{}{}", MSG_CERT_GENERATED, cert_path.display());
    println!("{}{}", MSG_KEY_GENERATED, key_path.display());

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
            eprintln!("{}{}", ERR_GENERIC, e);
            std::process::exit(1);
        }
    });

    // Initialize database connection pool and run migrations
    let pool = match db::init_db(&db_path).await {
        Ok(pool) => pool,
        Err(e) => {
            eprintln!("{}{}", ERR_DATABASE_INIT, e);
            std::process::exit(1);
        }
    };
    println!("{}{}", MSG_DATABASE, db_path.display());

    // Set secure permissions on database file (0o600) - Unix only
    #[cfg(unix)]
    if let Err(e) = set_secure_permissions(&db_path) {
        eprintln!("{}{}", ERR_SET_PERMISSIONS, e);
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
    port: u16,
) -> Option<(Arc<upnp::UpnpGateway>, tokio::task::JoinHandle<()>)> {
    if !enabled {
        return None;
    }

    match upnp::UpnpGateway::setup(bind, port).await {
        Ok(gateway) => {
            // Spawn background task to renew UPnP lease periodically
            let gateway_arc = Arc::new(gateway);
            let renewal_task = upnp::spawn_lease_renewal_task(gateway_arc.clone());
            Some((gateway_arc, renewal_task))
        }
        Err(e) => {
            eprintln!("{}{}", MSG_UPNP_WARNING, e);
            eprintln!("{}", MSG_UPNP_CONTINUE);
            eprintln!("{}", MSG_UPNP_MANUAL);
            None
        }
    }
}

/// Setup network: TCP listener and TLS acceptor
async fn setup_network(
    bind: std::net::IpAddr,
    port: u16,
    db_path: &std::path::Path,
) -> (TcpListener, TlsAcceptor) {
    // Get certificate directory (same parent as database)
    let cert_dir = db_path.parent().expect(ERR_DB_PATH_NO_PARENT).to_path_buf();

    // Load or generate TLS certificate
    let tls_acceptor = match load_or_generate_tls_config(&cert_dir) {
        Ok(acceptor) => acceptor,
        Err(e) => {
            eprintln!("{}{}", ERR_TLS_INIT, e);
            std::process::exit(1);
        }
    };
    println!("{}{}", MSG_CERTIFICATES, cert_dir.display());

    // Create socket address
    let addr = SocketAddr::new(bind, port);

    let listener = match TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(e) => {
            eprintln!("{}{}: {}", ERR_BIND_FAILED, addr, e);
            std::process::exit(1);
        }
    };

    println!("{}{}{}", MSG_LISTENING, addr, MSG_TLS_ENABLED);

    (listener, tls_acceptor)
}

/// Calculate and display certificate fingerprint (SHA-256)
fn display_certificate_fingerprint(cert_path: &std::path::Path) -> Result<(), String> {
    // Read certificate file
    let cert_pem =
        fs::read_to_string(cert_path).map_err(|e| format!("{}{}", ERR_OPEN_CERT_FILE, e))?;

    // Parse PEM to get DER-encoded certificate
    let cert_der = pem::parse(&cert_pem).map_err(|e| format!("{}{}", ERR_PARSE_CERT, e))?;

    // Calculate SHA-256 fingerprint
    let mut hasher = Sha256::new();
    hasher.update(cert_der.contents());
    let fingerprint = hasher.finalize();

    // Format as colon-separated hex string
    let fingerprint_str = fingerprint
        .iter()
        .map(|byte| format!("{:02X}", byte))
        .collect::<Vec<_>>()
        .join(":");

    println!("{}{}", MSG_CERT_FINGERPRINT, fingerprint_str);
    Ok(())
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
