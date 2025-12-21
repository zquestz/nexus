//! TLS configuration and connection establishment

use std::net::ToSocketAddrs;
use std::sync::Arc;

use once_cell::sync::Lazy;
use sha2::{Digest, Sha256};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::rustls::ClientConfig;
use tokio_rustls::rustls::pki_types::ServerName;
use tokio_socks::tcp::Socks5Stream;

use crate::i18n::{t, t_args};

use super::constants::CONNECTION_TIMEOUT;
use super::types::{ProxyConfig, TlsStream};

/// Global TLS connector (accepts any certificate, no hostname verification)
pub(super) static TLS_CONNECTOR: Lazy<TlsConnector> = Lazy::new(|| {
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

/// Establish TLS connection to the server and return certificate fingerprint
///
/// If a proxy configuration is provided, the connection will be tunneled through
/// the SOCKS5 proxy. Otherwise, a direct connection is made.
///
/// Localhost/loopback addresses bypass the proxy since proxying to localhost
/// doesn't make sense (the proxy server can't reach your local machine).
pub(super) async fn establish_connection(
    address: &str,
    port: u16,
    proxy: Option<&ProxyConfig>,
) -> Result<(TlsStream, String), String> {
    // Server name for TLS (doesn't matter - we accept any cert and disable SNI)
    let server_name = ServerName::try_from("localhost").expect("'localhost' is a valid DNS name");

    // Bypass proxy for localhost/loopback addresses
    let use_proxy = proxy.filter(|_| !is_loopback_address(address));

    let (tls_stream, fingerprint) = if let Some(proxy_config) = use_proxy {
        // Connect through SOCKS5 proxy
        establish_proxied_connection(address, port, proxy_config, server_name).await?
    } else {
        // Direct connection
        establish_direct_connection(address, port, server_name).await?
    };

    Ok((tls_stream, fingerprint))
}

/// Check if an address is a loopback/localhost address that should bypass the proxy
fn is_loopback_address(address: &str) -> bool {
    let addr_lower = address.to_lowercase();

    // Check for localhost hostname
    if addr_lower == "localhost" {
        return true;
    }

    // Check for IPv4 loopback (127.x.x.x)
    if addr_lower.starts_with("127.") {
        return true;
    }

    // Check for IPv6 loopback (::1)
    // Handle formats: "::1", "[::1]", "::1%iface", "[::1%iface]"
    // Zone identifier always comes after the address (inside brackets if bracketed)
    let trimmed = addr_lower.trim_start_matches('[').trim_end_matches(']');
    let without_zone = trimmed.split('%').next().unwrap_or(trimmed);
    if without_zone == "::1" {
        return true;
    }

    false
}

/// Establish a direct TLS connection (no proxy)
async fn establish_direct_connection(
    address: &str,
    port: u16,
    server_name: ServerName<'static>,
) -> Result<(TlsStream, String), String> {
    // Use to_socket_addrs to support IPv6 zone identifiers (e.g., "fe80::1%eth0")
    let mut addrs = (address, port).to_socket_addrs().map_err(|e| {
        t_args(
            "err-invalid-address",
            &[("address", address), ("error", &e.to_string())],
        )
    })?;

    let socket_addr = addrs
        .next()
        .ok_or_else(|| t_args("err-could-not-resolve", &[("address", address)]))?;

    // Establish TCP connection with timeout
    let tcp_stream = tokio::time::timeout(CONNECTION_TIMEOUT, TcpStream::connect(socket_addr))
        .await
        .map_err(|_| {
            t_args(
                "err-connection-timeout",
                &[("seconds", &CONNECTION_TIMEOUT.as_secs().to_string())],
            )
        })?
        .map_err(|e| t_args("err-connection-failed", &[("error", &e.to_string())]))?;

    // Perform TLS handshake
    let tls_stream = TLS_CONNECTOR
        .connect(server_name, tcp_stream)
        .await
        .map_err(|e| t_args("err-tls-handshake-failed", &[("error", &e.to_string())]))?;

    // Wrap in our enum type
    let tls_stream = TlsStream::Direct(tls_stream);

    // Calculate certificate fingerprint for TOFU verification
    let fingerprint = calculate_certificate_fingerprint(&tls_stream)?;

    Ok((tls_stream, fingerprint))
}

/// Establish a TLS connection through a SOCKS5 proxy
async fn establish_proxied_connection(
    target_address: &str,
    target_port: u16,
    proxy: &ProxyConfig,
    server_name: ServerName<'static>,
) -> Result<(TlsStream, String), String> {
    let proxy_addr = format!("{}:{}", proxy.address, proxy.port);

    // Connect to the target through the SOCKS5 proxy with timeout
    let socks_stream = tokio::time::timeout(CONNECTION_TIMEOUT, async {
        match (&proxy.username, &proxy.password) {
            (Some(username), Some(password)) => {
                // Authenticated SOCKS5 connection
                Socks5Stream::connect_with_password(
                    proxy_addr.as_str(),
                    (target_address, target_port),
                    username.as_str(),
                    password.as_str(),
                )
                .await
            }
            _ => {
                // Unauthenticated SOCKS5 connection
                Socks5Stream::connect(proxy_addr.as_str(), (target_address, target_port)).await
            }
        }
    })
    .await
    .map_err(|_| {
        t_args(
            "err-proxy-connection-timeout",
            &[("seconds", &CONNECTION_TIMEOUT.as_secs().to_string())],
        )
    })?
    .map_err(|e| t_args("err-proxy-connection-failed", &[("error", &e.to_string())]))?;

    // Perform TLS handshake through the SOCKS5 tunnel
    let tls_stream = TLS_CONNECTOR
        .connect(server_name, socks_stream)
        .await
        .map_err(|e| t_args("err-tls-handshake-failed", &[("error", &e.to_string())]))?;

    // Wrap in our enum type
    let tls_stream = TlsStream::Proxied(tls_stream);

    // Calculate certificate fingerprint for TOFU verification
    let fingerprint = calculate_certificate_fingerprint(&tls_stream)?;

    Ok((tls_stream, fingerprint))
}

/// Calculate SHA-256 fingerprint of the server's certificate
fn calculate_certificate_fingerprint(tls_stream: &TlsStream) -> Result<String, String> {
    let (_io, session) = tls_stream.get_ref();
    let certs = session
        .peer_certificates()
        .ok_or_else(|| t("err-no-peer-certificates"))?;

    if certs.is_empty() {
        return Err(t("err-no-certificates-in-chain"));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_loopback_localhost() {
        assert!(is_loopback_address("localhost"));
        assert!(is_loopback_address("LOCALHOST"));
        assert!(is_loopback_address("LocalHost"));
    }

    #[test]
    fn test_is_loopback_ipv4() {
        assert!(is_loopback_address("127.0.0.1"));
        assert!(is_loopback_address("127.0.0.2"));
        assert!(is_loopback_address("127.255.255.255"));
    }

    #[test]
    fn test_is_loopback_ipv6() {
        assert!(is_loopback_address("::1"));
        assert!(is_loopback_address("[::1]"));
        assert!(is_loopback_address("::1%lo"));
        assert!(is_loopback_address("[::1%lo]"));
    }

    #[test]
    fn test_not_loopback() {
        assert!(!is_loopback_address("192.168.1.1"));
        assert!(!is_loopback_address("10.0.0.1"));
        assert!(!is_loopback_address("example.com"));
        assert!(!is_loopback_address("::2"));
        assert!(!is_loopback_address("2001:db8::1"));
        assert!(!is_loopback_address("local"));
        assert!(!is_loopback_address("localhost.localdomain"));
    }
}
