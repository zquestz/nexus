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
    if *handshake_complete {
        eprintln!("Duplicate handshake attempt from {}", ctx.peer_addr);
        let response = ServerMessage::HandshakeResponse {
            success: false,
            version: nexus_common::PROTOCOL_VERSION.to_string(),
            error: Some("Handshake already completed".to_string()),
        };
        ctx.send_message(&response).await?;
        return Err(io::Error::new(io::ErrorKind::Other, "Duplicate handshake"));
    }

    // Check if version is compatible
    let server_version = nexus_common::PROTOCOL_VERSION;
    if version == server_version {
        *handshake_complete = true;
        let response = ServerMessage::HandshakeResponse {
            success: true,
            version: server_version.to_string(),
            error: None,
        };
        ctx.send_message(&response).await?;
    } else {
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
        return Err(io::Error::new(io::ErrorKind::Other, "Version mismatch"));
    }

    Ok(())
}
