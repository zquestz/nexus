//! Nexus BBS Server

mod args;
mod connection;
mod constants;
mod db;
mod handlers;
mod users;

use args::Args;
use clap::Parser;
use constants::*;
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

    // Main server loop - accept incoming connections
    let debug = args.debug;
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
                        eprintln!("{}{}: {}", ERR_CONNECTION, peer_addr, e);
                    }
                });
            }
            Err(e) => {
                eprintln!("{}{}", ERR_ACCEPT, e);
            }
        }
    }
}

/// Load existing TLS configuration or generate new self-signed certificate
fn load_or_generate_tls_config(cert_dir: &std::path::Path) -> Result<TlsAcceptor, String> {
    let cert_path = cert_dir.join(CERT_FILENAME);
    let key_path = cert_dir.join(KEY_FILENAME);

    // Check if certificate and key already exist
    if cert_path.exists() && key_path.exists() {
        // Load existing certificate
        load_tls_config(&cert_path, &key_path)
    } else {
        // Generate new self-signed certificate
        println!("{}", MSG_GENERATING_CERT);
        generate_self_signed_cert(&cert_path, &key_path)?;
        load_tls_config(&cert_path, &key_path)
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
        .push(rcgen::DnType::CommonName, "Nexus BBS Server");

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
            eprintln!("Error: {}", e);
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

/// Setup network: TCP listener and TLS acceptor
async fn setup_network(
    bind: std::net::IpAddr,
    port: u16,
    db_path: &std::path::Path,
) -> (TcpListener, TlsAcceptor) {
    // Get certificate directory (same parent as database)
    let cert_dir = db_path
        .parent()
        .expect("Database path should have a parent directory")
        .to_path_buf();

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
