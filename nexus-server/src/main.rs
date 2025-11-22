//! Nexus BBS Server

mod users;

use clap::Parser;
use nexus_common::protocol::{ClientMessage, ServerMessage, UserInfo};
use nexus_common::yggdrasil::is_yggdrasil_address;
use std::net::{Ipv6Addr, SocketAddrV6};
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
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    // Validate that the address is in the Yggdrasil range
    if !is_yggdrasil_address(&args.bind) {
        eprintln!("Error: Address {} is not in the Yggdrasil range (0200::/7)", args.bind);
        eprintln!("Yggdrasil addresses must start with 02xx: or 03xx:");
        std::process::exit(1);
    }

    println!("Nexus BBS Server v{}", env!("CARGO_PKG_VERSION"));
    println!("Binding to [{}]:{}", args.bind, args.port);

    // Create user manager
    let user_manager = UserManager::new();

    // Create socket address
    let addr = SocketAddrV6::new(args.bind, args.port, 0, 0);

    // Bind TCP listener
    let listener = match TcpListener::bind(addr).await {
        Ok(listener) => {
            println!("Successfully bound to {}", addr);
            listener
        }
        Err(e) => {
            eprintln!("Failed to bind to {}: {}", addr, e);
            std::process::exit(1);
        }
    };

    println!("Waiting for connections...");

    // Accept connections in a loop
    loop {
        match listener.accept().await {
            Ok((socket, peer_addr)) => {
                println!("New connection from: {}", peer_addr);

                let user_manager = user_manager.clone();

                // Spawn a task to handle this connection
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(socket, peer_addr, user_manager).await {
                        eprintln!("Error handling connection from {}: {}", peer_addr, e);
                    }
                    println!("Connection closed: {}", peer_addr);
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
            println!("User {} (ID {}) disconnected", user.username, id);
            
            // Broadcast disconnection to all users
            user_manager.broadcast(ServerMessage::UserDisconnected {
                user_id: id,
                username: user.username.clone(),
            }).await;
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
    peer_addr: std::net::SocketAddr,
    tx: &mpsc::UnboundedSender<ServerMessage>,
) -> std::io::Result<()> {
    let line = line.trim();
    if line.is_empty() {
        return Ok(());
    }

    println!("Received message from {}: {}", peer_addr, line);

    // Try to parse as a client message
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
                        return Err(std::io::Error::new(std::io::ErrorKind::Other, "Duplicate handshake"));
                    }

                    println!("Handshake from {}: version={}", peer_addr, version);

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
                        println!("Handshake complete with {}", peer_addr);
                    } else {
                        let response = ServerMessage::HandshakeResponse {
                            success: false,
                            version: server_version.to_string(),
                            error: Some(format!("Version mismatch: server uses {}, client uses {}", server_version, version)),
                        };
                        send_message(writer, &response).await?;
                        eprintln!("Handshake failed with {}: version mismatch", peer_addr);
                        return Err(std::io::Error::new(std::io::ErrorKind::Other, "Version mismatch"));
                    }
                }
                ClientMessage::Login { username, password: _ } => {
                    if !*handshake_complete {
                        eprintln!("Login attempt from {} without handshake", peer_addr);
                        let response = ServerMessage::LoginResponse {
                            success: false,
                            session_id: None,
                            error: Some("Handshake required".to_string()),
                        };
                        send_message(writer, &response).await?;
                        return Err(std::io::Error::new(std::io::ErrorKind::Other, "Handshake required"));
                    }

                    if user_id.is_some() {
                        eprintln!("Duplicate login attempt from {}", peer_addr);
                        let response = ServerMessage::LoginResponse {
                            success: false,
                            session_id: None,
                            error: Some("Already logged in".to_string()),
                        };
                        send_message(writer, &response).await?;
                        return Err(std::io::Error::new(std::io::ErrorKind::Other, "Already logged in"));
                    }

                    println!("Login attempt from {}: username={}", peer_addr, username);

                    // TODO: Actually validate credentials
                    // For now, accept any login
                    let session_id = format!("{}-{}", username, rand_session_id());
                    let id = user_manager.add_user(username.clone(), session_id.clone(), peer_addr, tx.clone()).await;
                    *user_id = Some(id);

                    let response = ServerMessage::LoginResponse {
                        success: true,
                        session_id: Some(session_id),
                        error: None,
                    };
                    send_message(writer, &response).await?;

                    println!("User {} logged in as ID {}", username, id);
                    
                    // Broadcast user connected event to all other users (not to themselves)
                    let user_info = UserInfo {
                        id,
                        username: username.clone(),
                        login_time: current_timestamp(),
                    };
                    user_manager.broadcast_except(id, ServerMessage::UserConnected {
                        user: user_info,
                    }).await;
                }
                ClientMessage::UserList => {
                    if user_id.is_none() {
                        eprintln!("UserList request from {} without login", peer_addr);
                        return Err(std::io::Error::new(std::io::ErrorKind::Other, "Not logged in"));
                    }

                    println!("UserList request from {}", peer_addr);

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

                    let response = ServerMessage::UserListResponse {
                        users: user_infos,
                    };
                    send_message(writer, &response).await?;
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to parse message from {}: {} - message was: {:?}", peer_addr, e, line);
            let response = ServerMessage::LoginResponse {
                success: false,
                session_id: None,
                error: Some(format!("Invalid message format: {}", e)),
            };
            send_message(writer, &response).await?;
            return Err(std::io::Error::new(std::io::ErrorKind::Other, "Parse error"));
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