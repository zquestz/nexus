//! TLS configuration and connection establishment

use std::net::{IpAddr, ToSocketAddrs};
use std::sync::Arc;

use nexus_common::address;
use once_cell::sync::Lazy;
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::rustls::ClientConfig;
use tokio_rustls::rustls::client::ClientConnection;
use tokio_rustls::rustls::pki_types::ServerName;
use tokio_socks::tcp::Socks5Stream;

use crate::constants::ERR_LOCALHOST_INVALID_DNS;
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

/// Create a TLS config that accepts any certificate (for TOFU model)
///
/// This is used by the transfer executor to establish connections to the
/// transfer port (7501) with the same certificate verification behavior.
pub fn create_tls_config() -> ClientConfig {
    let mut config = ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoVerifier))
        .with_no_client_auth();

    // Disable SNI (Server Name Indication) since we're not verifying hostnames
    config.enable_sni = false;

    config
}

/// Get the certificate fingerprint from a TLS session
///
/// Returns the SHA-256 fingerprint of the server's certificate as a colon-separated
/// hex string (e.g., "AA:BB:CC:...") via the workspace-canonical formatter.
pub fn get_certificate_fingerprint(session: &ClientConnection) -> Option<String> {
    let certs = session.peer_certificates()?;
    if certs.is_empty() {
        return None;
    }

    Some(nexus_common::fingerprint::format_certificate_fingerprint(
        certs[0].as_ref(),
    ))
}

/// Establish TLS connection to the server and return certificate fingerprint
///
/// If a proxy configuration is provided, the connection will be tunneled through
/// the SOCKS5 proxy. Otherwise, a direct connection is made.
///
/// Localhost/loopback addresses bypass the proxy since proxying to localhost
/// doesn't make sense (the proxy server can't reach your local machine).
pub(crate) async fn establish_connection(
    address: &str,
    port: u16,
    proxy: Option<&ProxyConfig>,
) -> Result<(TlsStream, String), String> {
    // Server name for TLS (doesn't matter - we accept any cert and disable SNI)
    let server_name = ServerName::try_from("localhost").expect(ERR_LOCALHOST_INVALID_DNS);

    // Bypass proxy for localhost/loopback and Yggdrasil addresses
    let use_proxy = proxy.filter(|_| !should_bypass_proxy(address));

    let (tls_stream, fingerprint) = if let Some(proxy_config) = use_proxy {
        // Connect through SOCKS5 proxy
        establish_proxied_connection(address, port, proxy_config, server_name).await?
    } else {
        // Direct connection
        establish_direct_connection(address, port, server_name).await?
    };

    Ok((tls_stream, fingerprint))
}

/// Whether `address` should bypass any configured SOCKS5 proxy.
///
/// String-form wrapper around [`nexus_common::address::is_proxy_bypassable`]:
/// strips IPv6 zone identifiers and surrounding brackets, treats the
/// literal `"localhost"` as loopback, and parses the result as an
/// `IpAddr` before delegating to the shared classifier. Hostnames
/// other than `"localhost"` (which require DNS to classify) return
/// `false` — they'll be routed through the proxy normally.
pub(crate) fn should_bypass_proxy(address: &str) -> bool {
    if address.to_lowercase() == "localhost" {
        return true;
    }
    address::normalize_ip_literal(address)
        .parse::<IpAddr>()
        .is_ok_and(address::is_proxy_bypassable)
}

/// Encode a hostname for DNS resolution, translating IDNA failures
/// into a localized error message.
///
/// Thin wrapper around `nexus_common::address::resolve_host_for_connection`
/// — that helper does the actual normalization (bracket strip, IP
/// passthrough, zone-ID preservation, Punycode for Unicode); this
/// version maps its `idna::Errors` into the translated client-facing
/// "invalid address" string.
fn resolve_host_for_connection(address: &str) -> Result<String, String> {
    nexus_common::address::resolve_host_for_connection(address).map_err(|e| {
        t_args(
            "err-invalid-address",
            &[("address", address), ("error", &e.to_string())],
        )
    })
}

/// Establish a direct TLS connection (no proxy)
async fn establish_direct_connection(
    address: &str,
    port: u16,
    server_name: ServerName<'static>,
) -> Result<(TlsStream, String), String> {
    // IDNA-encode Unicode hostnames before handing to the system resolver.
    let resolved = resolve_host_for_connection(address)?;
    // Use to_socket_addrs to support IPv6 zone identifiers (e.g., "fe80::1%eth0")
    let mut addrs = (resolved.as_str(), port).to_socket_addrs().map_err(|e| {
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
    // IDNA-encode Unicode hostnames before sending over SOCKS5.
    let resolved_target = resolve_host_for_connection(target_address)?;

    // Connect to the target through the SOCKS5 proxy with timeout
    let socks_stream = tokio::time::timeout(CONNECTION_TIMEOUT, async {
        match (&proxy.username, &proxy.password) {
            (Some(username), Some(password)) => {
                // Authenticated SOCKS5 connection
                Socks5Stream::connect_with_password(
                    proxy_addr.as_str(),
                    (resolved_target.as_str(), target_port),
                    username.as_str(),
                    password.as_str(),
                )
                .await
            }
            _ => {
                // Unauthenticated SOCKS5 connection
                Socks5Stream::connect(proxy_addr.as_str(), (resolved_target.as_str(), target_port))
                    .await
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

    Ok(nexus_common::fingerprint::format_certificate_fingerprint(
        certs[0].as_ref(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bypass_localhost() {
        assert!(should_bypass_proxy("localhost"));
        assert!(should_bypass_proxy("LOCALHOST"));
        assert!(should_bypass_proxy("LocalHost"));
    }

    #[test]
    fn test_bypass_ipv4_loopback() {
        assert!(should_bypass_proxy("127.0.0.1"));
        assert!(should_bypass_proxy("127.0.0.2"));
        assert!(should_bypass_proxy("127.255.255.255"));
    }

    #[test]
    fn test_bypass_ipv6_loopback() {
        assert!(should_bypass_proxy("::1"));
        assert!(should_bypass_proxy("[::1]"));
        assert!(should_bypass_proxy("::1%lo"));
        assert!(should_bypass_proxy("[::1%lo]"));
    }

    #[test]
    fn test_bypass_yggdrasil() {
        // Start of range (0200::/7)
        assert!(should_bypass_proxy("200::1"));
        assert!(should_bypass_proxy("[200::1]"));
        assert!(should_bypass_proxy("200:abcd:1234::1"));
        assert!(should_bypass_proxy("[200:abcd:1234::1]"));

        // Middle of range
        assert!(should_bypass_proxy("201::1"));
        assert!(should_bypass_proxy("2ff::1"));
        assert!(should_bypass_proxy("300::1"));
        assert!(should_bypass_proxy("3fe::1"));

        // End of range
        assert!(should_bypass_proxy("3ff::1"));
        assert!(should_bypass_proxy(
            "3ff:ffff:ffff:ffff:ffff:ffff:ffff:ffff"
        ));

        // With zone identifier
        assert!(should_bypass_proxy("200::1%eth0"));
        assert!(should_bypass_proxy("[200::1%eth0]"));

        // Case insensitive
        assert!(should_bypass_proxy("2FF::1"));
        assert!(should_bypass_proxy("3FF:ABCD::1"));
    }

    #[test]
    fn test_bypass_private_ipv4() {
        // 10.0.0.0/8
        assert!(should_bypass_proxy("10.0.0.1"));
        assert!(should_bypass_proxy("10.255.255.255"));
        assert!(should_bypass_proxy("10.50.100.200"));

        // 172.16.0.0/12
        assert!(should_bypass_proxy("172.16.0.1"));
        assert!(should_bypass_proxy("172.31.255.255"));
        assert!(should_bypass_proxy("172.20.10.5"));

        // 192.168.0.0/16
        assert!(should_bypass_proxy("192.168.0.1"));
        assert!(should_bypass_proxy("192.168.255.255"));
        assert!(should_bypass_proxy("192.168.1.100"));
    }

    #[test]
    fn test_bypass_ipv6_ula() {
        // fc00::/7 range (fc00:: to fdff::)
        assert!(should_bypass_proxy("fc00::1"));
        assert!(should_bypass_proxy("fd00::1"));
        assert!(should_bypass_proxy("fdab:cdef:1234::1"));
        assert!(should_bypass_proxy("[fd12:3456:789a::1]"));
    }

    #[test]
    fn test_not_bypass() {
        // Public IPv4
        assert!(!should_bypass_proxy("8.8.8.8"));
        assert!(!should_bypass_proxy("1.1.1.1"));

        // Just outside private ranges
        assert!(!should_bypass_proxy("11.0.0.1")); // Above 10.x.x.x
        assert!(!should_bypass_proxy("172.15.255.255")); // Below 172.16.x.x
        assert!(!should_bypass_proxy("172.32.0.1")); // Above 172.31.x.x
        assert!(!should_bypass_proxy("192.167.255.255")); // Below 192.168.x.x
        assert!(!should_bypass_proxy("192.169.0.1")); // Above 192.168.x.x

        // Hostnames
        assert!(!should_bypass_proxy("example.com"));
        assert!(!should_bypass_proxy("local"));
        assert!(!should_bypass_proxy("localhost.localdomain"));

        // Regular IPv6 (not loopback, not Yggdrasil)
        assert!(!should_bypass_proxy("::2"));
        assert!(!should_bypass_proxy("2001:db8::1"));
        assert!(!should_bypass_proxy("fe80::1"));

        // Just outside Yggdrasil range
        assert!(!should_bypass_proxy("1ff::1")); // Below 200::
        assert!(!should_bypass_proxy("400::1")); // Above 3ff::
    }
}
