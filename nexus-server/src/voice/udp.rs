//! UDP/DTLS voice server for real-time audio communication.
//!
//! DTLS listener on port 7500 (same as TCP, OS routes by protocol), reusing
//! the TCP/TLS certificate. Per packet: validate the token against
//! `VoiceRegistry`, look up the session, then re-encrypt and relay to the other
//! participants as a `RelayedVoicePacket`. Tokens come from `VoiceJoinResponse`.

use std::collections::HashMap;
use std::fs;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tracing::{debug, error, warn};

use dtls::config::Config as DtlsConfig;
use dtls::crypto::Certificate;
use dtls::listener::listen;
use tokio::sync::RwLock;

use webrtc_util::conn::{Conn, Listener};

use nexus_common::address::normalize_socket_addr;
use nexus_common::names::fold_name;
use nexus_common::voice::{
    MAX_VOICE_PACKET_SIZE, RelayedVoicePacket, VOICE_SESSION_TIMEOUT_SECS, VoiceMessageType,
    VoicePacket,
};

const STALE_CLIENT_CHECK_INTERVAL_SECS: u64 = 30;

use crate::channels::ChannelManager;
use crate::connection_tracker::ConnectionTracker;
use crate::constants::*;
use crate::db::Permission;
use crate::ip_rule_cache::IpRuleState;
use crate::users::UserManager;

use super::{VoiceRegistry, send_voice_leave_notifications};

struct DtlsClient {
    conn: Arc<dyn Conn + Send + Sync>,
    addr: SocketAddr,
    /// Last packet received; drives the idle timeout.
    last_packet: Instant,
}

pub struct VoiceUdpServer {
    listener: Arc<dyn Listener + Send + Sync>,
    registry: VoiceRegistry,
    /// Active DTLS clients, keyed by remote address.
    clients: Arc<RwLock<HashMap<SocketAddr, Arc<RwLock<DtlsClient>>>>>,
    ip_rule_cache: Arc<IpRuleState>,
    user_manager: UserManager,
    channel_manager: ChannelManager,
    connection_tracker: Arc<ConnectionTracker>,
}

impl VoiceUdpServer {
    pub fn new(
        listener: Arc<dyn Listener + Send + Sync>,
        registry: VoiceRegistry,
        ip_rule_cache: Arc<IpRuleState>,
        user_manager: UserManager,
        channel_manager: ChannelManager,
        connection_tracker: Arc<ConnectionTracker>,
    ) -> Self {
        Self {
            listener,
            registry,
            clients: Arc::new(RwLock::new(HashMap::new())),
            ip_rule_cache,
            user_manager,
            channel_manager,
            connection_tracker,
        }
    }

    /// Runs forever; spawn as a separate tokio task.
    pub async fn run(self: Arc<Self>) {
        let cleanup_self = self.clone();
        tokio::spawn(async move {
            cleanup_self.cleanup_loop().await;
        });

        loop {
            match self.listener.accept().await {
                Ok((conn, remote_addr)) => {
                    // Fold IPv4-mapped IPv6 to IPv4 so lookup keys match the
                    // form the TCP path registered (it normalizes at accept too).
                    let remote_addr = normalize_socket_addr(remote_addr);
                    // Ban check before processing; trust bypasses ban.
                    let should_allow = self.ip_rule_cache.should_allow(remote_addr.ip()).await;

                    if !should_allow {
                        debug!(ip = %remote_addr.ip(), "{}", LOG_VOICE_REJECTED_BANNED);
                        let _ = conn.close().await;
                        continue;
                    }

                    // Per-IP voice connection cap (shares the BBS limit value,
                    // counted separately). Authorization is the per-packet
                    // token check; this just bounds concurrent connections.
                    let Some(voice_guard) =
                        self.connection_tracker.try_acquire_voice(remote_addr.ip())
                    else {
                        debug!(ip = %remote_addr.ip(), "{}", LOG_VOICE_REJECTED_LIMIT);
                        let _ = conn.close().await;
                        continue;
                    };

                    debug!(ip = %remote_addr, "{}", LOG_VOICE_NEW_CONNECTION);

                    let client = Arc::new(RwLock::new(DtlsClient {
                        conn: conn.clone(),
                        addr: remote_addr,
                        last_packet: Instant::now(),
                    }));

                    {
                        let mut clients = self.clients.write().await;
                        clients.insert(remote_addr, client.clone());
                    }

                    let server = self.clone();
                    tokio::spawn(async move {
                        // Held for the connection's lifetime; releases the slot on drop.
                        let _voice_guard = voice_guard;
                        server.handle_connection(client, remote_addr).await;
                    });
                }
                Err(e) => {
                    warn!(err = %e, "{}", LOG_VOICE_ACCEPT_ERROR);
                }
            }
        }
    }

    async fn handle_connection(&self, client: Arc<RwLock<DtlsClient>>, remote_addr: SocketAddr) {
        let mut buf = vec![0u8; MAX_VOICE_PACKET_SIZE + 100]; // Extra for DTLS overhead

        loop {
            let conn = {
                let client_guard = client.read().await;
                client_guard.conn.clone()
            };

            let read_result = tokio::time::timeout(
                Duration::from_secs(VOICE_SESSION_TIMEOUT_SECS),
                conn.recv(&mut buf),
            )
            .await;

            match read_result {
                Ok(Ok(len)) if len > 0 => {
                    let packet_data = buf[..len].to_vec();
                    if !self.handle_packet(&client, &packet_data).await {
                        break; // Session gone
                    }
                }
                Ok(Ok(_)) => {
                    // Zero-length read = connection closed
                    debug!(ip = %remote_addr, "{}", LOG_VOICE_CONNECTION_CLOSED);
                    break;
                }
                Ok(Err(e)) => {
                    warn!(ip = %remote_addr, err = %e, "{}", LOG_VOICE_READ_ERROR);
                    break;
                }
                Err(_) => {
                    debug!(ip = %remote_addr, "{}", LOG_VOICE_CONNECTION_TIMEOUT);
                    break;
                }
            }
        }

        {
            let mut clients = self.clients.write().await;
            clients.remove(&remote_addr);
        }

        let conn = {
            let client_guard = client.read().await;
            client_guard.conn.clone()
        };
        let _ = conn.close().await;
    }

    /// Returns `false` if the connection should be closed (session gone).
    async fn handle_packet(&self, client: &Arc<RwLock<DtlsClient>>, data: &[u8]) -> bool {
        let Some(packet) = VoicePacket::from_bytes(data) else {
            let addr = client.read().await.addr;
            warn!(ip = %addr, "{}", LOG_VOICE_INVALID_PACKET);
            return true; // Invalid packet, but keep connection
        };

        {
            let mut client_guard = client.write().await;
            client_guard.last_packet = Instant::now();
        }

        // Validate the token on every packet — the session may have been
        // removed via VoiceLeave since the connection opened.
        let Some(session) = self.registry.get_by_token(packet.token).await else {
            let addr = client.read().await.addr;
            debug!(ip = %addr, "{}", LOG_VOICE_SESSION_NOT_FOUND);
            return false; // Session gone, close connection
        };

        let sender_nickname = session.nickname.clone();
        let target_key = session.target_key();
        let session_id = session.session_id;

        if session.udp_addr.is_none() {
            let addr = client.read().await.addr;
            self.registry.set_udp_addr(packet.token, addr).await;
        }

        // Update idle tracking only on speaking transitions, not every 10ms
        // VoiceData packet — this just keeps server-side idle accuracy.
        if matches!(
            packet.msg_type,
            VoiceMessageType::SpeakingStarted | VoiceMessageType::SpeakingStopped
        ) {
            self.user_manager.update_last_activity(session_id).await;
        }

        match packet.msg_type {
            VoiceMessageType::Keepalive => {
                // last_packet already bumped above; nothing else to do
                let addr = client.read().await.addr;
                debug!(user = %sender_nickname, ip = %addr, "{}", LOG_VOICE_KEEPALIVE);
            }
            VoiceMessageType::VoiceData
            | VoiceMessageType::SpeakingStarted
            | VoiceMessageType::SpeakingStopped => {
                // Gate relaying on voice_talk permission.
                match self
                    .user_manager
                    .has_permission(session_id, Permission::VoiceTalk)
                    .await
                {
                    Some(true) => {
                        self.relay_packet(&packet, &sender_nickname, &target_key)
                            .await;
                    }
                    Some(false) => {
                        warn!(user = %sender_nickname, "{}", LOG_VOICE_NO_PERMISSION);
                    }
                    None => {
                        // User disconnected; drop packet
                    }
                }
            }
        }

        true // Keep connection alive
    }

    /// Relay a voice packet to the other participants in the same session.
    async fn relay_packet(&self, packet: &VoicePacket, sender_nickname: &str, target_key: &str) {
        let sessions = self.registry.get_sessions_for_target(target_key).await;

        let relayed = RelayedVoicePacket::from_voice_packet(packet, sender_nickname.to_string());
        let relayed_bytes = relayed.to_bytes();

        let clients = self.clients.read().await;

        for session in sessions {
            // Never echo back to the sender
            if fold_name(&session.nickname) == fold_name(sender_nickname) {
                continue;
            }

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

    async fn cleanup_loop(&self) {
        let check_interval = Duration::from_secs(STALE_CLIENT_CHECK_INTERVAL_SECS);
        let timeout = Duration::from_secs(VOICE_SESSION_TIMEOUT_SECS);

        loop {
            tokio::time::sleep(check_interval).await;

            let now = Instant::now();
            let mut clients = self.clients.write().await;

            let mut timed_out_addrs = Vec::new();
            for (addr, client) in clients.iter() {
                let client_guard = client.read().await;
                if now.duration_since(client_guard.last_packet) > timeout {
                    timed_out_addrs.push(*addr);
                }
            }

            for addr in timed_out_addrs {
                if let Some(client) = clients.remove(&addr) {
                    let client_guard = client.read().await;
                    debug!(ip = %addr, "{}", LOG_VOICE_CLEANUP_TIMEOUT);
                    let _ = client_guard.conn.close().await;
                }
            }
            // Release the UDP client map before the stale-token reap, which only touches
            // the registry — and so it isn't held across read_user_state below.
            drop(clients);

            // Reap sessions that joined via TCP but never established UDP
            // (e.g. DTLS handshake blocked by a firewall).
            let stale_tokens = self
                .registry
                .find_stale_sessions(VOICE_SESSION_TIMEOUT_SECS)
                .await;

            if !stale_tokens.is_empty() {
                // read_user_state across the reap so each VoiceUserLeft orders
                // consistently with a concurrent rename (same reasoning as VoiceLeave):
                // the rename re-keys the registry entry before we remove it, or its
                // ChatUserRenamed is ordered after our VoiceUserLeft.
                let _user_state = self.user_manager.read_user_state().await;
                for token in stale_tokens {
                    if let Some(info) = self.registry.remove_by_token(token).await {
                        let leaving_user_tx = self
                            .user_manager
                            .get_user_by_session_id(info.session.session_id)
                            .await
                            .map(|u| u.tx.clone());

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
}

/// Create a DTLS listener for voice traffic, bound to `addr` (same IP as TCP).
pub async fn create_voice_listener(
    addr: SocketAddr,
    cert_path: &Path,
    key_path: &Path,
) -> Result<Arc<dyn Listener + Send + Sync>, String> {
    let config = load_dtls_config(cert_path, key_path)?;

    let listener = listen(addr, config)
        .await
        .map_err(|e| format!("{}{}: {}", ERR_VOICE_DTLS_LISTENER_PREFIX, addr, e))?;

    Ok(Arc::new(listener))
}

/// Load DTLS config; uses the same certificate as the TCP/TLS server.
fn load_dtls_config(cert_path: &Path, key_path: &Path) -> Result<DtlsConfig, String> {
    let cert_pem =
        fs::read_to_string(cert_path).map_err(|e| format!("{}{}", ERR_VOICE_READ_CERT_FILE, e))?;

    let key_pem =
        fs::read_to_string(key_path).map_err(|e| format!("{}{}", ERR_VOICE_READ_KEY_FILE, e))?;

    // WORKAROUND (dtls 0.13.0): its PEM parser expects the tag "PRIVATE_KEY"
    // (underscore), but PKCS#8 emits "PRIVATE KEY" (space). Rewrite the tag.
    let key_pem = key_pem
        .replace("-----BEGIN PRIVATE KEY-----", "-----BEGIN PRIVATE_KEY-----")
        .replace("-----END PRIVATE KEY-----", "-----END PRIVATE_KEY-----");

    // Certificate::from_pem expects key first, then cert.
    let combined_pem = format!("{}\n{}", key_pem, cert_pem);

    let certificate = Certificate::from_pem(&combined_pem)
        .map_err(|e| format!("{}{}", ERR_VOICE_PARSE_CERT, e))?;

    let config = DtlsConfig {
        certificates: vec![certificate],
        insecure_skip_verify: true, // Clients use TOFU model like TCP
        ..Default::default()
    };

    Ok(config)
}

#[cfg(test)]
mod tests {
    // Integration tests need a real DTLS listener (certificate files);
    // packet-handling unit tests live in nexus-common/src/voice.rs.
}
