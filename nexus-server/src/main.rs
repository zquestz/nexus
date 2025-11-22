//! Nexus BBS Server

mod args;
mod connection;
mod db;
mod handlers;
mod users;

use args::Args;
use clap::Parser;
use nexus_common::yggdrasil::is_yggdrasil_address;
use std::net::SocketAddrV6;
use tokio::net::TcpListener;
use users::UserManager;

#[tokio::main]
async fn main() {
    let args = Args::parse();

    // Validate that the bind address is in the Yggdrasil network range
    if !is_yggdrasil_address(&args.bind) {
        eprintln!(
            "Error: Address {} is not in the Yggdrasil range (0200::/7)",
            args.bind
        );
        eprintln!("Yggdrasil addresses must start with 02xx: or 03xx:");
        std::process::exit(1);
    }

    // Determine database path (use provided path or platform default)
    let db_path = args.database.unwrap_or_else(|| {
        match db::default_database_path() {
            Ok(path) => path,
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
    });

    // Initialize database connection pool and run migrations
    let pool = match db::init_db(&db_path).await {
        Ok(pool) => pool,
        Err(e) => {
            eprintln!("Failed to initialize database: {}", e);
            std::process::exit(1);
        }
    };

    // Create database and user manager instances
    // Note: SqlitePool uses Arc internally, so clone() is cheap
    let user_db = db::UserDb::new(pool.clone());
    let user_manager = UserManager::new();

    // Create socket address (flow_info and scope_id set to 0)
    let addr = SocketAddrV6::new(args.bind, args.port, 0, 0);

    let listener = match TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(e) => {
            eprintln!("Failed to bind to {}: {}", addr, e);
            std::process::exit(1);
        }
    };

    println!("Nexus BBS Server v{}", env!("CARGO_PKG_VERSION"));
    println!("Listening on [{}]:{}", args.bind, args.port);
    println!("Database: {}", db_path.display());

    // Main server loop - accept incoming connections
    loop {
        match listener.accept().await {
            Ok((socket, peer_addr)) => {
                // Clone references for the spawned task
                let user_manager_clone = user_manager.clone();
                let user_db_clone = user_db.clone();

                // Spawn a new task to handle this connection
                tokio::spawn(async move {
                    if let Err(e) = connection::handle_connection(
                        socket,
                        peer_addr,
                        user_manager_clone,
                        user_db_clone,
                    )
                    .await
                    {
                        eprintln!("Error handling connection from {}: {}", peer_addr, e);
                    }
                });
            }
            Err(e) => {
                eprintln!("Failed to accept connection: {}", e);
            }
        }
    }
}
