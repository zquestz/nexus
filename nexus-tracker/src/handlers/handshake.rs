//! Handshake handler.
//!
//! Compatibility uses `check_compatibility` against
//! `TRACKER_PROTOCOL_VERSION` — the tracker protocol is versioned
//! independently from the BBS protocol.

use std::io;

use tokio::io::AsyncWrite;
use tracing::warn;

use nexus_common::TRACKER_PROTOCOL_VERSION;
use nexus_common::framing::FrameWriter;
use nexus_common::io::send_server_message;
use nexus_common::protocol::ServerMessage;
use nexus_common::validators;
use nexus_common::version::{self, CompatibilityResult};

use crate::constants::{
    LOG_HANDSHAKE_CLIENT_TOO_NEW, LOG_HANDSHAKE_MAJOR_MISMATCH, LOG_HANDSHAKE_MINOR_MISMATCH,
};
use crate::errors::{err_tracker_handshake_version_invalid, err_tracker_protocol_version_mismatch};

/// Handle a `Handshake` request: send `HandshakeResponse` on every path
/// (success, version invalid, version mismatch) and return whether the
/// handshake succeeded.
pub async fn handle_handshake<W>(
    version: &str,
    locale: &str,
    fingerprint: &str,
    writer: &mut FrameWriter<W>,
) -> io::Result<bool>
where
    W: AsyncWrite + Unpin,
{
    // Validate the wire-format version string (length + semver shape).
    let client_version = match validators::validate_version(version) {
        Ok(v) => v,
        Err(_) => {
            let response = ServerMessage::HandshakeResponse {
                success: false,
                version: Some(TRACKER_PROTOCOL_VERSION.to_string()),
                fingerprint: fingerprint.to_string(),
                error: Some(err_tracker_handshake_version_invalid(locale)),
            };
            send_server_message(writer, &response).await?;
            return Ok(false);
        }
    };

    let server_version = version::tracker_protocol_version();
    let server_version_str = TRACKER_PROTOCOL_VERSION;

    match version::check_compatibility(&server_version, &client_version) {
        CompatibilityResult::Compatible => {
            let response = ServerMessage::HandshakeResponse {
                success: true,
                version: Some(server_version_str.to_string()),
                fingerprint: fingerprint.to_string(),
                error: None,
            };
            send_server_message(writer, &response).await?;
            Ok(true)
        }
        CompatibilityResult::MajorMismatch {
            server_major,
            client_major,
        } => {
            warn!(
                server_major = server_major,
                client_major = client_major,
                "{}",
                LOG_HANDSHAKE_MAJOR_MISMATCH
            );
            send_mismatch(writer, locale, fingerprint, server_version_str, version).await?;
            Ok(false)
        }
        CompatibilityResult::MinorMismatch {
            server_minor,
            client_minor,
        } => {
            warn!(
                server_minor = server_minor,
                client_minor = client_minor,
                "{}",
                LOG_HANDSHAKE_MINOR_MISMATCH
            );
            send_mismatch(writer, locale, fingerprint, server_version_str, version).await?;
            Ok(false)
        }
        CompatibilityResult::ClientTooNew {
            server_minor,
            client_minor,
        } => {
            warn!(
                server_minor = server_minor,
                client_minor = client_minor,
                "{}",
                LOG_HANDSHAKE_CLIENT_TOO_NEW
            );
            send_mismatch(writer, locale, fingerprint, server_version_str, version).await?;
            Ok(false)
        }
    }
}

/// Send a `HandshakeResponse` carrying the unified version-mismatch error.
async fn send_mismatch<W>(
    writer: &mut FrameWriter<W>,
    locale: &str,
    fingerprint: &str,
    server_version_str: &str,
    client_version_str: &str,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let response = ServerMessage::HandshakeResponse {
        success: false,
        version: Some(server_version_str.to_string()),
        fingerprint: fingerprint.to_string(),
        error: Some(err_tracker_protocol_version_mismatch(
            locale,
            server_version_str,
            client_version_str,
        )),
    };
    send_server_message(writer, &response).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_common::framing::{FrameReader, FrameWriter};
    use nexus_common::io::read_server_message;

    /// Write a Handshake response to an in-memory pipe and read it back as
    /// the caller would. Returns `(success, error)` for assertions plus
    /// the raw `HandshakeResponse` for any deeper checks.
    async fn run_handle_handshake(version: &str) -> (bool, Option<String>, ServerMessage) {
        let (client, server) = tokio::io::duplex(8192);
        let (server_read, server_write) = tokio::io::split(server);
        let (client_read, _client_write) = tokio::io::split(client);

        let server_handle = tokio::spawn({
            let version = version.to_string();
            async move {
                let mut writer = FrameWriter::new(server_write);
                let result =
                    handle_handshake(&version, "en", "FAKE-FINGERPRINT", &mut writer).await;
                drop(writer);
                drop(server_read);
                result
            }
        });

        let mut reader = FrameReader::new(client_read);
        let received = read_server_message(&mut reader)
            .await
            .expect("read")
            .expect("frame");

        let success_flag = server_handle.await.expect("join").expect("handler");
        let (success, error) = match &received.message {
            ServerMessage::HandshakeResponse { success, error, .. } => (*success, error.clone()),
            other => panic!("expected HandshakeResponse, got {other:?}"),
        };
        // Sanity: the boolean returned by the handler should match the
        // success field in the wire response.
        assert_eq!(success_flag, success, "handler return vs wire success");
        (success, error, received.message)
    }

    #[tokio::test]
    async fn test_handshake_compatible_version_succeeds() {
        let (success, error, msg) = run_handle_handshake(TRACKER_PROTOCOL_VERSION).await;
        assert!(success, "matching version should succeed");
        assert!(error.is_none(), "no error expected");
        if let ServerMessage::HandshakeResponse { fingerprint, .. } = msg {
            assert_eq!(fingerprint, "FAKE-FINGERPRINT");
        }
    }

    #[tokio::test]
    async fn test_handshake_invalid_version_string_fails() {
        let (success, error, _) = run_handle_handshake("not-a-semver").await;
        assert!(!success);
        let err = error.expect("error message expected");
        assert!(
            err.contains("Invalid handshake version"),
            "expected invalid-version message, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_handshake_empty_version_fails() {
        let (success, error, _) = run_handle_handshake("").await;
        assert!(!success);
        assert!(error.is_some());
    }

    #[tokio::test]
    async fn test_handshake_major_mismatch_fails() {
        // The tracker protocol is currently 0.1.0 (pre-1.0). A 1.x.y client
        // is a major mismatch.
        let (success, error, _) = run_handle_handshake("1.0.0").await;
        assert!(!success);
        let err = error.expect("error message expected");
        assert!(
            err.contains("Incompatible tracker protocol version"),
            "expected mismatch message, got: {err}"
        );
        // Both versions should appear in the message (per the args).
        assert!(err.contains("1.0.0"), "client version in message");
    }

    #[tokio::test]
    async fn test_handshake_minor_mismatch_fails_during_pre_1_0() {
        // Server is 0.1.0; 0.2.0 client is a minor mismatch (pre-1.0 rule).
        let (success, error, _) = run_handle_handshake("0.2.0").await;
        assert!(!success);
        assert!(error.is_some());
    }

    #[tokio::test]
    async fn test_handshake_response_carries_fingerprint_on_failure() {
        // Per the protocol, fingerprint is reported on every response —
        // success and failure — so clients can detect TLS interception.
        let (_, _, msg) = run_handle_handshake("not-a-semver").await;
        if let ServerMessage::HandshakeResponse { fingerprint, .. } = msg {
            assert_eq!(fingerprint, "FAKE-FINGERPRINT");
        } else {
            panic!("expected HandshakeResponse");
        }
    }
}
