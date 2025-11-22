//! Nexus BBS Server

mod db;
mod users;

use clap::Parser;
use nexus_common::protocol::{ClientMessage, ServerMessage, UserInfo};
use nexus_common::yggdrasil::is_yggdrasil_address;
use std::net::{Ipv6Addr, SocketAddrV6};
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use users::UserManager;

/// Nexus BBS Server - A next-gen BBS server for the Yggdrasil network
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// IPv6 address to bind to (should be your Yggdrasil address)
    #[arg(short, long)]
    bind: Ipv6Addr,

    /// Port to listen on
    #[arg(short, long, default_value = "7500")]
    port: u16,

    /// Database file path (default: platform-specific data directory)
    #[arg(short, long)]
    database: Option<PathBuf>,
}

/// Get the default database path for the current platform
fn default_database_path() -> PathBuf {
    let data_dir = dirs::data_dir().expect("Unable to determine data directory");
    data_dir.join("nexusd").join("nexus.db")
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    // Validate that the address is in the Yggdrasil range
    if !is_yggdrasil_address(&args.bind) {
        eprintln!(
            "Error: Address {} is not in the Yggdrasil range (0200::/7)",
            args.bind
        );
        eprintln!("Yggdrasil addresses must start with 02xx: or 03xx:");
        std::process::exit(1);
    }

    // Determine database path
    let db_path = args.database.unwrap_or_else(default_database_path);

    // Initialize database
    let pool = match db::init_db(&db_path).await {
        Ok(pool) => pool,
        Err(e) => {
            eprintln!("Failed to initialize database: {}", e);
            std::process::exit(1);
        }
    };

    let user_db = db::UserDb::new(pool.clone());

    // Create user manager
    let user_manager = UserManager::new();

    // Create socket address
    let addr = SocketAddrV6::new(args.bind, args.port, 0, 0);

    // Bind TCP listener
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

    // Accept connections in a loop
    loop {
        match listener.accept().await {
            Ok((socket, peer_addr)) => {
                let user_manager = user_manager.clone();
                let user_db = user_db.clone();

                // Spawn a task to handle this connection
                tokio::spawn(async move {
                    if let Err(e) =
                        handle_connection(socket, peer_addr, user_manager, user_db).await
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

/// Handle a client connection
async fn handle_connection(
    socket: tokio::net::TcpStream,
    peer_addr: std::net::SocketAddr,
    user_manager: UserManager,
    user_db: db::UserDb,
) -> std::io::Result<()> {
    let (reader, mut writer) = socket.into_split();
    let mut reader = BufReader::new(reader);

    // Create channel for receiving server messages to send to this client
    let (tx, mut rx) = mpsc::unbounded_channel::<ServerMessage>();

    // Send a welcome message
    writer.write_all(b"Welcome to Nexus BBS!\r\n").await?;
    writer.flush().await?;

    let mut user_id: Option<u32> = None;
    let mut handshake_complete = false;
    let mut line = String::new();

    // Main loop - handle both incoming messages and outgoing events
    loop {
        tokio::select! {
            // Handle incoming client messages
            result = reader.read_line(&mut line) => {
                let n = result?;

                // Connection closed
                if n == 0 {
                    break;
                }

                // Handle the message
                if let Err(e) = handle_client_message(
                    &line,
                    &mut user_id,
                    &mut handshake_complete,
                    &mut writer,
                    &user_manager,
                    &user_db,
                    peer_addr,
                    &tx,
                ).await {
                    eprintln!("Error handling message: {}", e);
                    break;
                }

                line.clear();
            }

            // Handle outgoing server messages/events
            Some(msg) = rx.recv() => {
                let json = serde_json::to_string(&msg).unwrap();
                writer.write_all(json.as_bytes()).await?;
                writer.write_all(b"\n").await?;
                writer.flush().await?;
            }
        }
    }

    // Remove user on disconnect
    if let Some(id) = user_id {
        if let Some(user) = user_manager.remove_user(id).await {
            // Broadcast disconnection to all users
            user_manager
                .broadcast(ServerMessage::UserDisconnected {
                    user_id: id,
                    username: user.username.clone(),
                })
                .await;
        }
    }

    Ok(())
}

/// Handle a message from the client
async fn handle_client_message(
    line: &str,
    user_id: &mut Option<u32>,
    handshake_complete: &mut bool,
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    user_manager: &UserManager,
    user_db: &db::UserDb,
    peer_addr: std::net::SocketAddr,
    tx: &mpsc::UnboundedSender<ServerMessage>,
) -> std::io::Result<()> {
    let line = line.trim();
    if line.is_empty() {
        return Ok(());
    }

    // Try to parse as a client message
    // NOTE: We don't log the raw message here to avoid leaking passwords
    match serde_json::from_str::<ClientMessage>(line) {
        Ok(msg) => {
            match msg {
                ClientMessage::Handshake { version } => {
                    if *handshake_complete {
                        eprintln!("Duplicate handshake attempt from {}", peer_addr);
                        let response = ServerMessage::HandshakeResponse {
                            success: false,
                            version: nexus_common::PROTOCOL_VERSION.to_string(),
                            error: Some("Handshake already completed".to_string()),
                        };
                        send_message(writer, &response).await?;
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            "Duplicate handshake",
                        ));
                    }

                    // Check if version is compatible
                    let server_version = nexus_common::PROTOCOL_VERSION;
                    if version == server_version {
                        *handshake_complete = true;
                        let response = ServerMessage::HandshakeResponse {
                            success: true,
                            version: server_version.to_string(),
                            error: None,
                        };
                        send_message(writer, &response).await?;
                    } else {
                        let response = ServerMessage::HandshakeResponse {
                            success: false,
                            version: server_version.to_string(),
                            error: Some(format!(
                                "Version mismatch: server uses {}, client uses {}",
                                server_version, version
                            )),
                        };
                        send_message(writer, &response).await?;
                        eprintln!("Handshake failed with {}: version mismatch", peer_addr);
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            "Version mismatch",
                        ));
                    }
                }
                ClientMessage::Login {
                    username,
                    password,
                    features,
                } => {
                    if !*handshake_complete {
                        eprintln!("Login attempt from {} without handshake", peer_addr);
                        return send_error_and_disconnect(
                            writer,
                            "Handshake required",
                            Some("Login"),
                        )
                        .await;
                    }

                    if user_id.is_some() {
                        eprintln!("Duplicate login attempt from {}", peer_addr);
                        return send_error_and_disconnect(
                            writer,
                            "Already logged in",
                            Some("Login"),
                        )
                        .await;
                    }

                    // Check if this is the first user (will become admin)
                    let is_first_user = match user_db.has_any_users().await {
                        Ok(has_users) => !has_users,
                        Err(e) => {
                            eprintln!("Database error checking for users: {}", e);
                            return send_error_and_disconnect(
                                writer,
                                "Database error",
                                Some("Login"),
                            )
                            .await;
                        }
                    };

                    // Check if user exists
                    let account = match user_db.get_user_by_username(&username).await {
                        Ok(acc) => acc,
                        Err(e) => {
                            eprintln!("Database error looking up user {}: {}", username, e);
                            return send_error_and_disconnect(
                                writer,
                                "Database error",
                                Some("Login"),
                            )
                            .await;
                        }
                    };

                    // Verify password or create first user
                    let authenticated_account = if let Some(account) = account {
                        // User exists - verify password
                        match db::verify_password(&password, &account.hashed_password) {
                            Ok(true) => {
                                println!("User '{}' logged in from {}", username, peer_addr);
                                account
                            }
                            Ok(false) => {
                                eprintln!(
                                    "Invalid password for user {} from {}",
                                    username, peer_addr
                                );
                                return send_error_and_disconnect(
                                    writer,
                                    "Invalid username or password",
                                    Some("Login"),
                                )
                                .await;
                            }
                            Err(e) => {
                                eprintln!("Password verification error for {}: {}", username, e);
                                return send_error_and_disconnect(
                                    writer,
                                    "Authentication error",
                                    Some("Login"),
                                )
                                .await;
                            }
                        }
                    } else if is_first_user {
                        // First user - create as admin
                        let hashed_password = match db::hash_password(&password) {
                            Ok(hash) => hash,
                            Err(e) => {
                                eprintln!("Failed to hash password for {}: {}", username, e);
                                return send_error_and_disconnect(
                                    writer,
                                    "Failed to create user",
                                    Some("Login"),
                                )
                                .await;
                            }
                        };

                        // Admin gets all permissions
                        match user_db
                            .create_user(&username, &hashed_password, true, &db::Permissions::all())
                            .await
                        {
                            Ok(account) => {
                                println!(
                                    "Created first user (admin): '{}' from {}",
                                    username, peer_addr
                                );
                                account
                            }
                            Err(e) => {
                                eprintln!("Failed to create admin user {}: {}", username, e);
                                return send_error_and_disconnect(
                                    writer,
                                    "Failed to create user",
                                    Some("Login"),
                                )
                                .await;
                            }
                        }
                    } else {
                        // User doesn't exist and not first user
                        eprintln!("User {} does not exist", username);
                        return send_error_and_disconnect(
                            writer,
                            "Invalid username or password",
                            Some("Login"),
                        )
                        .await;
                    };

                    // User authenticated successfully - create session
                    // Note: Features are client preferences (what they want to subscribe to)
                    // Permissions are checked when executing commands, not at login
                    let session_id = format!("{}-{}", username, rand_session_id());
                    let id = user_manager
                        .add_user(
                            authenticated_account.id,
                            username.clone(),
                            session_id.clone(),
                            peer_addr,
                            tx.clone(),
                            features,
                        )
                        .await;
                    *user_id = Some(id);

                    let response = ServerMessage::LoginResponse {
                        success: true,
                        session_id: Some(session_id),
                        error: None,
                    };
                    send_message(writer, &response).await?;

                    // Broadcast user connected to all other users
                    // Broadcast user connected event to all other users (not to themselves)
                    let user_info = UserInfo {
                        id,
                        username: username.clone(),
                        login_time: current_timestamp(),
                    };
                    user_manager
                        .broadcast_except(id, ServerMessage::UserConnected { user: user_info })
                        .await;
                }
                ClientMessage::UserList => {
                    let id = match user_id {
                        Some(id) => *id,
                        None => {
                            eprintln!("UserList request from {} without login", peer_addr);
                            return send_error_and_disconnect(
                                writer,
                                "Not logged in",
                                Some("UserList"),
                            )
                            .await;
                        }
                    };

                    // Get user and check permission
                    let user = match user_manager.get_user(id).await {
                        Some(u) => u,
                        None => {
                            eprintln!("UserList request from unknown user {}", peer_addr);
                            return send_error_and_disconnect(
                                writer,
                                "Authentication error",
                                Some("UserList"),
                            )
                            .await;
                        }
                    };

                    let has_perm = match user_db
                        .has_permission(user.db_user_id, db::Permission::ListUsers)
                        .await
                    {
                        Ok(has) => has,
                        Err(e) => {
                            eprintln!("UserList permission check error: {}", e);
                            return send_error_and_disconnect(
                                writer,
                                "Database error",
                                Some("UserList"),
                            )
                            .await;
                        }
                    };

                    if !has_perm {
                        eprintln!("UserList request from {} without permission", peer_addr);
                        return send_error_and_disconnect(
                            writer,
                            "Permission denied",
                            Some("UserList"),
                        )
                        .await;
                    }

                    // Get all users from the manager
                    let all_users = user_manager.get_all_users().await;
                    let user_infos: Vec<UserInfo> = all_users
                        .into_iter()
                        .map(|u| UserInfo {
                            id: u.id,
                            username: u.username,
                            login_time: u.login_time,
                        })
                        .collect();

                    let response = ServerMessage::UserListResponse { users: user_infos };
                    send_message(writer, &response).await?;
                }
                ClientMessage::ChatSend { message } => {
                    if user_id.is_none() {
                        eprintln!("ChatSend from {} without login", peer_addr);
                        return send_error_and_disconnect(
                            writer,
                            "Not logged in",
                            Some("ChatSend"),
                        )
                        .await;
                    }

                    // Check message length limit (1024 characters)
                    if message.len() > 1024 {
                        eprintln!(
                            "ChatSend from {} exceeds length limit: {} chars",
                            peer_addr,
                            message.len()
                        );
                        return send_error_and_disconnect(
                            writer,
                            "Message too long (max 1024 characters)",
                            Some("ChatSend"),
                        )
                        .await;
                    }

                    let id = user_id.unwrap();

                    // Get the user and check permissions
                    let user = user_manager.get_user(id).await;
                    if let Some(user) = user {
                        let has_perm = match user_db
                            .has_permission(user.db_user_id, db::Permission::SendChat)
                            .await
                        {
                            Ok(has) => has,
                            Err(e) => {
                                eprintln!("ChatSend permission check error: {}", e);
                                return send_error_and_disconnect(
                                    writer,
                                    "Database error",
                                    Some("ChatSend"),
                                )
                                .await;
                            }
                        };

                        if !has_perm {
                            eprintln!("ChatSend from {} without permission", peer_addr);
                            return send_error_and_disconnect(
                                writer,
                                "Permission denied",
                                Some("ChatSend"),
                            )
                            .await;
                        }

                        if !user.has_feature("chat") {
                            eprintln!("ChatSend from {} without chat feature enabled", peer_addr);
                            return send_error_and_disconnect(
                                writer,
                                "Chat feature not enabled",
                                Some("ChatSend"),
                            )
                            .await;
                        }

                        // Broadcast to all users with chat feature
                        user_manager
                            .broadcast_to_feature(
                                "chat",
                                ServerMessage::ChatMessage {
                                    user_id: id,
                                    username: user.username.clone(),
                                    message: message.clone(),
                                },
                            )
                            .await;
                    } else {
                        return send_error_and_disconnect(
                            writer,
                            "User not found",
                            Some("ChatSend"),
                        )
                        .await;
                    }
                }
            }
        }
        Err(e) => {
            eprintln!(
                "Failed to parse message from {}: {} - message was: {:?}",
                peer_addr, e, line
            );
            return send_error_and_disconnect(
                writer,
                &format!("Invalid message format: {}", e),
                None,
            )
            .await;
        }
    }

    Ok(())
}

/// Send a message to the client
async fn send_message(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    message: &ServerMessage,
) -> std::io::Result<()> {
    let json = serde_json::to_string(message).unwrap();
    writer.write_all(json.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;
    Ok(())
}

/// Send an error message to the client and return an error to trigger disconnection
async fn send_error_and_disconnect(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    error_message: &str,
    command: Option<&str>,
) -> std::io::Result<()> {
    let error_msg = ServerMessage::Error {
        message: error_message.to_string(),
        command: command.map(|s| s.to_string()),
    };
    let _ = send_message(writer, &error_msg).await;
    Err(std::io::Error::new(
        std::io::ErrorKind::Other,
        error_message,
    ))
}

/// Generate a random session ID
fn rand_session_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    format!("{:x}", timestamp)
}

/// Get current Unix timestamp in seconds
fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
