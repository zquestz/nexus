//! Network connection and message handling

use crate::types::{Message, NetworkConnection};
use iced::futures::{SinkExt, Stream};
use iced::stream;
use nexus_common::PROTOCOL_VERSION;
use nexus_common::io::send_client_message;
use nexus_common::protocol::{ClientMessage, ServerMessage};
use sha2::{Digest, Sha256};
use std::net::ToSocketAddrs;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::{Mutex, mpsc};
use tokio_rustls::TlsConnector;
use tokio_rustls::rustls::ClientConfig;
use tokio_rustls::rustls::pki_types::ServerName;

/// Connection timeout duration (30 seconds)
const CONNECTION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Buffer size for the Iced stream channel
const STREAM_CHANNEL_SIZE: usize = 100;

/// Default features to request during login
const DEFAULT_FEATURES: &[&str] = &["chat"];

/// Type alias for TLS stream read half with buffering
type Reader = BufReader<tokio::io::ReadHalf<tokio_rustls::client::TlsStream<TcpStream>>>;

/// Type alias for TLS stream write half
type Writer = tokio::io::WriteHalf<tokio_rustls::client::TlsStream<TcpStream>>;

/// Login information returned from the server
struct LoginInfo {
    session_id: String,
    is_admin: bool,
    permissions: Vec<String>,
    chat_topic: Option<String>,
}

/// Type alias for the connection registry
type ConnectionRegistry =
    Arc<Mutex<std::collections::HashMap<usize, mpsc::UnboundedReceiver<ServerMessage>>>>;

/// Global registry for network receivers
pub static NETWORK_RECEIVERS: once_cell::sync::Lazy<ConnectionRegistry> =
    once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(std::collections::HashMap::new())));

/// Global TLS connector (accepts any certificate, no hostname verification)
static TLS_CONNECTOR: once_cell::sync::Lazy<TlsConnector> = once_cell::sync::Lazy::new(|| {
    // Create a custom certificate verifier that accepts any certificate
    let mut config = ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoVerifier))
        .with_no_client_auth();

    // Disable SNI (Server Name Indication) since we're not verifying hostnames
    config.enable_sni = false;

    TlsConnector::from(Arc::new(config))
});

/// Custom certificate verifier that accepts any certificate (no verification)
#[derive(Debug)]
struct NoVerifier;

impl tokio_rustls::rustls::client::danger::ServerCertVerifier for NoVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &tokio_rustls::rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[tokio_rustls::rustls::pki_types::CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: tokio_rustls::rustls::pki_types::UnixTime,
    ) -> Result<tokio_rustls::rustls::client::danger::ServerCertVerified, tokio_rustls::rustls::Error>
    {
        // Accept any certificate without verification
        Ok(tokio_rustls::rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &tokio_rustls::rustls::pki_types::CertificateDer<'_>,
        _dss: &tokio_rustls::rustls::DigitallySignedStruct,
    ) -> Result<
        tokio_rustls::rustls::client::danger::HandshakeSignatureValid,
        tokio_rustls::rustls::Error,
    > {
        // Accept any signature without verification
        Ok(tokio_rustls::rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &tokio_rustls::rustls::pki_types::CertificateDer<'_>,
        _dss: &tokio_rustls::rustls::DigitallySignedStruct,
    ) -> Result<
        tokio_rustls::rustls::client::danger::HandshakeSignatureValid,
        tokio_rustls::rustls::Error,
    > {
        // Accept any signature without verification
        Ok(tokio_rustls::rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<tokio_rustls::rustls::SignatureScheme> {
        // Support all signature schemes
        vec![
            tokio_rustls::rustls::SignatureScheme::RSA_PKCS1_SHA256,
            tokio_rustls::rustls::SignatureScheme::RSA_PKCS1_SHA384,
            tokio_rustls::rustls::SignatureScheme::RSA_PKCS1_SHA512,
            tokio_rustls::rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            tokio_rustls::rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            tokio_rustls::rustls::SignatureScheme::ECDSA_NISTP521_SHA512,
            tokio_rustls::rustls::SignatureScheme::RSA_PSS_SHA256,
            tokio_rustls::rustls::SignatureScheme::RSA_PSS_SHA384,
            tokio_rustls::rustls::SignatureScheme::RSA_PSS_SHA512,
            tokio_rustls::rustls::SignatureScheme::ED25519,
        ]
    }
}

/// Handle for shutting down a network connection
#[derive(Debug)]
pub struct ShutdownHandle {
    tx: tokio::sync::oneshot::Sender<()>,
}

impl ShutdownHandle {
    /// Signal the network task to shut down
    pub fn shutdown(self) {
        let _ = self.tx.send(());
    }
}

/// Connect to server, perform handshake and login
///
/// Establishes a TCP connection, performs protocol handshake and authentication,
/// then sets up bidirectional communication channels. Returns a NetworkConnection
/// handle for sending messages to the server.
pub async fn connect_to_server(
    server_address: String,
    port: u16,
    username: String,
    password: String,
    connection_id: usize,
) -> Result<NetworkConnection, String> {
    // Establish TCP connection and get certificate fingerprint
    let (stream, fingerprint) = establish_connection(&server_address, port).await?;

    let (reader, mut writer) = tokio::io::split(stream);
    let mut reader = BufReader::new(reader);

    // Perform handshake and login
    perform_handshake(&mut reader, &mut writer).await?;
    let login_info = perform_login(&mut reader, &mut writer, username, password).await?;

    // Set up bidirectional communication
    setup_communication_channels(reader, writer, login_info, connection_id, fingerprint).await
}

/// Establish TLS connection to the server and return certificate fingerprint
async fn establish_connection(
    address: &str,
    port: u16,
) -> Result<(tokio_rustls::client::TlsStream<TcpStream>, String), String> {
    // Use to_socket_addrs to support IPv6 zone identifiers (e.g., "fe80::1%eth0")
    let mut addrs = (address, port)
        .to_socket_addrs()
        .map_err(|e| format!("Invalid address '{}': {}", address, e))?;

    let socket_addr = addrs
        .next()
        .ok_or_else(|| format!("Could not resolve address '{}'", address))?;

    // Establish TCP connection
    let tcp_stream = tokio::time::timeout(CONNECTION_TIMEOUT, TcpStream::connect(socket_addr))
        .await
        .map_err(|_| {
            format!(
                "Connection timed out after {} seconds",
                CONNECTION_TIMEOUT.as_secs()
            )
        })?
        .map_err(|e| format!("Connection failed: {}", e))?;

    // Perform TLS handshake (hostname doesn't matter, we accept any cert)
    let server_name = ServerName::try_from("localhost")
        .map_err(|e| format!("Failed to create server name: {}", e))?;

    let tls_stream = TLS_CONNECTOR
        .connect(server_name, tcp_stream)
        .await
        .map_err(|e| format!("TLS handshake failed: {}", e))?;

    // Calculate certificate fingerprint for TOFU verification
    let fingerprint = calculate_certificate_fingerprint(&tls_stream)?;

    Ok((tls_stream, fingerprint))
}

/// Calculate SHA-256 fingerprint of the server's certificate
fn calculate_certificate_fingerprint(
    tls_stream: &tokio_rustls::client::TlsStream<TcpStream>,
) -> Result<String, String> {
    // Get the peer certificates from the TLS session
    let (_io, session) = tls_stream.get_ref();
    let certs = session
        .peer_certificates()
        .ok_or("No peer certificates found")?;

    if certs.is_empty() {
        return Err("No certificates in chain".to_string());
    }

    // Calculate SHA-256 fingerprint of the first certificate (end entity)
    let mut hasher = Sha256::new();
    hasher.update(certs[0].as_ref());
    let fingerprint = hasher.finalize();

    // Format as colon-separated hex string
    let fingerprint_str = fingerprint
        .iter()
        .map(|byte| format!("{:02X}", byte))
        .collect::<Vec<_>>()
        .join(":");

    Ok(fingerprint_str)
}

/// Perform protocol handshake with the server
async fn perform_handshake(reader: &mut Reader, writer: &mut Writer) -> Result<(), String> {
    // Send handshake
    let handshake = ClientMessage::Handshake {
        version: PROTOCOL_VERSION.to_string(),
    };
    send_client_message(writer, &handshake)
        .await
        .map_err(|e| format!("Failed to send handshake: {}", e))?;

    // Wait for handshake response
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .await
        .map_err(|e| format!("Failed to read handshake response: {}", e))?;

    match serde_json::from_str::<ServerMessage>(line.trim()) {
        Ok(ServerMessage::HandshakeResponse { success: true, .. }) => Ok(()),
        Ok(ServerMessage::HandshakeResponse {
            success: false,
            error,
            ..
        }) => Err(format!("Handshake failed: {}", error.unwrap_or_default())),
        Ok(_) => Err("Unexpected handshake response".to_string()),
        Err(e) => Err(format!("Failed to parse handshake response: {}", e)),
    }
}

/// Perform login and return login info (session ID, admin status, permissions)
async fn perform_login(
    reader: &mut Reader,
    writer: &mut Writer,
    username: String,
    password: String,
) -> Result<LoginInfo, String> {
    // Send login with default features
    let login = ClientMessage::Login {
        username,
        password,
        features: DEFAULT_FEATURES.iter().map(|s| s.to_string()).collect(),
    };
    send_client_message(writer, &login)
        .await
        .map_err(|e| format!("Failed to send login: {}", e))?;

    // Wait for login response
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .await
        .map_err(|e| format!("Failed to read login response: {}", e))?;

    match serde_json::from_str::<ServerMessage>(line.trim()) {
        Ok(ServerMessage::LoginResponse {
            success: true,
            session_id: Some(id),
            is_admin,
            permissions,
            server_info,
            ..
        }) => Ok(LoginInfo {
            session_id: id,
            is_admin: is_admin.unwrap_or(false),
            permissions: permissions.unwrap_or_default(),
            chat_topic: server_info.map(|info| info.chat_topic),
        }),
        Ok(ServerMessage::LoginResponse {
            success: true,
            session_id: None,
            ..
        }) => Err("No session ID received".to_string()),
        Ok(ServerMessage::LoginResponse {
            success: false,
            error: Some(msg),
            ..
        }) => Err(msg),
        Ok(ServerMessage::LoginResponse {
            success: false,
            error: None,
            ..
        }) => Err("Login failed".to_string()),
        Ok(ServerMessage::Error { message, .. }) => Err(message),
        Ok(_) => Err("Unexpected login response".to_string()),
        Err(e) => Err(format!("Failed to parse login response: {}", e)),
    }
}

/// Set up bidirectional communication channels and spawn network task
async fn setup_communication_channels(
    reader: Reader,
    writer: Writer,
    login_info: LoginInfo,
    connection_id: usize,
    fingerprint: String,
) -> Result<NetworkConnection, String> {
    // Create channels for bidirectional communication
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<ClientMessage>();
    let (msg_tx, msg_rx) = mpsc::unbounded_channel::<ServerMessage>();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    // Spawn background task for bidirectional communication
    spawn_network_task(reader, writer, cmd_rx, msg_tx, shutdown_rx);

    // Register connection in global registry with pre-assigned ID
    register_connection(connection_id, msg_rx).await;

    Ok(NetworkConnection {
        tx: cmd_tx,
        session_id: login_info.session_id,
        connection_id,
        shutdown: Some(Arc::new(Mutex::new(Some(ShutdownHandle {
            tx: shutdown_tx,
        })))),
        is_admin: login_info.is_admin,
        permissions: login_info.permissions,
        chat_topic: login_info.chat_topic,
        certificate_fingerprint: fingerprint,
    })
}

/// Spawn background task to handle bidirectional communication
fn spawn_network_task(
    mut reader: Reader,
    mut writer: Writer,
    mut cmd_rx: mpsc::UnboundedReceiver<ClientMessage>,
    msg_tx: mpsc::UnboundedSender<ServerMessage>,
    mut shutdown_rx: tokio::sync::oneshot::Receiver<()>,
) {
    tokio::spawn(async move {
        let mut line = String::new();
        loop {
            tokio::select! {
                // Read from server
                result = reader.read_line(&mut line) => {
                    match result {
                        Ok(0) => break, // Connection closed
                        Ok(_) => {
                            // Parse and send message to UI
                            if let Ok(server_msg) = serde_json::from_str::<ServerMessage>(line.trim())
                                && msg_tx.send(server_msg).is_err() {
                                    break; // UI closed
                                }
                            line.clear();
                        }
                        Err(_) => break,
                    }
                }
                // Send to server
                Some(msg) = cmd_rx.recv() => {
                    if send_client_message(&mut writer, &msg).await.is_err() {
                        break;
                    }
                }
                // Shutdown signal
                _ = &mut shutdown_rx => {
                    // Properly close TLS connection with shutdown
                    let _ = writer.shutdown().await;
                    break;
                }
            }
        }
    });
}

/// Register connection in global registry with pre-assigned ID
async fn register_connection(connection_id: usize, msg_rx: mpsc::UnboundedReceiver<ServerMessage>) {
    let mut receivers = NETWORK_RECEIVERS.lock().await;
    receivers.insert(connection_id, msg_rx);
}

/// Create Iced stream for network messages
///
/// Creates a subscription stream that receives messages from the server
/// for a specific connection. When the connection closes, sends a NetworkError
/// message and ends the stream.
pub fn network_stream(connection_id: usize) -> impl Stream<Item = Message> {
    stream::channel(STREAM_CHANNEL_SIZE, move |mut output| async move {
        // Get the receiver from the registry
        let mut rx = {
            let mut receivers = NETWORK_RECEIVERS.lock().await;
            receivers.remove(&connection_id)
        };

        if let Some(ref mut receiver) = rx {
            while let Some(msg) = receiver.recv().await {
                let _ = output
                    .send(Message::ServerMessageReceived(connection_id, msg))
                    .await;
            }
        }

        // Connection closed - send error and end stream naturally
        let _ = output
            .send(Message::NetworkError(
                connection_id,
                "Connection closed".to_string(),
            ))
            .await;

        // Stream ends naturally here, allowing Iced to clean up the subscription
    })
}
