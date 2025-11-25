//! Handshake message handler

use super::HandlerContext;
use nexus_common::protocol::ServerMessage;
use std::io;

/// Handle a handshake request from the client
pub async fn handle_handshake(
    version: String,
    handshake_complete: &mut bool,
    ctx: &mut HandlerContext<'_>,
) -> io::Result<()> {
    // Check for duplicate handshake
    if *handshake_complete {
        eprintln!("Duplicate handshake attempt from {}", ctx.peer_addr);
        let response = ServerMessage::HandshakeResponse {
            success: false,
            version: nexus_common::PROTOCOL_VERSION.to_string(),
            error: Some("Handshake already completed".to_string()),
        };
        ctx.send_message(&response).await?;
        return Err(io::Error::other("Duplicate handshake"));
    }

    // Verify protocol version compatibility
    let server_version = nexus_common::PROTOCOL_VERSION;

    if version == server_version {
        // Version matches - complete handshake
        *handshake_complete = true;
        let response = ServerMessage::HandshakeResponse {
            success: true,
            version: server_version.to_string(),
            error: None,
        };
        ctx.send_message(&response).await?;
    } else {
        // Version mismatch - reject handshake
        let response = ServerMessage::HandshakeResponse {
            success: false,
            version: server_version.to_string(),
            error: Some(format!(
                "Version mismatch: server uses {}, client uses {}",
                server_version, version
            )),
        };
        ctx.send_message(&response).await?;
        eprintln!("Handshake failed with {}: version mismatch", ctx.peer_addr);
        return Err(io::Error::other("Version mismatch"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::testing::create_test_context;
    use tokio::io::AsyncReadExt;

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

        // Close writer so client can read complete response
        drop(test_ctx.write_half);

        // Read response from client side
        let mut response = String::new();
        test_ctx.client.read_to_string(&mut response).await.unwrap();

        // Parse JSON response
        let response_msg: ServerMessage = serde_json::from_str(response.trim()).unwrap();

        // Verify response
        match response_msg {
            ServerMessage::HandshakeResponse {
                success,
                version,
                error,
            } => {
                assert!(success, "Response should indicate success");
                assert_eq!(version, nexus_common::PROTOCOL_VERSION);
                assert!(error.is_none(), "Error should be None on success");
            }
            _ => panic!("Expected HandshakeResponse"),
        }
    }

    #[tokio::test]
    async fn test_version_mismatch() {
        let mut test_ctx = create_test_context().await;
        let mut handshake_complete = false;

        // Call handler with mismatched version
        let client_version = "0.9.9";
        let result = handle_handshake(
            client_version.to_string(),
            &mut handshake_complete,
            &mut test_ctx.handler_context(),
        )
        .await;

        // Should fail
        assert!(
            result.is_err(),
            "Handshake should fail with version mismatch"
        );
        assert!(!handshake_complete, "Handshake flag should remain false");

        // Error should be about version mismatch
        let err = result.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::Other);

        // Close writer so client can read response
        drop(test_ctx.write_half);

        // Read and verify JSON response
        let mut response = String::new();
        test_ctx.client.read_to_string(&mut response).await.unwrap();

        let response_msg: ServerMessage = serde_json::from_str(response.trim()).unwrap();

        // Verify error message format
        match response_msg {
            ServerMessage::HandshakeResponse {
                success,
                version,
                error,
            } => {
                assert!(!success, "Response should indicate failure");
                assert_eq!(version, nexus_common::PROTOCOL_VERSION);
                assert!(error.is_some(), "Should have error message");

                let error_msg = error.unwrap();
                assert!(error_msg.contains("Version mismatch"));
                assert!(error_msg.contains(client_version));
                assert!(error_msg.contains(nexus_common::PROTOCOL_VERSION));
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
}
