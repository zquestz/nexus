//! Shared TLS certificate lifecycle for Nexus daemons.
//!
//! [`ensure_cert`] checks the data directory for a cert + key, generates
//! a fresh self-signed pair (atomically, with `0o600` mode set at file
//! creation on Unix) if either is missing, and returns the SHA-256
//! fingerprint of the certificate in the workspace-canonical 95-byte
//! uppercase form. [`build_acceptor`] turns the on-disk pair into a
//! `tokio-rustls` [`TlsAcceptor`].
//!
//! Per-daemon variations — the cert and key filenames and the
//! certificate Common Name — are passed in via [`TlsCertConfig`]. The
//! operator-facing log/error strings emitted from this module are
//! generic to TLS (not daemon-specific), so they live here as `pub
//! const`s rather than per-daemon constants.

use std::fs;
use std::io::{self, BufReader};
use std::path::Path;
use std::sync::Arc;

use rcgen::{CertificateParams, KeyPair};
use time::{Duration, OffsetDateTime};
use tokio::net::TcpStream;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::rustls::pki_types::CertificateDer;
use tracing::info;

use crate::secure_file;

/// Validity period (in days) for auto-generated self-signed certificates.
///
/// 10 years is the standard "long but not absurd" choice for self-signed
/// daemon certs. Long enough that operators don't deal with TOFU
/// re-acceptance churn during normal multi-year deployments; short
/// enough that a leaked historical cert/key has bounded exposure
/// (compared to rcgen's bare default, which is valid through year 4096).
pub const CERT_VALIDITY_DAYS: i64 = 365 * 10;

/// Status: about to generate a fresh self-signed certificate.
pub const MSG_GENERATING_CERT: &str = "Generating self-signed TLS certificate...";

/// Status: certificate file successfully written (caller appends path).
pub const MSG_CERT_GENERATED: &str = "Certificate generated: ";

/// Status: private key file successfully written (caller appends path).
pub const MSG_KEY_GENERATED: &str = "Private key generated: ";

/// Status: certificate fingerprint display (caller appends 95-char fingerprint).
pub const MSG_CERT_FINGERPRINT: &str = "Certificate fingerprint (SHA-256): ";

/// Key pair generation failed (caller appends underlying error).
pub const ERR_GENERATE_KEYPAIR: &str = "Failed to generate key pair: ";

/// Certificate parameter creation failed (caller appends underlying error).
pub const ERR_CREATE_CERT_PARAMS: &str = "Failed to create certificate parameters: ";

/// Certificate signing failed (caller appends underlying error).
pub const ERR_GENERATE_CERT: &str = "Failed to generate certificate: ";

/// Certificate file write failed (caller appends path and underlying error).
pub const ERR_WRITE_CERT_FILE: &str = "Failed to write certificate file: ";

/// Private key file write failed (caller appends path and underlying error).
pub const ERR_WRITE_KEY_FILE: &str = "Failed to write private key file: ";

/// Certificate file open failed (caller appends path and underlying error).
pub const ERR_OPEN_CERT_FILE: &str = "Failed to open certificate file: ";

/// Certificate PEM parsing failed (caller appends underlying error).
pub const ERR_PARSE_CERT: &str = "Failed to parse certificate: ";

/// Certificate file contained no certificates.
pub const ERR_NO_CERTS_FOUND: &str = "No certificates found in certificate file";

/// Private key file open failed (caller appends path and underlying error).
pub const ERR_OPEN_KEY_FILE: &str = "Failed to open private key file: ";

/// Private key PEM parsing failed (caller appends underlying error).
pub const ERR_PARSE_KEY: &str = "Failed to parse private key: ";

/// Private key file contained no keys.
pub const ERR_NO_KEY_FOUND: &str = "No private key found in key file";

/// rustls `ServerConfig` construction failed (caller appends underlying error).
pub const ERR_CREATE_TLS_CONFIG: &str = "Failed to create TLS configuration: ";

/// Per-daemon TLS configuration: filenames within the data directory
/// and the Common Name embedded in the auto-generated self-signed
/// certificate. The same value is passed to both [`ensure_cert`] and
/// [`build_acceptor`] — typically constructed once at startup from the
/// daemon's `constants.rs`.
#[derive(Clone, Copy, Debug)]
pub struct TlsCertConfig<'a> {
    /// Certificate filename within the data directory
    /// (e.g., `"server.crt"` for the BBS server, `"tracker.crt"` for
    /// the tracker).
    pub cert_filename: &'a str,
    /// Private key filename within the data directory.
    pub key_filename: &'a str,
    /// Common Name embedded in the auto-generated self-signed certificate
    /// (e.g., `"Nexus BBS Server"`, `"Nexus Tracker"`).
    pub common_name: &'a str,
}

/// Ensure `<data_dir>/<cert_filename>` and `<data_dir>/<key_filename>`
/// exist, generating a fresh self-signed pair if either is missing.
/// Returns the SHA-256 fingerprint of the certificate (in the
/// workspace-canonical 95-byte uppercase colon-separated form) for
/// handshake reporting.
///
/// On Unix the cert + key are written via
/// [`crate::secure_file::write_atomic`] — created with mode `0o600`
/// atomically at open time, so there is no window where a fresh
/// cert / key is world-readable.
///
/// # Errors
///
/// Returns an error string (already prefixed with the relevant operator-
/// facing constant) on key generation, certificate signing, file write,
/// or PEM parse failure.
pub fn ensure_cert(data_dir: &Path, config: TlsCertConfig<'_>) -> Result<String, String> {
    let cert_path = data_dir.join(config.cert_filename);
    let key_path = data_dir.join(config.key_filename);

    if !cert_path.exists() || !key_path.exists() {
        info!("{}", MSG_GENERATING_CERT);
        generate_self_signed(&cert_path, &key_path, config.common_name)?;
    }
    let fingerprint = compute_fingerprint(&cert_path)?;
    info!("{}{}", MSG_CERT_FINGERPRINT, fingerprint);
    Ok(fingerprint)
}

/// Build a `TlsAcceptor` from the on-disk cert+key. The caller is
/// responsible for having ensured the files exist (typically via
/// [`ensure_cert`]).
///
/// # Errors
///
/// Returns an error string if either file is missing, malformed, or
/// rejected by rustls.
pub fn build_acceptor(data_dir: &Path, config: TlsCertConfig<'_>) -> Result<TlsAcceptor, String> {
    let cert_path = data_dir.join(config.cert_filename);
    let key_path = data_dir.join(config.key_filename);

    let cert_file = fs::File::open(&cert_path)
        .map_err(|e| format!("{}{}: {}", ERR_OPEN_CERT_FILE, cert_path.display(), e))?;
    let mut cert_reader = BufReader::new(cert_file);
    let certs: Vec<CertificateDer> = rustls_pemfile::certs(&mut cert_reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("{}{}", ERR_PARSE_CERT, e))?;
    if certs.is_empty() {
        return Err(ERR_NO_CERTS_FOUND.to_string());
    }

    let key_file = fs::File::open(&key_path)
        .map_err(|e| format!("{}{}: {}", ERR_OPEN_KEY_FILE, key_path.display(), e))?;
    let mut key_reader = BufReader::new(key_file);
    let private_key = rustls_pemfile::private_key(&mut key_reader)
        .map_err(|e| format!("{}{}", ERR_PARSE_KEY, e))?
        .ok_or(ERR_NO_KEY_FOUND)?;

    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, private_key)
        .map_err(|e| format!("{}{}", ERR_CREATE_TLS_CONFIG, e))?;
    Ok(TlsAcceptor::from(Arc::new(config)))
}

/// Accept a TLS connection with a slowloris-defense timeout.
///
/// Wraps [`TlsAcceptor::accept`] in [`crate::TLS_HANDSHAKE_TIMEOUT`]. On
/// elapse or handshake error, returns an `io::Error` whose message is
/// prefixed with [`crate::TLS_HANDSHAKE_FAILED_PREFIX`] so the per-daemon
/// `log_connection_error` downgrades scanner / incompatible-client noise
/// to debug level.
///
/// # Errors
///
/// Returns `io::Error` on timeout or rustls handshake failure. The
/// connection is dropped silently — at this layer there's no encrypted
/// channel to send a meaningful response over.
pub async fn accept_tls_with_timeout(
    acceptor: &TlsAcceptor,
    stream: TcpStream,
) -> io::Result<tokio_rustls::server::TlsStream<TcpStream>> {
    match tokio::time::timeout(crate::TLS_HANDSHAKE_TIMEOUT, acceptor.accept(stream)).await {
        Ok(Ok(s)) => Ok(s),
        Ok(Err(e)) => Err(io::Error::other(format!(
            "{}{}",
            crate::TLS_HANDSHAKE_FAILED_PREFIX,
            e
        ))),
        Err(_) => Err(io::Error::other(crate::TLS_HANDSHAKE_TIMEOUT_MSG)),
    }
}

fn generate_self_signed(
    cert_path: &Path,
    key_path: &Path,
    common_name: &str,
) -> Result<(), String> {
    let key_pair = KeyPair::generate().map_err(|e| format!("{}{}", ERR_GENERATE_KEYPAIR, e))?;
    let mut params =
        CertificateParams::new(vec![]).map_err(|e| format!("{}{}", ERR_CREATE_CERT_PARAMS, e))?;
    // Set validity explicitly. Rcgen's default is 1975-01-01 through
    // 4096-01-01 (~2,121 years), which is a security smell on a TOFU
    // model — a leaked historical key would never expire. Cap at
    // [`CERT_VALIDITY_DAYS`] (10 years) instead.
    let now = OffsetDateTime::now_utc();
    params.not_before = now;
    params.not_after = now + Duration::days(CERT_VALIDITY_DAYS);
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, common_name);
    let cert = params
        .self_signed(&key_pair)
        .map_err(|e| format!("{}{}", ERR_GENERATE_CERT, e))?;

    secure_file::write_atomic(cert_path, cert.pem().as_bytes())
        .map_err(|e| format!("{}{}: {}", ERR_WRITE_CERT_FILE, cert_path.display(), e))?;
    info!("{}{}", MSG_CERT_GENERATED, cert_path.display());

    secure_file::write_atomic(key_path, key_pair.serialize_pem().as_bytes())
        .map_err(|e| format!("{}{}: {}", ERR_WRITE_KEY_FILE, key_path.display(), e))?;
    info!("{}{}", MSG_KEY_GENERATED, key_path.display());

    Ok(())
}

fn compute_fingerprint(cert_path: &Path) -> Result<String, String> {
    let cert_pem = fs::read_to_string(cert_path)
        .map_err(|e| format!("{}{}: {}", ERR_OPEN_CERT_FILE, cert_path.display(), e))?;
    let cert_der = pem::parse(&cert_pem).map_err(|e| format!("{}{}", ERR_PARSE_CERT, e))?;
    Ok(crate::fingerprint::format_certificate_fingerprint(
        cert_der.contents(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> TlsCertConfig<'static> {
        TlsCertConfig {
            cert_filename: "test.crt",
            key_filename: "test.key",
            common_name: "Nexus Test",
        }
    }

    #[test]
    fn ensure_cert_creates_files_when_missing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let cfg = test_config();
        let fingerprint = ensure_cert(tmp.path(), cfg).expect("ensure_cert");

        assert!(
            tmp.path().join(cfg.cert_filename).exists(),
            "cert should exist"
        );
        assert!(
            tmp.path().join(cfg.key_filename).exists(),
            "key should exist"
        );
        // 32 hex pairs separated by 31 colons = 95 ASCII bytes.
        assert_eq!(
            fingerprint.len(),
            95,
            "fingerprint should be the canonical 95-char form, got {fingerprint:?}"
        );
        assert!(
            fingerprint
                .bytes()
                .all(|b| b.is_ascii_hexdigit() || b == b':'),
            "fingerprint should be uppercase hex pairs separated by colons"
        );
        assert!(
            !fingerprint.chars().any(|c| c.is_ascii_lowercase()),
            "fingerprint hex must be uppercase"
        );
    }

    #[test]
    fn ensure_cert_returns_stable_fingerprint_on_second_call() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let cfg = test_config();
        let first = ensure_cert(tmp.path(), cfg).expect("first call (generates)");
        let second = ensure_cert(tmp.path(), cfg).expect("second call (loads existing)");
        assert_eq!(
            first, second,
            "second call should compute fingerprint from the same cert on disk"
        );
    }

    #[cfg(unix)]
    #[test]
    fn cert_and_key_are_owner_only_on_unix() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().expect("tempdir");
        let cfg = test_config();
        ensure_cert(tmp.path(), cfg).expect("ensure_cert");

        for filename in [cfg.cert_filename, cfg.key_filename] {
            let path = tmp.path().join(filename);
            let mode = fs::metadata(&path).expect("metadata").permissions().mode();
            assert_eq!(
                mode & 0o777,
                secure_file::SECURE_FILE_MODE,
                "{filename} should be created with 0o600"
            );
        }
    }

    #[test]
    fn build_acceptor_succeeds_after_ensure_cert() {
        // `build_acceptor` calls `ServerConfig::builder()` which requires
        // a default crypto provider. `main()` installs it at runtime, but
        // tests don't go through main, so install (best-effort) here.
        // Subsequent `install_default` calls return Err harmlessly.
        let _ = tokio_rustls::rustls::crypto::ring::default_provider().install_default();

        let tmp = tempfile::tempdir().expect("tempdir");
        let cfg = test_config();
        ensure_cert(tmp.path(), cfg).expect("ensure_cert");
        build_acceptor(tmp.path(), cfg).expect("build_acceptor");
    }

    #[test]
    fn build_acceptor_fails_when_cert_missing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let cfg = test_config();
        // No ensure_cert call, so the directory is empty.
        // (`TlsAcceptor` is not `Debug`, so we can't use `expect_err`.)
        let err = build_acceptor(tmp.path(), cfg)
            .err()
            .expect("should fail without files");
        assert!(
            err.starts_with(ERR_OPEN_CERT_FILE),
            "expected ERR_OPEN_CERT_FILE prefix, got: {err}"
        );
    }
}
