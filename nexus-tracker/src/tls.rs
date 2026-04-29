//! TLS certificate management
//!
//! Loads a server certificate and key from `<data-dir>/tracker.{crt,key}`
//! when both are present; otherwise generates a fresh self-signed pair
//! on first run. Files are owner-only (`0o600`) on Unix. The cert's
//! SHA-256 fingerprint is computed and returned for handshake reporting
//! (per the tracker protocol).

use std::fs;
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::rustls::pki_types::CertificateDer;
use tracing::info;

#[cfg(unix)]
use crate::constants::TLS_FILE_MODE;
use crate::constants::{
    CERT_FILENAME, ERR_CREATE_CERT_PARAMS, ERR_CREATE_TLS_CONFIG, ERR_GENERATE_CERT,
    ERR_GENERATE_KEYPAIR, ERR_NO_CERTS_FOUND, ERR_NO_KEY_FOUND, ERR_OPEN_CERT_FILE,
    ERR_OPEN_KEY_FILE, ERR_PARSE_CERT, ERR_PARSE_KEY, ERR_RENAME_FILE, ERR_WRITE_CERT_FILE,
    ERR_WRITE_KEY_FILE, KEY_FILENAME, MSG_CERT_FINGERPRINT, MSG_CERT_GENERATED,
    MSG_GENERATING_CERT, MSG_KEY_GENERATED, TLS_CERT_COMMON_NAME,
};

/// Ensure `<data-dir>/tracker.crt` and `<data-dir>/tracker.key` exist,
/// generating a fresh self-signed pair if either is missing. Returns the
/// SHA-256 fingerprint of the certificate (in the workspace-canonical
/// 95-byte uppercase colon-separated form) for handshake reporting.
///
/// # Errors
///
/// Returns an error string (already prefixed with the relevant operator-
/// facing constant) on key generation, certificate signing, file write,
/// or PEM parse failure.
pub fn ensure_cert(data_dir: &Path) -> Result<String, String> {
    let cert_path = data_dir.join(CERT_FILENAME);
    let key_path = data_dir.join(KEY_FILENAME);

    if !cert_path.exists() || !key_path.exists() {
        info!("{}", MSG_GENERATING_CERT);
        generate_self_signed(&cert_path, &key_path)?;
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
#[allow(dead_code)] // used by the listener (next step)
pub fn build_acceptor(data_dir: &Path) -> Result<TlsAcceptor, String> {
    let cert_path = data_dir.join(CERT_FILENAME);
    let key_path = data_dir.join(KEY_FILENAME);

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

fn generate_self_signed(cert_path: &Path, key_path: &Path) -> Result<(), String> {
    use rcgen::{CertificateParams, KeyPair};

    let key_pair = KeyPair::generate().map_err(|e| format!("{}{}", ERR_GENERATE_KEYPAIR, e))?;
    let mut params =
        CertificateParams::new(vec![]).map_err(|e| format!("{}{}", ERR_CREATE_CERT_PARAMS, e))?;
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, TLS_CERT_COMMON_NAME);
    let cert = params
        .self_signed(&key_pair)
        .map_err(|e| format!("{}{}", ERR_GENERATE_CERT, e))?;

    write_secure(cert_path, cert.pem().as_bytes(), ERR_WRITE_CERT_FILE)?;
    info!("{}{}", MSG_CERT_GENERATED, cert_path.display());

    write_secure(
        key_path,
        key_pair.serialize_pem().as_bytes(),
        ERR_WRITE_KEY_FILE,
    )?;
    info!("{}{}", MSG_KEY_GENERATED, key_path.display());

    Ok(())
}

fn compute_fingerprint(cert_path: &Path) -> Result<String, String> {
    let cert_pem = fs::read_to_string(cert_path)
        .map_err(|e| format!("{}{}: {}", ERR_OPEN_CERT_FILE, cert_path.display(), e))?;
    let cert_der = pem::parse(&cert_pem).map_err(|e| format!("{}{}", ERR_PARSE_CERT, e))?;
    Ok(nexus_common::fingerprint::format_certificate_fingerprint(
        cert_der.contents(),
    ))
}

/// Atomically write `contents` to `path` with mode `0o600` on Unix.
/// Writes to `<path>.tmp` first, then renames into place. The `err_prefix`
/// is the operator-facing prefix for the underlying I/O failure.
fn write_secure(path: &Path, contents: &[u8], err_prefix: &str) -> Result<(), String> {
    let tmp = temp_path_for(path);

    {
        #[cfg(unix)]
        let mut file = {
            use std::os::unix::fs::OpenOptionsExt;
            fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(TLS_FILE_MODE)
                .open(&tmp)
                .map_err(|e| format!("{}{}: {}", err_prefix, tmp.display(), e))?
        };
        #[cfg(not(unix))]
        let mut file = fs::File::create(&tmp)
            .map_err(|e| format!("{}{}: {}", err_prefix, tmp.display(), e))?;
        file.write_all(contents)
            .map_err(|e| format!("{}{}: {}", err_prefix, tmp.display(), e))?;
    }

    fs::rename(&tmp, path).map_err(|e| format!("{}{}: {}", ERR_RENAME_FILE, path.display(), e))?;
    Ok(())
}

fn temp_path_for(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(".tmp");
    PathBuf::from(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ensure_cert_creates_files_when_missing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let fingerprint = ensure_cert(tmp.path()).expect("ensure_cert");

        assert!(tmp.path().join(CERT_FILENAME).exists(), "cert should exist");
        assert!(tmp.path().join(KEY_FILENAME).exists(), "key should exist");
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
        // Hex digits should be uppercase per the workspace formatter.
        assert!(
            !fingerprint.chars().any(|c| c.is_ascii_lowercase()),
            "fingerprint hex must be uppercase"
        );
    }

    #[test]
    fn test_ensure_cert_returns_stable_fingerprint_on_second_call() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let first = ensure_cert(tmp.path()).expect("first call (generates)");
        let second = ensure_cert(tmp.path()).expect("second call (loads existing)");
        assert_eq!(
            first, second,
            "second call should compute fingerprint from the same cert on disk"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_cert_and_key_are_owner_only_on_unix() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().expect("tempdir");
        ensure_cert(tmp.path()).expect("ensure_cert");

        for filename in [CERT_FILENAME, KEY_FILENAME] {
            let path = tmp.path().join(filename);
            let mode = fs::metadata(&path).expect("metadata").permissions().mode();
            assert_eq!(
                mode & 0o777,
                TLS_FILE_MODE,
                "{filename} should be created with 0o600"
            );
        }
    }

    #[test]
    fn test_build_acceptor_succeeds_after_ensure_cert() {
        // `build_acceptor` calls `ServerConfig::builder()` which requires
        // a default crypto provider. `main()` installs it at runtime, but
        // tests don't go through main, so install (best-effort) here.
        // Subsequent `install_default` calls return Err harmlessly.
        let _ = tokio_rustls::rustls::crypto::ring::default_provider().install_default();

        let tmp = tempfile::tempdir().expect("tempdir");
        ensure_cert(tmp.path()).expect("ensure_cert");
        build_acceptor(tmp.path()).expect("build_acceptor");
    }

    #[test]
    fn test_build_acceptor_fails_when_cert_missing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        // No ensure_cert call, so the directory is empty.
        // (`TlsAcceptor` is not `Debug`, so we can't use `expect_err`.)
        let err = build_acceptor(tmp.path())
            .err()
            .expect("should fail without files");
        assert!(
            err.starts_with(ERR_OPEN_CERT_FILE),
            "expected ERR_OPEN_CERT_FILE prefix, got: {err}"
        );
    }

    #[test]
    fn test_ensure_cert_leaves_no_tmp_files_behind() {
        let tmp = tempfile::tempdir().expect("tempdir");
        ensure_cert(tmp.path()).expect("ensure_cert");

        for filename in [CERT_FILENAME, KEY_FILENAME] {
            let path = tmp.path().join(filename);
            let tmp_path = temp_path_for(&path);
            assert!(
                !tmp_path.exists(),
                "tmp file should have been renamed away: {}",
                tmp_path.display()
            );
        }
    }
}
