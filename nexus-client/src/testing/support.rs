//! Shared helpers for constructing connections in handler tests.

use std::sync::Arc;

use nexus_common::framing::MessageId;
use nexus_common::protocol::ClientMessage;
use nexus_common::validators::PasswordStrength;
use tokio::sync::{Mutex, mpsc};

use crate::types::{ConnectionInfo, ServerConnection, ServerConnectionParams};

/// Build a minimal `ServerConnection` wired to a message receiver so handler
/// tests can drive a submit path and assert on the `ClientMessage` it sends.
pub(crate) fn test_connection_with_receiver(
    connection_id: usize,
) -> (
    ServerConnection,
    mpsc::UnboundedReceiver<(MessageId, ClientMessage)>,
) {
    let (tx, rx) = mpsc::unbounded_channel();
    let conn = ServerConnection::new(ServerConnectionParams {
        bookmark_id: None,
        user_id: None,
        nickname: "me".to_string(),
        connection_info: ConnectionInfo {
            server_name: String::new(),
            address: String::new(),
            port: 0,
            transfer_port: 0,
            certificate_fingerprint: String::new(),
            username: "me".to_string(),
            password: String::new(),
            nickname: "me".to_string(),
        },
        display_name: String::new(),
        connection_id,
        is_admin: true,
        permissions: Vec::new(),
        features: Vec::new(),
        server_name: None,
        server_description: None,
        public_address: None,
        server_version: None,
        server_image: String::new(),
        cached_server_image: None,
        chat_burst_limit: None,
        chat_rate_limit: None,
        max_connections_per_ip: None,
        max_outbound_rate: None,
        max_transfers_per_ip: None,
        file_reindex_interval: None,
        persistent_channels: None,
        auto_join_channels: None,
        min_password_strength: PasswordStrength::Weak,
        log_level: None,
        scheduler_chunk_size: None,
        tx,
        shutdown_handle: Arc::new(Mutex::new(None)),
    });
    (conn, rx)
}
