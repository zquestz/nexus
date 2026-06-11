//! Connection and authentication helpers for the transfer executor
//!
//! Handles connecting to the transfer port (7501), TLS handshake,
//! certificate fingerprint verification, and protocol authentication.

use std::net::ToSocketAddrs;
use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncWrite, BufReader, ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;
use tokio_socks::tcp::Socks5Stream;

use nexus_common::address::resolve_host_for_connection;
use nexus_common::framing::{FrameReader, FrameWriter};
use nexus_common::io::{
    read_server_handshake_response, read_server_login_response, send_client_message,
};
use nexus_common::protocol::{ClientMessage, ServerMessage};
use nexus_common::{EXPECT_SNI_SERVER_NAME_VALID_DNS, PROTOCOL_VERSION, SNI_SERVER_NAME};

use super::{CONNECTION_TIMEOUT, IDLE_TIMEOUT, TransferError};
use crate::network::{DNS_LOOKUP_TIMEOUT, ProxyConfig};
use crate::types::ConnectionInfo;

/// Boxed async read half (type alias to reduce complexity)
type BoxedRead = Box<dyn AsyncRead + Unpin + Send>;

/// Boxed async write half (type alias to reduce complexity)
type BoxedWrite = Box<dyn AsyncWrite + Unpin + Send>;

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
    // Get and verify fingerprint before splitting. A missing peer cert is
    // a TLS-layer anomaly (handshake completed without exposing a cert),
    // not a fingerprint mismatch — surface it as a connection error.
    let (_, session) = tls_stream.get_ref();
    let fingerprint = crate::network::tls::get_certificate_fingerprint(session)
        .ok_or(TransferError::ConnectionError)?;

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
        .expect(EXPECT_SNI_SERVER_NAME_VALID_DNS);

    // Check if we should bypass proxy for this address (localhost, Yggdrasil)
    let use_proxy = proxy.filter(|_| !crate::network::tls::should_bypass_proxy(target_addr));

    // IDNA-encode the Unicode hostname before handing it to either
    // the SOCKS5 proxy or the system resolver. Without this, an IDN
    // bookmark (e.g. "münchen.de") connects fine on the BBS port
    // (which already does this in `network/tls.rs`) but the matching
    // transfer port silently fails to resolve. Punycode-encoded
    // form is what every downstream consumer expects.
    let resolved_target =
        resolve_host_for_connection(target_addr).map_err(|_| TransferError::ConnectionError)?;

    // Connect and perform TLS handshake - either direct or through proxy
    let (read_half, write_half) = if let Some(proxy_config) = use_proxy {
        // Proxied connection via SOCKS5. The SOCKS5 server resolves
        // the target itself, so we just hand it the (host, port) tuple
        // — no client-side DNS lookup of the target. Proxy address
        // resolution happens inside `Socks5Stream::connect`, bounded
        // by the outer `CONNECTION_TIMEOUT` wrap.
        let proxy_addr = format!("{}:{}", proxy_config.address, proxy_config.port);

        let socks_stream = timeout(CONNECTION_TIMEOUT, async {
            match (&proxy_config.username, &proxy_config.password) {
                (Some(username), Some(password)) => {
                    Socks5Stream::connect_with_password(
                        proxy_addr.as_str(),
                        (resolved_target.as_str(), target_port),
                        username.as_str(),
                        password.as_str(),
                    )
                    .await
                }
                _ => {
                    Socks5Stream::connect(
                        proxy_addr.as_str(),
                        (resolved_target.as_str(), target_port),
                    )
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
        // Direct connection. Use `to_socket_addrs` (which supports
        // IPv6 zone identifiers like `fe80::1%eth0`) wrapped in
        // `spawn_blocking` + `DNS_LOOKUP_TIMEOUT` so a wedged
        // resolver can't park the queued transfer indefinitely.
        // Mirrors the BBS connect path's DNS handling.
        let resolved_clone = resolved_target.clone();
        let lookup = tokio::task::spawn_blocking(move || {
            (resolved_clone.as_str(), target_port)
                .to_socket_addrs()
                .map(|iter| iter.collect::<Vec<_>>())
        });
        let socket_addr = timeout(DNS_LOOKUP_TIMEOUT, lookup)
            .await
            .map_err(|_| TransferError::ConnectionError)?
            .map_err(|_| TransferError::ConnectionError)?
            .map_err(|_| TransferError::ConnectionError)?
            .into_iter()
            .next()
            .ok_or(TransferError::ConnectionError)?;

        let tcp_stream = timeout(CONNECTION_TIMEOUT, TcpStream::connect(socket_addr))
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

    let handshake_response = timeout(IDLE_TIMEOUT, read_server_handshake_response(&mut reader))
        .await
        .map_err(|_| TransferError::ConnectionError)?
        .map_err(|_| TransferError::ProtocolError)?
        .ok_or(TransferError::ConnectionError)?
        .message;

    match handshake_response {
        ServerMessage::HandshakeResponse {
            success: true,
            fingerprint: server_fingerprint,
            ..
        } => {
            // Stage 2: server-reported fingerprint must match the TLS-observed
            // value. `verify_and_split` already confirmed the TLS-observed
            // fingerprint equals `conn_info.certificate_fingerprint` (which
            // was committed via TOFU on the BBS port), so comparing the
            // server-reported value against `conn_info.certificate_fingerprint`
            // is equivalent. They always should match — same TLS cert as the
            // BBS port. A mismatch means active interception on 7501.
            // Both sides come from the canonical `format_certificate_fingerprint`,
            // so direct `!=` is correct — no normalization needed.
            if server_fingerprint != conn_info.certificate_fingerprint {
                return Err(TransferError::CertificateMismatch);
            }
        }
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

    let login_response = timeout(IDLE_TIMEOUT, read_server_login_response(&mut reader))
        .await
        .map_err(|_| TransferError::ConnectionError)?
        .map_err(|_| TransferError::ProtocolError)?
        .ok_or(TransferError::ConnectionError)?
        .message;

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
