//! TLS configuration and connection establishment

use std::net::ToSocketAddrs;
use std::sync::Arc;

use once_cell::sync::Lazy;
use sha2::{Digest, Sha256};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::rustls::ClientConfig;
use tokio_rustls::rustls::pki_types::ServerName;

use crate::i18n::{t, t_args};

use super::constants::CONNECTION_TIMEOUT;
use super::types::TlsStream;

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
pub(super) async fn establish_connection(
    address: &str,
    port: u16,
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

    // Establish TCP connection
    let tcp_stream = tokio::time::timeout(CONNECTION_TIMEOUT, TcpStream::connect(socket_addr))
        .await
        .map_err(|_| {
            t_args(
                "err-connection-timeout",
                &[("seconds", &CONNECTION_TIMEOUT.as_secs().to_string())],
            )
        })?
        .map_err(|e| t_args("err-connection-failed", &[("error", &e.to_string())]))?;

    // Perform TLS handshake (hostname doesn't matter, we accept any cert)
    let server_name = ServerName::try_from("localhost").map_err(|e| {
        t_args(
            "err-failed-create-server-name",
            &[("error", &e.to_string())],
        )
    })?;

    let tls_stream = TLS_CONNECTOR
        .connect(server_name, tcp_stream)
        .await
        .map_err(|e| t_args("err-tls-handshake-failed", &[("error", &e.to_string())]))?;

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
