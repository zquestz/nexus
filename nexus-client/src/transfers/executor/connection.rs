//! Connection and authentication helpers for the transfer executor
//!
//! Handles connecting to the transfer port (7501), TLS handshake,
//! certificate fingerprint verification, and protocol authentication.

use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncWrite, BufReader, ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;
use tokio_socks::tcp::Socks5Stream;

use nexus_common::PROTOCOL_VERSION;
use nexus_common::framing::{FrameReader, FrameWriter};
use nexus_common::io::send_client_message;
use nexus_common::protocol::{ClientMessage, ServerMessage};

use super::streaming::read_message_with_timeout;
use super::{CONNECTION_TIMEOUT, IDLE_TIMEOUT, TransferError};
use crate::network::ProxyConfig;
use crate::types::ConnectionInfo;

/// Boxed async read half (type alias to reduce complexity)
type BoxedRead = Box<dyn AsyncRead + Unpin + Send>;

/// Boxed async write half (type alias to reduce complexity)
type BoxedWrite = Box<dyn AsyncWrite + Unpin + Send>;

// =============================================================================
// Constants
// =============================================================================

/// SNI server name for TLS connections
///
/// We use "localhost" since we disable hostname verification (we verify via
/// certificate fingerprint instead). This is required by the TLS handshake
/// but not used for actual verification.
const SNI_SERVER_NAME: &str = "localhost";

// =============================================================================
// TLS Helpers
// =============================================================================

/// Verify certificate fingerprint and split TLS stream into read/write halves
///
/// This helper reduces duplication between direct and proxied connection paths.
fn verify_and_split<S>(
    tls_stream: TlsStream<S>,
    expected_fingerprint: &str,
) -> Result<(BoxedRead, BoxedWrite), TransferError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    ReadHalf<TlsStream<S>>: Send,
    WriteHalf<TlsStream<S>>: Send,
{
    // Get and verify fingerprint before splitting
    let (_, session) = tls_stream.get_ref();
    let fingerprint = crate::network::tls::get_certificate_fingerprint(session)
        .ok_or(TransferError::CertificateMismatch)?;

    if fingerprint != expected_fingerprint {
        return Err(TransferError::CertificateMismatch);
    }

    let (r, w) = tokio::io::split(tls_stream);
    Ok((Box::new(r), Box::new(w)))
}

// =============================================================================
// Connection
// =============================================================================

/// Connect to transfer port, verify certificate, and authenticate
///
/// Returns boxed trait objects for the reader/writer to support both direct
/// and proxied connections with different underlying stream types.
pub async fn connect_and_authenticate(
    conn_info: &ConnectionInfo,
    proxy: Option<ProxyConfig>,
) -> Result<(FrameReader<BufReader<BoxedRead>>, FrameWriter<BoxedWrite>), TransferError> {
    let target_addr = &conn_info.address;
    let target_port = conn_info.transfer_port;

    // Set up TLS config
    let tls_config = crate::network::tls::create_tls_config();
    let connector = TlsConnector::from(Arc::new(tls_config));

    let server_name = SNI_SERVER_NAME
        .try_into()
        .expect("SNI_SERVER_NAME is valid");

    // Check if we should bypass proxy for this address (localhost, Yggdrasil)
    let use_proxy = proxy.filter(|_| !crate::network::tls::should_bypass_proxy(target_addr));

    // Connect and perform TLS handshake - either direct or through proxy
    let (read_half, write_half) = if let Some(proxy_config) = use_proxy {
        // Proxied connection via SOCKS5
        let proxy_addr = format!("{}:{}", proxy_config.address, proxy_config.port);

        let socks_stream = timeout(CONNECTION_TIMEOUT, async {
            match (&proxy_config.username, &proxy_config.password) {
                (Some(username), Some(password)) => {
                    Socks5Stream::connect_with_password(
                        proxy_addr.as_str(),
                        (target_addr.as_str(), target_port),
                        username.as_str(),
                        password.as_str(),
                    )
                    .await
                }
                _ => {
                    Socks5Stream::connect(proxy_addr.as_str(), (target_addr.as_str(), target_port))
                        .await
                }
            }
        })
        .await
        .map_err(|_| TransferError::ConnectionError)?
        .map_err(|_| TransferError::ConnectionError)?;

        let tls_stream = timeout(
            CONNECTION_TIMEOUT,
            connector.connect(server_name, socks_stream),
        )
        .await
        .map_err(|_| TransferError::ConnectionError)?
        .map_err(|_| TransferError::ConnectionError)?;

        verify_and_split(tls_stream, &conn_info.certificate_fingerprint)?
    } else {
        // Direct connection
        let addr = format!("{}:{}", target_addr, target_port);

        let tcp_stream = timeout(CONNECTION_TIMEOUT, TcpStream::connect(&addr))
            .await
            .map_err(|_| TransferError::ConnectionError)?
            .map_err(|_| TransferError::ConnectionError)?;

        let tls_stream = timeout(
            CONNECTION_TIMEOUT,
            connector.connect(server_name, tcp_stream),
        )
        .await
        .map_err(|_| TransferError::ConnectionError)?
        .map_err(|_| TransferError::ConnectionError)?;

        verify_and_split(tls_stream, &conn_info.certificate_fingerprint)?
    };

    // Set up framing
    let buf_reader = BufReader::new(read_half);
    let mut reader = FrameReader::new(buf_reader);
    let mut writer = FrameWriter::new(write_half);

    // Perform handshake
    let handshake = ClientMessage::Handshake {
        version: PROTOCOL_VERSION.to_string(),
    };
    send_client_message(&mut writer, &handshake)
        .await
        .map_err(|_| TransferError::ConnectionError)?;

    let handshake_response = read_message_with_timeout(&mut reader, IDLE_TIMEOUT).await?;

    match handshake_response {
        ServerMessage::HandshakeResponse { success: true, .. } => {}
        ServerMessage::HandshakeResponse { success: false, .. } => {
            return Err(TransferError::UnsupportedVersion);
        }
        _ => {
            return Err(TransferError::ProtocolError);
        }
    }

    // Perform login
    let login = ClientMessage::Login {
        username: conn_info.username.clone(),
        password: conn_info.password.clone(),
        features: vec![],
        locale: String::new(),
        avatar: None,
        nickname: if conn_info.nickname.is_empty() {
            None
        } else {
            Some(conn_info.nickname.clone())
        },
    };
    send_client_message(&mut writer, &login)
        .await
        .map_err(|_| TransferError::ConnectionError)?;

    let login_response = read_message_with_timeout(&mut reader, IDLE_TIMEOUT).await?;

    match login_response {
        ServerMessage::LoginResponse { success: true, .. } => {}
        ServerMessage::LoginResponse { success: false, .. } => {
            return Err(TransferError::AuthenticationFailed);
        }
        _ => {
            return Err(TransferError::ProtocolError);
        }
    }

    Ok((reader, writer))
}
