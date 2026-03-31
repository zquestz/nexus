//! Handshake message handler

use std::io;

use tokio::io::AsyncWrite;

use nexus_common::protocol::ServerMessage;
use nexus_common::validators::{self, VersionError};
use nexus_common::version::{self, CompatibilityResult};

use super::{
    HandlerContext, err_handshake_already_completed, err_version_client_too_new, err_version_empty,
    err_version_invalid_semver, err_version_major_mismatch, err_version_minor_mismatch,
    err_version_too_long,
};

/// Handle a handshake request from the client
pub async fn handle_handshake<W>(
    version: String,
    handshake_complete: &mut bool,
    ctx: &mut HandlerContext<'_, W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let server_version_str = nexus_common::PROTOCOL_VERSION;

    // Check for duplicate handshake
    if *handshake_complete {
        eprintln!("Duplicate handshake attempt from {}", ctx.peer_addr);
        let response = ServerMessage::HandshakeResponse {
            success: false,
            version: Some(server_version_str.to_string()),
            error: Some(err_handshake_already_completed(ctx.locale)),
        };
        ctx.send_message(&response).await?;
        return Err(io::Error::other("Duplicate handshake"));
    }

    // Validate and parse version string
    let client_version = match validators::validate_version(&version) {
        Ok(v) => v,
        Err(e) => {
            let error_msg = match e {
                VersionError::Empty => err_version_empty(ctx.locale),
                VersionError::TooLong => {
                    err_version_too_long(ctx.locale, validators::MAX_VERSION_LENGTH)
                }
                VersionError::InvalidSemver => err_version_invalid_semver(ctx.locale),
            };
            let response = ServerMessage::HandshakeResponse {
                success: false,
                version: Some(server_version_str.to_string()),
                error: Some(error_msg),
            };
            ctx.send_message(&response).await?;
            return Err(io::Error::other("Invalid version string"));
        }
    };

    // Check semver compatibility using the already-parsed version
    match version::check_compatibility(&client_version) {
        CompatibilityResult::Compatible => {
            // Version is compatible - complete handshake
            *handshake_complete = true;
            let response = ServerMessage::HandshakeResponse {
                success: true,
                version: Some(server_version_str.to_string()),
                error: None,
            };
            ctx.send_message(&response).await
        }
        CompatibilityResult::MajorMismatch {
            server_major,
            client_major,
        } => {
            eprintln!(
                "Handshake from {} failed: major version mismatch (client: {}, server: {})",
                ctx.peer_addr, client_major, server_major
            );
            let response = ServerMessage::HandshakeResponse {
                success: false,
                version: Some(server_version_str.to_string()),
                error: Some(err_version_major_mismatch(
                    ctx.locale,
                    server_major,
                    client_major,
                )),
            };
            ctx.send_message(&response).await?;
            Err(io::Error::other("Major version mismatch"))
        }
        CompatibilityResult::MinorMismatch {
            server_minor,
            client_minor,
        } => {
            eprintln!(
                "Handshake from {} failed: minor version mismatch (client: {}, server: {})",
                ctx.peer_addr, client_minor, server_minor
            );
            let response = ServerMessage::HandshakeResponse {
                success: false,
                version: Some(server_version_str.to_string()),
                error: Some(err_version_minor_mismatch(
                    ctx.locale,
                    server_version_str,
                    &version,
                )),
            };
            ctx.send_message(&response).await?;
            Err(io::Error::other("Minor version mismatch (pre-1.0)"))
        }
        CompatibilityResult::ClientTooNew {
            server_minor,
            client_minor,
        } => {
            eprintln!(
                "Handshake from {} failed: client minor version {} is newer than server minor version {}",
                ctx.peer_addr, client_minor, server_minor
            );
            let response = ServerMessage::HandshakeResponse {
                success: false,
                version: Some(server_version_str.to_string()),
                error: Some(err_version_client_too_new(
                    ctx.locale,
                    server_version_str,
                    &version,
                )),
            };
            ctx.send_message(&response).await?;
            Err(io::Error::other("Client version too new"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::testing::{create_test_context, read_server_message};
    use nexus_common::version;

    #[tokio::test]
    async fn test_successful_handshake() {
        let mut test_ctx = create_test_context().await;
        let mut handshake_complete = false;

        // Call handler with matching version
        let version = nexus_common::PROTOCOL_VERSION.to_string();
        let result = handle_handshake(
            version,
            &mut handshake_complete,
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should succeed
        assert!(
            result.is_ok(),
            "Handshake should succeed with matching version"
        );
        assert!(handshake_complete, "Handshake flag should be set to true");

        // Read response from client side using new framing format
        let response_msg = read_server_message(&mut test_ctx).await;

        // Verify response
        match response_msg {
            ServerMessage::HandshakeResponse {
                success,
                version,
                error,
            } => {
                assert!(success, "Response should indicate success");
                assert_eq!(version, Some(nexus_common::PROTOCOL_VERSION.to_string()));
                assert!(error.is_none(), "Error should be None on success");
            }
            _ => panic!("Expected HandshakeResponse"),
        }
    }

    #[tokio::test]
    async fn test_pre_1_0_rejects_older_minor_version() {
        let mut test_ctx = create_test_context().await;
        let mut handshake_complete = false;

        // During pre-1.0, older minor versions are incompatible (breaking changes)
        let server_ver = version::protocol_version();
        // Precondition: we must be in pre-1.0 with minor > 0 for this test to be meaningful
        assert_eq!(server_ver.major, 0, "This test requires pre-1.0 protocol");
        assert!(
            server_ver.minor > 0,
            "This test requires minor > 0 to test older minor"
        );

        let client_version = format!("{}.{}.0", server_ver.major, server_ver.minor - 1);
        let result = handle_handshake(
            client_version.clone(),
            &mut handshake_complete,
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(
            result.is_err(),
            "Handshake should fail with older minor version during pre-1.0"
        );
        assert!(
            !handshake_complete,
            "Handshake flag should not be set on minor mismatch"
        );

        // Verify error is specifically about minor mismatch
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("Minor version mismatch"),
            "Error should be minor version mismatch, got: {}",
            err
        );

        // Verify response message
        let response_msg = read_server_message(&mut test_ctx).await;
        match response_msg {
            ServerMessage::HandshakeResponse {
                success,
                error,
                version: resp_version,
            } => {
                assert!(!success, "Response should indicate failure");
                assert_eq!(
                    resp_version,
                    Some(nexus_common::PROTOCOL_VERSION.to_string())
                );
                let error_msg = error.expect("Should have error message");
                assert!(
                    error_msg.contains("same minor version"),
                    "Error should mention same minor version requirement, got: {}",
                    error_msg
                );
                assert!(
                    error_msg.contains(&client_version),
                    "Error should include client version, got: {}",
                    error_msg
                );
            }
            _ => panic!("Expected HandshakeResponse"),
        }
    }

    #[tokio::test]
    async fn test_pre_1_0_rejects_newer_minor_version() {
        let mut test_ctx = create_test_context().await;
        let mut handshake_complete = false;

        // During pre-1.0, newer minor versions are also incompatible
        // (this is different from post-1.0 where client_minor <= server_minor is OK)
        let server_ver = version::protocol_version();
        assert_eq!(server_ver.major, 0, "This test requires pre-1.0 protocol");

        let client_version = format!("{}.{}.0", server_ver.major, server_ver.minor + 1);
        let result = handle_handshake(
            client_version.clone(),
            &mut handshake_complete,
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(
            result.is_err(),
            "Handshake should fail with newer minor version during pre-1.0"
        );
        assert!(
            !handshake_complete,
            "Handshake flag should not be set on minor mismatch"
        );

        // During pre-1.0, newer minor should be MinorMismatch (not ClientTooNew)
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("Minor version mismatch"),
            "Error should be minor version mismatch (not client-too-new), got: {}",
            err
        );

        // Verify response message
        let response_msg = read_server_message(&mut test_ctx).await;
        match response_msg {
            ServerMessage::HandshakeResponse {
                success,
                error,
                version: resp_version,
            } => {
                assert!(!success, "Response should indicate failure");
                assert_eq!(
                    resp_version,
                    Some(nexus_common::PROTOCOL_VERSION.to_string())
                );
                let error_msg = error.expect("Should have error message");
                assert!(
                    error_msg.contains("same minor version"),
                    "Error should mention same minor version requirement, got: {}",
                    error_msg
                );
                assert!(
                    error_msg.contains(&client_version),
                    "Error should include client version, got: {}",
                    error_msg
                );
            }
            _ => panic!("Expected HandshakeResponse"),
        }
    }

    #[tokio::test]
    async fn test_compatible_different_patch_version() {
        let mut test_ctx = create_test_context().await;
        let mut handshake_complete = false;

        // Parse the server version to create a version with different patch
        let server_ver = version::protocol_version();
        let client_version = format!("{}.{}.99", server_ver.major, server_ver.minor);
        let result = handle_handshake(
            client_version,
            &mut handshake_complete,
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(
            result.is_ok(),
            "Handshake should succeed with different patch version"
        );
        assert!(handshake_complete, "Handshake flag should be set to true");
    }

    #[tokio::test]
    async fn test_major_version_mismatch() {
        let mut test_ctx = create_test_context().await;
        let mut handshake_complete = false;

        // Use a different major version
        let server_ver = version::protocol_version();
        let client_version = format!("{}.0.0", server_ver.major + 1);
        let result = handle_handshake(
            client_version,
            &mut handshake_complete,
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(
            result.is_err(),
            "Handshake should fail with major version mismatch"
        );
        assert!(!handshake_complete, "Handshake flag should remain false");

        // Error should be about major version mismatch
        let err = result.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::Other);
        assert!(err.to_string().contains("Major version mismatch"));

        // Read and verify response using new framing format
        let response_msg = read_server_message(&mut test_ctx).await;

        match response_msg {
            ServerMessage::HandshakeResponse { success, error, .. } => {
                assert!(!success, "Response should indicate failure");
                assert!(error.is_some(), "Should have error message");
                let error_msg = error.unwrap();
                assert!(
                    error_msg.contains("Incompatible") || error_msg.contains("version"),
                    "Error should mention incompatible version"
                );
            }
            _ => panic!("Expected HandshakeResponse"),
        }
    }

    #[tokio::test]
    async fn test_client_minor_version_too_new() {
        let mut test_ctx = create_test_context().await;
        let mut handshake_complete = false;

        // Use a newer minor version
        let server_ver = version::protocol_version();
        let client_version = format!("{}.{}.0", server_ver.major, server_ver.minor + 1);
        let result = handle_handshake(
            client_version.clone(),
            &mut handshake_complete,
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(
            result.is_err(),
            "Handshake should fail when client minor version is too new"
        );
        assert!(!handshake_complete, "Handshake flag should remain false");

        // Read and verify response using new framing format
        let response_msg = read_server_message(&mut test_ctx).await;

        match response_msg {
            ServerMessage::HandshakeResponse { success, error, .. } => {
                assert!(!success, "Response should indicate failure");
                assert!(error.is_some(), "Should have error message");
                let error_msg = error.unwrap();
                assert!(
                    error_msg.contains("newer") || error_msg.contains(&client_version),
                    "Error should mention client is newer"
                );
            }
            _ => panic!("Expected HandshakeResponse"),
        }
    }

    #[tokio::test]
    async fn test_invalid_semver_format() {
        let mut test_ctx = create_test_context().await;
        let mut handshake_complete = false;

        // Use an invalid semver format
        let result = handle_handshake(
            "not-valid-semver".to_string(),
            &mut handshake_complete,
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(
            result.is_err(),
            "Handshake should fail with invalid semver format"
        );
        assert!(!handshake_complete, "Handshake flag should remain false");

        // Read and verify response using new framing format
        let response_msg = read_server_message(&mut test_ctx).await;

        match response_msg {
            ServerMessage::HandshakeResponse { success, error, .. } => {
                assert!(!success, "Response should indicate failure");
                assert!(error.is_some(), "Should have error message");
                let error_msg = error.unwrap();
                assert!(
                    error_msg.contains("semver") || error_msg.contains("MAJOR.MINOR.PATCH"),
                    "Error should mention semver format"
                );
            }
            _ => panic!("Expected HandshakeResponse"),
        }
    }

    #[tokio::test]
    async fn test_duplicate_handshake() {
        let mut test_ctx = create_test_context().await;
        let mut handshake_complete = false;

        let version = nexus_common::PROTOCOL_VERSION.to_string();

        // First handshake - should succeed
        let result1 = handle_handshake(
            version.clone(),
            &mut handshake_complete,
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result1.is_ok(), "First handshake should succeed");
        assert!(
            handshake_complete,
            "Flag should be set after first handshake"
        );

        // Second handshake - should fail (duplicate)
        let result2 = handle_handshake(
            version,
            &mut handshake_complete,
            &mut test_ctx.handler_context(),
        )
        .await;
        assert!(result2.is_err(), "Duplicate handshake should fail");
        assert!(handshake_complete, "Flag should remain true");

        // Error should be about duplicate
        let err = result2.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::Other);
        assert!(err.to_string().contains("Duplicate"));
    }

    #[tokio::test]
    async fn test_prerelease_version_compatible() {
        let mut test_ctx = create_test_context().await;
        let mut handshake_complete = false;

        // Pre-release versions should be compatible based on their base version
        let server_ver = version::protocol_version();
        let client_version = format!(
            "{}.{}.{}-alpha",
            server_ver.major, server_ver.minor, server_ver.patch
        );
        let result = handle_handshake(
            client_version,
            &mut handshake_complete,
            &mut test_ctx.handler_context(),
        )
        .await;

        assert!(
            result.is_ok(),
            "Handshake should succeed with pre-release version of same base"
        );
        assert!(handshake_complete, "Handshake flag should be set to true");
    }
}
