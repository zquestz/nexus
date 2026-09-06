//! TLS configuration and connection establishment

use std::net::{IpAddr, ToSocketAddrs};
use std::sync::Arc;

use nexus_common::address;
use once_cell::sync::Lazy;
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::rustls::ClientConfig;
use tokio_rustls::rustls::client::ClientConnection;
use tokio_rustls::rustls::client::danger::{
    HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier,
};
use tokio_rustls::rustls::crypto::{
    WebPkiSupportedAlgorithms, verify_tls12_signature, verify_tls13_signature,
};
use tokio_rustls::rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use tokio_rustls::rustls::{DigitallySignedStruct, Error as RustlsError, SignatureScheme};
use tokio_socks::tcp::Socks5Stream;

use nexus_common::{EXPECT_SNI_SERVER_NAME_VALID_DNS, LOCALHOST_HOSTNAME, SNI_SERVER_NAME};

use super::constants::{CONNECTION_TIMEOUT, DNS_LOOKUP_TIMEOUT};
use super::types::{ConnectError, ProxyConfig, TlsStream};

/// Global TLS connector (TOFU certificate trust, verified handshake signatures).
/// Built from `create_tls_config` so the BBS and transfer ports share a single
/// TLS verification policy.
pub(super) static TLS_CONNECTOR: Lazy<TlsConnector> =
    Lazy::new(|| TlsConnector::from(Arc::new(create_tls_config())));

/// Verifies possession of the certificate's private key without a CA path.
/// Certificate trust is checked through TOFU fingerprint pinning after TLS.
#[derive(Debug)]
struct TofuVerifier {
    supported_algorithms: WebPkiSupportedAlgorithms,
}

impl ServerCertVerifier for TofuVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, RustlsError> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        verify_tls12_signature(message, cert, dss, &self.supported_algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        verify_tls13_signature(message, cert, dss, &self.supported_algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.supported_algorithms.supported_schemes()
    }
}

/// Create the client's TLS config (TOFU model: accepts any certificate, no SNI).
///
/// Handshake signatures must prove possession of the certificate's private key.
/// Single source of truth for the client's TLS verification policy — used by the
/// BBS `TLS_CONNECTOR` and by the transfer executor for the transfer port (7501).
pub fn create_tls_config() -> ClientConfig {
    let builder = ClientConfig::builder();
    let verifier = TofuVerifier {
        supported_algorithms: builder.crypto_provider().signature_verification_algorithms,
    };
    let mut config = builder
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
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

/// Establish TLS connection to the server and return certificate fingerprint.
///
/// If a proxy configuration is provided, the connection will be tunneled through
/// the SOCKS5 proxy. Otherwise, a direct connection is made.
///
/// Localhost/loopback addresses bypass the proxy since proxying to localhost
/// doesn't make sense (the proxy server can't reach your local machine).
///
/// Errors are returned typed by phase (TCP / TLS / proxy / fingerprint
/// extraction) so callers can render phase-appropriate messages
/// without parsing localized strings. The caller's renderer is
/// responsible for converting the variant to a localized message.
pub(crate) async fn establish_connection(
    address: &str,
    port: u16,
    proxy: Option<&ProxyConfig>,
) -> Result<(TlsStream, String), ConnectError> {
    // Server name for TLS (doesn't matter - we accept any cert and disable SNI)
    let server_name =
        ServerName::try_from(SNI_SERVER_NAME).expect(EXPECT_SNI_SERVER_NAME_VALID_DNS);

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
    if address.to_lowercase() == LOCALHOST_HOSTNAME {
        return true;
    }
    address::normalize_ip_literal(address)
        .parse::<IpAddr>()
        .is_ok_and(address::is_proxy_bypassable)
}

/// Establish a direct TLS connection (no proxy)
async fn establish_direct_connection(
    address: &str,
    port: u16,
    server_name: ServerName<'static>,
) -> Result<(TlsStream, String), ConnectError> {
    // IDNA-encode Unicode hostnames before handing to the system resolver.
    let resolved = nexus_common::address::resolve_host_for_connection(address).map_err(|e| {
        ConnectError::InvalidAddress {
            address: address.to_string(),
            error: e.to_string(),
        }
    })?;

    // Use to_socket_addrs to support IPv6 zone identifiers (e.g.,
    // "fe80::1%eth0"). It's a sync blocking call, so we hand it to a
    // dedicated `spawn_blocking` worker and bound the wait with
    // `DNS_LOOKUP_TIMEOUT` — a wedged system resolver otherwise hangs
    // the connect (and the tracker query) indefinitely. Mirrors the
    // BBS-server tracker task's `DNS_LOOKUP_TIMEOUT` pattern.
    let resolved_clone = resolved.clone();
    let lookup = tokio::task::spawn_blocking(move || {
        (resolved_clone.as_str(), port)
            .to_socket_addrs()
            .map(|iter| iter.collect::<Vec<_>>())
    });
    let addrs = tokio::time::timeout(DNS_LOOKUP_TIMEOUT, lookup)
        .await
        .map_err(|_| ConnectError::DnsTimeout {
            address: address.to_string(),
        })?
        .map_err(|e| ConnectError::InvalidAddress {
            address: address.to_string(),
            error: e.to_string(),
        })?
        .map_err(|e| ConnectError::InvalidAddress {
            address: address.to_string(),
            error: e.to_string(),
        })?;

    let socket_addr = addrs
        .into_iter()
        .next()
        .ok_or_else(|| ConnectError::CouldNotResolve {
            address: address.to_string(),
        })?;

    // Establish TCP connection with timeout
    let tcp_stream = tokio::time::timeout(CONNECTION_TIMEOUT, TcpStream::connect(socket_addr))
        .await
        .map_err(|_| ConnectError::TcpTimeout)?
        .map_err(|e| ConnectError::TcpFailed {
            error: e.to_string(),
        })?;

    // Perform TLS handshake — bound by `CONNECTION_TIMEOUT` so a peer
    // that completes TCP and stalls TLS (no ServerHello / mid-handshake
    // wedge) can't park the await indefinitely. Without this, a hostile
    // tracker compounds with the in-flight task leak: every leaked
    // task holds a socket forever.
    let tls_stream = tokio::time::timeout(
        CONNECTION_TIMEOUT,
        TLS_CONNECTOR.connect(server_name, tcp_stream),
    )
    .await
    .map_err(|_| ConnectError::TlsHandshakeTimeout)?
    .map_err(|e| ConnectError::TlsHandshake {
        error: e.to_string(),
    })?;

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
) -> Result<(TlsStream, String), ConnectError> {
    let proxy_addr = format!("{}:{}", proxy.address, proxy.port);
    // IDNA-encode Unicode hostnames before sending over SOCKS5.
    let resolved_target = nexus_common::address::resolve_host_for_connection(target_address)
        .map_err(|e| ConnectError::InvalidAddress {
            address: target_address.to_string(),
            error: e.to_string(),
        })?;

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
    .map_err(|_| ConnectError::ProxyTimeout)?
    .map_err(|e| ConnectError::ProxyFailed {
        error: e.to_string(),
    })?;

    // Perform TLS handshake through the SOCKS5 tunnel — same
    // `CONNECTION_TIMEOUT` bound as the direct path; see the comment
    // there.
    let tls_stream = tokio::time::timeout(
        CONNECTION_TIMEOUT,
        TLS_CONNECTOR.connect(server_name, socks_stream),
    )
    .await
    .map_err(|_| ConnectError::TlsHandshakeTimeout)?
    .map_err(|e| ConnectError::TlsHandshake {
        error: e.to_string(),
    })?;

    // Wrap in our enum type
    let tls_stream = TlsStream::Proxied(tls_stream);

    // Calculate certificate fingerprint for TOFU verification
    let fingerprint = calculate_certificate_fingerprint(&tls_stream)?;

    Ok((tls_stream, fingerprint))
}

/// Calculate SHA-256 fingerprint of the server's certificate.
/// Returns [`ConnectError::NoCertificates`] when the TLS session
/// reports no peer certificates — should not happen in practice
/// (the handshake succeeded, so rustls saw at least one) but rustls's
/// API is `Option`-shaped so we handle the case explicitly.
fn calculate_certificate_fingerprint(tls_stream: &TlsStream) -> Result<String, ConnectError> {
    let (_io, session) = tls_stream.get_ref();
    let certs = session
        .peer_certificates()
        .ok_or(ConnectError::NoCertificates)?;

    if certs.is_empty() {
        return Err(ConnectError::NoCertificates);
    }

    Ok(nexus_common::fingerprint::format_certificate_fingerprint(
        certs[0].as_ref(),
    ))
}

#[cfg(test)]
mod tests {
    use rcgen::{CertificateParams, KeyPair};
    use tokio_rustls::rustls::crypto::ring;
    use tokio_rustls::rustls::pki_types::PrivatePkcs8KeyDer;
    use tokio_rustls::rustls::sign::{CertifiedKey, SingleCertAndKey};
    use tokio_rustls::rustls::version::{TLS12, TLS13};
    use tokio_rustls::rustls::{
        CertificateError, InconsistentKeys, ServerConfig, ServerConnection,
        SupportedProtocolVersion,
    };

    use super::*;

    const MAX_HANDSHAKE_ROUNDS: usize = 8;

    fn handshake(
        version: &'static SupportedProtocolVersion,
        is_wrong_key: bool,
    ) -> Result<(), RustlsError> {
        let _ = ring::default_provider().install_default();
        let client_config = create_tls_config();
        assert!(!client_config.enable_sni);
        let provider = Arc::clone(client_config.crypto_provider());

        let certificate_key = KeyPair::generate().expect("generate certificate key");
        let certificate = CertificateParams::new(vec!["certificate.example".to_owned()])
            .expect("create certificate parameters")
            .self_signed(&certificate_key)
            .expect("generate self-signed certificate");
        let signing_key = if is_wrong_key {
            KeyPair::generate().expect("generate unrelated signing key")
        } else {
            certificate_key
        };
        let signing_key = provider
            .key_provider
            .load_private_key(PrivatePkcs8KeyDer::from(signing_key.serialize_der()).into())
            .expect("load signing key");
        let certified_key = CertifiedKey::new(vec![certificate.der().clone()], signing_key);
        if is_wrong_key {
            assert_eq!(
                certified_key.keys_match(),
                Err(RustlsError::InconsistentKeys(InconsistentKeys::KeyMismatch))
            );
        } else {
            certified_key
                .keys_match()
                .expect("matching certificate key");
        }

        // The resolver deliberately permits a mismatched key, so the client
        // must reject the handshake rather than the server rejecting setup.
        let server_config = ServerConfig::builder_with_provider(provider)
            .with_protocol_versions(&[version])
            .expect("enable requested TLS version")
            .with_no_client_auth()
            .with_cert_resolver(Arc::new(SingleCertAndKey::from(certified_key)));
        let mut server = ServerConnection::new(Arc::new(server_config)).expect("create server");
        let mut client = ClientConnection::new(
            Arc::new(client_config),
            ServerName::try_from("different.example").expect("valid test hostname"),
        )
        .expect("create client");

        for _ in 0..MAX_HANDSHAKE_ROUNDS {
            let mut client_records = Vec::new();
            client
                .write_tls(&mut client_records)
                .expect("write client TLS records");
            let mut incoming = client_records.as_slice();
            while !incoming.is_empty() {
                assert!(server.read_tls(&mut incoming).expect("read client records") > 0);
                server
                    .process_new_packets()
                    .expect("server accepts client handshake");
            }

            let mut server_records = Vec::new();
            server
                .write_tls(&mut server_records)
                .expect("write server TLS records");
            let mut incoming = server_records.as_slice();
            while !incoming.is_empty() {
                assert!(client.read_tls(&mut incoming).expect("read server records") > 0);
                client.process_new_packets()?;
            }

            if !client.is_handshaking() && !server.is_handshaking() {
                assert_eq!(client.protocol_version(), Some(version.version));
                assert_eq!(server.protocol_version(), Some(version.version));
                assert!(server.server_name().is_none());
                assert_eq!(
                    client.peer_certificates(),
                    Some(std::slice::from_ref(certificate.der()))
                );
                assert_eq!(
                    get_certificate_fingerprint(&client),
                    Some(nexus_common::fingerprint::format_certificate_fingerprint(
                        certificate.der().as_ref()
                    ))
                );
                return Ok(());
            }
        }
        panic!("handshake did not complete within the round limit");
    }

    #[test]
    fn test_tls12_accepts_self_signed_handshake() {
        handshake(&TLS12, false).expect("valid TLS 1.2 handshake");
    }

    #[test]
    fn test_tls13_accepts_self_signed_handshake() {
        handshake(&TLS13, false).expect("valid TLS 1.3 handshake");
    }

    #[test]
    fn test_tls12_rejects_wrong_signing_key() {
        assert_eq!(
            handshake(&TLS12, true),
            Err(RustlsError::InvalidCertificate(
                CertificateError::BadSignature
            ))
        );
    }

    #[test]
    fn test_tls13_rejects_wrong_signing_key() {
        assert_eq!(
            handshake(&TLS13, true),
            Err(RustlsError::InvalidCertificate(
                CertificateError::BadSignature
            ))
        );
    }

    #[test]
    fn test_verify_schemes_follow_supplied_algorithms() {
        let mut supported_algorithms = ring::default_provider().signature_verification_algorithms;
        supported_algorithms.mapping = &supported_algorithms.mapping[1..];
        let verifier = TofuVerifier {
            supported_algorithms,
        };
        assert_eq!(
            verifier.supported_verify_schemes(),
            supported_algorithms.supported_schemes()
        );
    }

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
