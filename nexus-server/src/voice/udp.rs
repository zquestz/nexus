//! UDP/DTLS voice server for real-time audio communication
//!
//! This module handles the UDP side of voice chat, receiving voice packets
//! from clients over DTLS and relaying them to other participants.
//!
//! ## Architecture
//!
//! - DTLS listener on port 7500 (same as TCP, OS routes by protocol)
//! - Uses the same certificate as TCP/TLS
//! - Voice packets authenticated by token from VoiceJoinResponse
//! - Server decrypts, validates token, adds sender info, re-encrypts, relays
//!
//! ## Packet Flow
//!
//! 1. Client joins voice via TCP (VoiceJoin) and receives token
//! 2. Client establishes DTLS connection to server UDP port
//! 3. Client sends VoicePacket with token for authentication
//! 4. Server validates token, looks up session in VoiceRegistry
//! 5. Server relays as RelayedVoicePacket to other participants

use std::collections::HashMap;
use std::fs;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, RwLock as StdRwLock};
use std::time::{Duration, Instant};

use tracing::{debug, error, warn};

use dtls::config::Config as DtlsConfig;
use dtls::crypto::Certificate;
use dtls::listener::listen;
use tokio::sync::RwLock;

use webrtc_util::conn::{Conn, Listener};

use nexus_common::voice::{
    MAX_VOICE_PACKET_SIZE, RelayedVoicePacket, VOICE_SESSION_TIMEOUT_SECS, VoiceMessageType,
    VoicePacket,
};

/// Interval between stale client cleanup checks (seconds)
const STALE_CLIENT_CHECK_INTERVAL_SECS: u64 = 30;

use crate::channels::ChannelManager;
use crate::constants::*;
use crate::db::Permission;
use crate::ip_rule_cache::IpRuleCache;
use crate::users::UserManager;

use super::{VoiceRegistry, send_voice_leave_notifications};

/// DTLS connection state for a voice client
struct DtlsClient {
    /// The DTLS connection
    conn: Arc<dyn Conn + Send + Sync>,
    /// Client's remote address
    addr: SocketAddr,
    /// Last packet received time (for timeout)
    last_packet: Instant,
}

/// Manages UDP/DTLS voice connections
pub struct VoiceUdpServer {
    /// DTLS listener for voice connections
    listener: Arc<dyn Listener + Send + Sync>,
    /// Voice registry for session lookups
    registry: VoiceRegistry,
    /// Active DTLS clients, keyed by remote address
    clients: Arc<RwLock<HashMap<SocketAddr, Arc<RwLock<DtlsClient>>>>>,
    /// IP rule cache for ban checking
    ip_rule_cache: Arc<StdRwLock<IpRuleCache>>,
    /// User manager for permission checks
    user_manager: UserManager,
    /// Channel manager for voice leave broadcasts
    channel_manager: ChannelManager,
}

impl VoiceUdpServer {
    /// Create a new voice UDP server with a pre-created DTLS listener
    ///
    /// # Arguments
    ///
    /// * `listener` - Pre-created DTLS listener
    /// * `registry` - Voice registry for session lookups
    /// * `ip_rule_cache` - IP rule cache for ban checking
    /// * `user_manager` - User manager for permission checks
    pub fn new(
        listener: Arc<dyn Listener + Send + Sync>,
        registry: VoiceRegistry,
        ip_rule_cache: Arc<StdRwLock<IpRuleCache>>,
        user_manager: UserManager,
        channel_manager: ChannelManager,
    ) -> Self {
        Self {
            listener,
            registry,
            clients: Arc::new(RwLock::new(HashMap::new())),
            ip_rule_cache,
            user_manager,
            channel_manager,
        }
    }

    /// Run the voice UDP server
    ///
    /// This method runs forever, accepting DTLS connections and processing voice packets.
    /// It should be spawned as a separate tokio task.
    pub async fn run(self: Arc<Self>) {
        // Spawn cleanup task
        let cleanup_self = self.clone();
        tokio::spawn(async move {
            cleanup_self.cleanup_loop().await;
        });

        // Accept loop
        loop {
            match self.listener.accept().await {
                Ok((conn, remote_addr)) => {
                    // Check IP ban before processing (trust bypasses ban)
                    let should_allow = {
                        let cache = self.ip_rule_cache.read().expect(ERR_IP_CACHE_POISONED);
                        if cache.needs_rebuild() {
                            drop(cache);
                            self.ip_rule_cache
                                .write()
                                .expect(ERR_IP_CACHE_POISONED)
                                .should_allow(remote_addr.ip())
                        } else {
                            cache.should_allow_read_only(remote_addr.ip())
                        }
                    };

                    if !should_allow {
                        debug!(ip = %remote_addr.ip(), "{}", LOG_VOICE_REJECTED_BANNED);
                        let _ = conn.close().await;
                        continue;
                    }

                    // Reject IPs that don't have an active voice session
                    if !self.registry.has_session_for_ip(remote_addr.ip()).await {
                        warn!(ip = %remote_addr.ip(), "{}", LOG_VOICE_REJECTED_NO_SESSION);
                        let _ = conn.close().await;
                        continue;
                    }

                    debug!(ip = %remote_addr, "{}", LOG_VOICE_NEW_CONNECTION);

                    // Create client state
                    let client = Arc::new(RwLock::new(DtlsClient {
                        conn: conn.clone(),
                        addr: remote_addr,
                        last_packet: Instant::now(),
                    }));

                    // Store client
                    {
                        let mut clients = self.clients.write().await;
                        clients.insert(remote_addr, client.clone());
                    }

                    // Spawn handler for this connection
                    let server = self.clone();
                    tokio::spawn(async move {
                        server.handle_connection(client, remote_addr).await;
                    });
                }
                Err(e) => {
                    warn!(err = %e, "{}", LOG_VOICE_ACCEPT_ERROR);
                }
            }
        }
    }

    /// Handle a single DTLS connection
    async fn handle_connection(&self, client: Arc<RwLock<DtlsClient>>, remote_addr: SocketAddr) {
        let mut buf = vec![0u8; MAX_VOICE_PACKET_SIZE + 100]; // Extra for DTLS overhead

        loop {
            // Get connection reference
            let conn = {
                let client_guard = client.read().await;
                client_guard.conn.clone()
            };

            // Read with timeout
            let read_result = tokio::time::timeout(
                Duration::from_secs(VOICE_SESSION_TIMEOUT_SECS),
                conn.recv(&mut buf),
            )
            .await;

            match read_result {
                Ok(Ok(len)) if len > 0 => {
                    let packet_data = buf[..len].to_vec();
                    if !self.handle_packet(&client, &packet_data).await {
                        // Session no longer exists, close connection
                        break;
                    }
                }
                Ok(Ok(_)) => {
                    // Zero-length read, connection closed
                    debug!(ip = %remote_addr, "{}", LOG_VOICE_CONNECTION_CLOSED);
                    break;
                }
                Ok(Err(e)) => {
                    warn!(ip = %remote_addr, err = %e, "{}", LOG_VOICE_READ_ERROR);
                    break;
                }
                Err(_) => {
                    // Timeout
                    debug!(ip = %remote_addr, "{}", LOG_VOICE_CONNECTION_TIMEOUT);
                    break;
                }
            }
        }

        // Remove client on disconnect
        {
            let mut clients = self.clients.write().await;
            clients.remove(&remote_addr);
        }

        // Close the connection
        let conn = {
            let client_guard = client.read().await;
            client_guard.conn.clone()
        };
        let _ = conn.close().await;
    }

    /// Handle an incoming voice packet
    ///
    /// Returns `false` if the connection should be closed (session no longer exists).
    async fn handle_packet(&self, client: &Arc<RwLock<DtlsClient>>, data: &[u8]) -> bool {
        // Parse the voice packet
        let Some(packet) = VoicePacket::from_bytes(data) else {
            let addr = client.read().await.addr;
            warn!(ip = %addr, "{}", LOG_VOICE_INVALID_PACKET);
            return true; // Invalid packet, but keep connection
        };

        // Update last packet time
        {
            let mut client_guard = client.write().await;
            client_guard.last_packet = Instant::now();
        }

        // Always validate against registry - session may have been removed via VoiceLeave
        let Some(session) = self.registry.get_by_token(packet.token).await else {
            let addr = client.read().await.addr;
            debug!(ip = %addr, "{}", LOG_VOICE_SESSION_NOT_FOUND);
            return false; // Session gone, close connection
        };

        let sender_nickname = session.nickname.clone();
        let target_key = session.target_key();
        let session_id = session.session_id;

        // Update UDP address in registry if not set
        if session.udp_addr.is_none() {
            let addr = client.read().await.addr;
            self.registry.set_udp_addr(packet.token, addr).await;
        }

        // Update idle tracking on speaking transitions (not every 10ms VoiceData packet).
        // Client-side auto-away skips connections in voice entirely, so this is only
        // for server-side idle accuracy — no need for high-frequency updates.
        if matches!(
            packet.msg_type,
            VoiceMessageType::SpeakingStarted | VoiceMessageType::SpeakingStopped
        ) {
            self.user_manager.update_last_activity(session_id).await;
        }

        // Handle based on message type
        match packet.msg_type {
            VoiceMessageType::Keepalive => {
                // Keepalive just updates last_packet time, already done above
                let addr = client.read().await.addr;
                debug!(user = %sender_nickname, ip = %addr, "{}", LOG_VOICE_KEEPALIVE);
            }
            VoiceMessageType::VoiceData
            | VoiceMessageType::SpeakingStarted
            | VoiceMessageType::SpeakingStopped => {
                // Check voice_talk permission before relaying
                match self
                    .user_manager
                    .has_permission(session_id, Permission::VoiceTalk)
                    .await
                {
                    Some(true) => {
                        // User has permission, relay the packet
                        self.relay_packet(&packet, &sender_nickname, &target_key)
                            .await;
                    }
                    Some(false) => {
                        // User lacks permission, drop packet silently
                        warn!(user = %sender_nickname, "{}", LOG_VOICE_NO_PERMISSION);
                    }
                    None => {
                        // User disconnected, drop packet
                    }
                }
            }
        }

        true // Keep connection alive
    }

    /// Relay a voice packet to other participants in the same voice session
    async fn relay_packet(&self, packet: &VoicePacket, sender_nickname: &str, target_key: &str) {
        // Get all sessions for this target
        let sessions = self.registry.get_sessions_for_target(target_key).await;

        // Create relayed packet
        let relayed = RelayedVoicePacket::from_voice_packet(packet, sender_nickname.to_string());
        let relayed_bytes = relayed.to_bytes();

        // Get client map for connection lookup
        let clients = self.clients.read().await;

        for session in sessions {
            // Don't send back to sender
            if session.nickname == sender_nickname {
                continue;
            }

            // Find the DTLS connection for this session
            if let Some(udp_addr) = session.udp_addr
                && let Some(client) = clients.get(&udp_addr)
            {
                let conn = {
                    let client_guard = client.read().await;
                    client_guard.conn.clone()
                };

                if let Err(e) = conn.send(&relayed_bytes).await {
                    error!(user = %session.nickname, ip = %udp_addr, err = %e, "{}", LOG_VOICE_RELAY_FAILED);
                }
            }
        }
    }

    /// Cleanup loop for removing stale client entries
    async fn cleanup_loop(&self) {
        let check_interval = Duration::from_secs(STALE_CLIENT_CHECK_INTERVAL_SECS);
        let timeout = Duration::from_secs(VOICE_SESSION_TIMEOUT_SECS);

        loop {
            tokio::time::sleep(check_interval).await;

            let now = Instant::now();
            let mut clients = self.clients.write().await;

            // Collect addresses of timed-out clients
            let mut timed_out_addrs = Vec::new();
            for (addr, client) in clients.iter() {
                let client_guard = client.read().await;
                if now.duration_since(client_guard.last_packet) > timeout {
                    timed_out_addrs.push(*addr);
                }
            }

            // Remove timed-out clients
            for addr in timed_out_addrs {
                if let Some(client) = clients.remove(&addr) {
                    let client_guard = client.read().await;
                    debug!(ip = %addr, "{}", LOG_VOICE_CLEANUP_TIMEOUT);
                    // Close connection
                    let _ = client_guard.conn.close().await;
                }
            }

            // Clean up sessions that never established a UDP connection
            // (e.g., client sent VoiceJoin but DTLS handshake failed due to firewall)
            let stale_tokens = self
                .registry
                .find_stale_sessions(VOICE_SESSION_TIMEOUT_SECS)
                .await;

            for token in stale_tokens {
                if let Some(info) = self.registry.remove_by_token(token).await {
                    // Get the leaving user's tx if still connected
                    let leaving_user_tx = self
                        .user_manager
                        .get_user_by_session_id(info.session.session_id)
                        .await
                        .map(|u| u.tx.clone());

                    // Send notifications using the consolidated helper
                    send_voice_leave_notifications(
                        &info,
                        leaving_user_tx.as_ref(),
                        &self.user_manager,
                        &self.channel_manager,
                    )
                    .await;

                    debug!(user = %info.session.nickname, "{}", LOG_VOICE_STALE_SESSION);
                }
            }
        }
    }
}

/// Create a DTLS listener for voice traffic
///
/// # Arguments
///
/// * `addr` - Socket address to bind to (typically same IP as TCP, port 7500)
/// * `cert_path` - Path to the certificate PEM file
/// * `key_path` - Path to the private key PEM file
///
/// # Returns
///
/// The DTLS listener, or an error message
pub async fn create_voice_listener(
    addr: SocketAddr,
    cert_path: &Path,
    key_path: &Path,
) -> Result<Arc<dyn Listener + Send + Sync>, String> {
    let config = load_dtls_config(cert_path, key_path)?;

    let listener = listen(addr, config)
        .await
        .map_err(|e| format!("Failed to create voice DTLS listener on {}: {}", addr, e))?;

    Ok(Arc::new(listener))
}

/// Load DTLS configuration from certificate and key files
///
/// Uses the same certificate as the TCP/TLS server.
fn load_dtls_config(cert_path: &Path, key_path: &Path) -> Result<DtlsConfig, String> {
    // Read certificate and key PEM files
    let cert_pem = fs::read_to_string(cert_path)
        .map_err(|e| format!("Failed to read certificate file: {}", e))?;

    let key_pem = fs::read_to_string(key_path)
        .map_err(|e| format!("Failed to read private key file: {}", e))?;

    // WORKAROUND: The dtls crate (0.13.0) has a bug in its PEM parser - it expects the tag
    // "PRIVATE_KEY" (with underscore) but standard PKCS#8 PEM files use "PRIVATE KEY" (with space).
    // See: https://github.com/webrtc-rs/webrtc/tree/master/dtls
    // Convert the tag to work around this bug.
    let key_pem = key_pem
        .replace("-----BEGIN PRIVATE KEY-----", "-----BEGIN PRIVATE_KEY-----")
        .replace("-----END PRIVATE KEY-----", "-----END PRIVATE_KEY-----");

    // Combine into single PEM string (Certificate::from_pem expects key first, then cert)
    let combined_pem = format!("{}\n{}", key_pem, cert_pem);

    // Parse certificate with private key
    let certificate = Certificate::from_pem(&combined_pem)
        .map_err(|e| format!("Failed to parse certificate: {}", e))?;

    // Create DTLS config
    let config = DtlsConfig {
        certificates: vec![certificate],
        insecure_skip_verify: true, // Clients use TOFU model like TCP
        ..Default::default()
    };

    Ok(config)
}

#[cfg(test)]
mod tests {
    // Note: Full integration tests require actual DTLS listener setup
    // which needs certificate files. Unit tests for packet handling
    // are in nexus-common/src/voice.rs
}
