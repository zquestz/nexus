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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use tracing::{debug, error, warn};

use dtls::config::Config as DtlsConfig;
use dtls::conn::DTLSConn;
use dtls::crypto::Certificate;
use tokio::sync::{Mutex, RwLock, mpsc};

use webrtc_util::conn::Conn;

use nexus_common::address::normalize_socket_addr;
use nexus_common::names::fold_name;
use nexus_common::voice::{
    MAX_VOICE_PACKET_SIZE, RelayedVoicePacket, VOICE_SESSION_TIMEOUT_SECS, VoiceMessageType,
    VoicePacketRef,
};

const STALE_CLIENT_CHECK_INTERVAL_SECS: u64 = 30;
const DTLS_HANDSHAKE_TIMEOUT_SECS: u64 = 30;
const MILLIS_PER_SECOND: u64 = 1_000;

static VOICE_IDLE_CLOCK_START: OnceLock<Instant> = OnceLock::new();

use crate::channels::ChannelManager;
use crate::connection_tracker::{ConnectionTracker, VoiceGuard};
use crate::constants::*;
use crate::db::Permission;
use crate::ip_rule_cache::IpRuleState;
use crate::users::UserManager;

use super::demux::{PendingVoiceConnection, VoiceUdpConnHandle, VoiceUdpListener};
use super::{VoiceRegistry, send_voice_leave_notifications};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoiceControlCommand {
    SpeakingStopped { session_id: u32 },
}

#[derive(Clone)]
pub struct VoiceControlHandle {
    tx: mpsc::UnboundedSender<VoiceControlCommand>,
}

impl VoiceControlHandle {
    pub fn channel() -> (Self, mpsc::UnboundedReceiver<VoiceControlCommand>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self { tx }, rx)
    }

    pub fn speaking_stopped(&self, session_id: u32) {
        let _ = self
            .tx
            .send(VoiceControlCommand::SpeakingStopped { session_id });
    }
}

struct DtlsClient {
    conn: Arc<dyn Conn + Send + Sync>,
    raw_conn: VoiceUdpConnHandle,
    /// Last packet received in milliseconds since `VOICE_IDLE_CLOCK_START`.
    last_packet_millis: AtomicU64,
}

impl DtlsClient {
    fn new(conn: Arc<dyn Conn + Send + Sync>, raw_conn: VoiceUdpConnHandle) -> Self {
        Self {
            conn,
            raw_conn,
            last_packet_millis: AtomicU64::new(voice_idle_now_millis()),
        }
    }

    fn connection_handles(&self) -> (Arc<dyn Conn + Send + Sync>, VoiceUdpConnHandle) {
        (self.conn.clone(), self.raw_conn.clone())
    }

    fn mark_packet_received(&self) {
        self.last_packet_millis
            .store(voice_idle_now_millis(), Ordering::Relaxed);
    }

    fn is_idle(&self, now_millis: u64, timeout_millis: u64) -> bool {
        let last_packet_millis = self.last_packet_millis.load(Ordering::Relaxed);
        now_millis.saturating_sub(last_packet_millis) > timeout_millis
    }
}

pub struct VoiceListener {
    demux: VoiceUdpListener,
    dtls_config: DtlsConfig,
}

impl VoiceListener {
    async fn accept(&self) -> webrtc_util::Result<PendingVoiceConnection> {
        self.demux.accept().await
    }

    fn dtls_config(&self) -> DtlsConfig {
        self.dtls_config.clone()
    }
}

pub struct VoiceUdpServer {
    listener: VoiceListener,
    registry: VoiceRegistry,
    /// Active DTLS clients, keyed by remote address.
    clients: Arc<RwLock<HashMap<SocketAddr, Arc<DtlsClient>>>>,
    control_rx: Mutex<mpsc::UnboundedReceiver<VoiceControlCommand>>,
    ip_rule_cache: Arc<IpRuleState>,
    user_manager: UserManager,
    channel_manager: ChannelManager,
    connection_tracker: Arc<ConnectionTracker>,
}

impl VoiceUdpServer {
    pub fn new(
        listener: VoiceListener,
        registry: VoiceRegistry,
        ip_rule_cache: Arc<IpRuleState>,
        user_manager: UserManager,
        channel_manager: ChannelManager,
        connection_tracker: Arc<ConnectionTracker>,
        control_rx: mpsc::UnboundedReceiver<VoiceControlCommand>,
    ) -> Self {
        Self {
            listener,
            registry,
            clients: Arc::new(RwLock::new(HashMap::new())),
            control_rx: Mutex::new(control_rx),
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

        let control_self = self.clone();
        tokio::spawn(async move {
            control_self.control_loop().await;
        });

        loop {
            match self.listener.accept().await {
                Ok(pending) => self.handle_pending_connection(pending).await,
                Err(e) => {
                    warn!(err = %e, "{}", LOG_VOICE_ACCEPT_ERROR);
                }
            }
        }
    }

    async fn handle_pending_connection(self: &Arc<Self>, pending: PendingVoiceConnection) {
        // Fold IPv4-mapped IPv6 to IPv4 so lookup keys match the form the TCP
        // path registered (it normalizes at accept too).
        let remote_addr = normalize_socket_addr(pending.remote_addr());
        let should_allow = self.ip_rule_cache.should_allow(remote_addr.ip()).await;

        if !should_allow {
            debug!(ip = %remote_addr.ip(), "{}", LOG_VOICE_REJECTED_BANNED);
            pending.close().await;
            return;
        }

        // Count pending handshakes against the voice per-IP cap so a peer
        // cannot bypass the limit by never completing DTLS.
        let Some(voice_guard) = self.connection_tracker.try_acquire_voice(remote_addr.ip()) else {
            debug!(ip = %remote_addr.ip(), "{}", LOG_VOICE_REJECTED_LIMIT);
            pending.close().await;
            return;
        };

        let server = self.clone();
        let dtls_config = self.listener.dtls_config();
        tokio::spawn(async move {
            server
                .complete_pending_handshake(pending, remote_addr, voice_guard, dtls_config)
                .await;
        });
    }

    async fn complete_pending_handshake(
        self: Arc<Self>,
        pending: PendingVoiceConnection,
        remote_addr: SocketAddr,
        voice_guard: VoiceGuard,
        dtls_config: DtlsConfig,
    ) {
        let (dtls_raw_conn, raw_conn, pending_permit) = pending.into_parts();

        let handshake_result = tokio::time::timeout(
            Duration::from_secs(DTLS_HANDSHAKE_TIMEOUT_SECS),
            DTLSConn::new(dtls_raw_conn, dtls_config, false, None),
        )
        .await;

        drop(pending_permit);

        match handshake_result {
            Ok(Ok(dtls_conn)) => {
                let conn: Arc<dyn Conn + Send + Sync> = Arc::new(dtls_conn);
                self.register_connection(conn, raw_conn, remote_addr, voice_guard)
                    .await;
            }
            Ok(Err(e)) => {
                debug!(ip = %remote_addr, err = %e, "{}", LOG_VOICE_HANDSHAKE_FAILED);
                raw_conn.close().await;
            }
            Err(_) => {
                debug!(ip = %remote_addr, "{}", LOG_VOICE_HANDSHAKE_TIMEOUT);
                raw_conn.close().await;
            }
        }
    }

    async fn register_connection(
        self: Arc<Self>,
        conn: Arc<dyn Conn + Send + Sync>,
        raw_conn: VoiceUdpConnHandle,
        remote_addr: SocketAddr,
        voice_guard: VoiceGuard,
    ) {
        debug!(ip = %remote_addr, "{}", LOG_VOICE_NEW_CONNECTION);

        let client = Arc::new(DtlsClient::new(conn, raw_conn));

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

    async fn remove_client_if_current(
        &self,
        remote_addr: SocketAddr,
        client: &Arc<DtlsClient>,
    ) -> Option<Arc<DtlsClient>> {
        let mut clients = self.clients.write().await;
        if clients
            .get(&remote_addr)
            .is_some_and(|current| Arc::ptr_eq(current, client))
        {
            return clients.remove(&remote_addr);
        }

        None
    }

    async fn handle_connection(&self, client: Arc<DtlsClient>, remote_addr: SocketAddr) {
        let mut buf = vec![0u8; MAX_VOICE_PACKET_SIZE + 100]; // Extra for DTLS overhead
        let (conn, raw_conn) = client.connection_handles();

        loop {
            match conn.recv(&mut buf).await {
                Ok(len) if len > 0 => {
                    if !self.handle_packet(&client, remote_addr, &buf[..len]).await {
                        break; // Session gone
                    }
                }
                Ok(_) => {
                    // Zero-length read = connection closed
                    debug!(ip = %remote_addr, "{}", LOG_VOICE_CONNECTION_CLOSED);
                    break;
                }
                Err(e) => {
                    warn!(ip = %remote_addr, err = %e, "{}", LOG_VOICE_READ_ERROR);
                    break;
                }
            }
        }

        self.remove_client_if_current(remote_addr, &client).await;
        let _ = conn.close().await;
        raw_conn.close().await;
    }

    /// Returns `false` if the connection should be closed (session gone).
    async fn handle_packet(
        &self,
        client: &Arc<DtlsClient>,
        remote_addr: SocketAddr,
        data: &[u8],
    ) -> bool {
        let Some(packet) = VoicePacketRef::from_bytes(data) else {
            warn!(ip = %remote_addr, "{}", LOG_VOICE_INVALID_PACKET);
            return true; // Invalid packet, but keep connection
        };

        client.mark_packet_received();

        // Validate the token on every packet — the session may have been
        // removed via VoiceLeave since the connection opened.
        let Some(session) = self.registry.get_by_token(packet.token).await else {
            debug!(ip = %remote_addr, "{}", LOG_VOICE_SESSION_NOT_FOUND);
            return false; // Session gone, close connection
        };

        let sender_nickname = session.nickname.clone();
        let target_key = session.target_key();
        let session_id = session.session_id;

        if session.udp_addr.is_none() {
            self.registry.set_udp_addr(packet.token, remote_addr).await;
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
                // Idle timestamp already bumped above; nothing else to do.
                debug!(user = %sender_nickname, ip = %remote_addr, "{}", LOG_VOICE_KEEPALIVE);
            }
            VoiceMessageType::VoiceData
            | VoiceMessageType::SpeakingStarted
            | VoiceMessageType::SpeakingStopped => {
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
    async fn relay_packet(
        &self,
        packet: &VoicePacketRef<'_>,
        sender_nickname: &str,
        target_key: &str,
    ) {
        let relayed_bytes = RelayedVoicePacket::to_bytes_from_voice_packet(packet, sender_nickname);
        self.relay_bytes(relayed_bytes, sender_nickname, target_key)
            .await;
    }

    async fn relay_bytes(&self, relayed_bytes: Vec<u8>, sender_nickname: &str, target_key: &str) {
        let sender_folded = fold_name(sender_nickname);
        let sessions = self.registry.get_sessions_for_target(target_key).await;

        let targets = {
            let clients = self.clients.read().await;
            sessions
                .into_iter()
                .filter_map(|session| {
                    // Never echo back to the sender
                    if fold_name(&session.nickname) == sender_folded {
                        return None;
                    }

                    let udp_addr = session.udp_addr?;
                    clients
                        .get(&udp_addr)
                        .map(|client| (session.nickname, udp_addr, client.clone()))
                })
                .collect::<Vec<_>>()
        };

        for (nickname, udp_addr, client) in targets {
            let conn = client.conn.clone();

            if let Err(e) = conn.send(&relayed_bytes).await {
                error!(user = %nickname, ip = %udp_addr, err = %e, "{}", LOG_VOICE_RELAY_FAILED);
            }
        }
    }

    async fn control_loop(&self) {
        loop {
            let command = {
                let mut rx = self.control_rx.lock().await;
                rx.recv().await
            };

            match command {
                Some(VoiceControlCommand::SpeakingStopped { session_id }) => {
                    self.relay_speaking_stopped(session_id).await;
                }
                None => break,
            }
        }
    }

    async fn relay_speaking_stopped(&self, session_id: u32) {
        let Some(session) = self.registry.get_by_session_id(session_id).await else {
            return;
        };

        let relayed_bytes = relayed_speaking_stopped_bytes(&session.nickname);
        self.relay_bytes(relayed_bytes, &session.nickname, &session.target_key())
            .await;
    }

    async fn cleanup_loop(&self) {
        let check_interval = Duration::from_secs(STALE_CLIENT_CHECK_INTERVAL_SECS);

        loop {
            tokio::time::sleep(check_interval).await;

            let now_millis = voice_idle_now_millis();
            let timeout_millis = VOICE_SESSION_TIMEOUT_SECS.saturating_mul(MILLIS_PER_SECOND);

            let clients_snapshot = {
                let clients = self.clients.read().await;
                clients
                    .iter()
                    .map(|(addr, client)| (*addr, client.clone()))
                    .collect::<Vec<_>>()
            };

            let mut timed_out_candidates = Vec::new();
            for (addr, client) in clients_snapshot {
                if client.is_idle(now_millis, timeout_millis) {
                    timed_out_candidates.push((addr, client));
                }
            }

            for (addr, client) in timed_out_candidates {
                let Some(client) = self.remove_client_if_current(addr, &client).await else {
                    continue;
                };

                self.remove_timed_out_voice_session(addr).await;

                let (conn, raw_conn) = client.connection_handles();

                debug!(ip = %addr, "{}", LOG_VOICE_CLEANUP_TIMEOUT);
                let _ = conn.close().await;
                raw_conn.close().await;
            }

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

    async fn remove_timed_out_voice_session(&self, addr: SocketAddr) {
        // Hold the user-state read lock across removal + notifications so a
        // concurrent rename cannot make VoiceUserLeft carry a stale nickname.
        let _user_state = self.user_manager.read_user_state().await;
        let Some(info) = self.registry.remove_by_udp_addr(addr).await else {
            return;
        };

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

        debug!(
            user = %info.session.nickname,
            ip = %addr,
            "{}",
            LOG_VOICE_TIMED_OUT_SESSION
        );
    }
}

fn voice_idle_now_millis() -> u64 {
    let elapsed = VOICE_IDLE_CLOCK_START.get_or_init(Instant::now).elapsed();
    u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
}

fn relayed_speaking_stopped_bytes(nickname: &str) -> Vec<u8> {
    RelayedVoicePacket {
        msg_type: VoiceMessageType::SpeakingStopped,
        sender: nickname.to_string(),
        sequence: 0,
        timestamp: 0,
        payload: Vec::new(),
    }
    .to_bytes()
}

/// Create a DTLS listener for voice traffic, bound to `addr` (same IP as TCP).
pub async fn create_voice_listener(
    addr: SocketAddr,
    cert_path: &Path,
    key_path: &Path,
) -> Result<VoiceListener, String> {
    let config = load_dtls_config(cert_path, key_path)?;

    let demux = VoiceUdpListener::bind(addr)
        .await
        .map_err(|e| format!("{}{}: {}", ERR_VOICE_DTLS_LISTENER_PREFIX, addr, e))?;

    Ok(VoiceListener {
        demux,
        dtls_config: config,
    })
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
    use nexus_common::voice::{RelayedVoicePacket, VoiceMessageType};

    use super::{VoiceControlCommand, VoiceControlHandle, relayed_speaking_stopped_bytes};

    #[test]
    fn voice_control_handle_queues_speaking_stopped() {
        let (handle, mut rx) = VoiceControlHandle::channel();

        handle.speaking_stopped(42);

        assert_eq!(
            rx.try_recv(),
            Ok(VoiceControlCommand::SpeakingStopped { session_id: 42 })
        );
    }

    #[test]
    fn server_synthesized_speaking_stopped_packet_is_cleanup_only() {
        let bytes = relayed_speaking_stopped_bytes("alice");
        let packet = RelayedVoicePacket::from_bytes(&bytes).expect("packet should decode");

        assert_eq!(packet.msg_type, VoiceMessageType::SpeakingStopped);
        assert_eq!(packet.sender, "alice");
        assert_eq!(packet.sequence, 0);
        assert_eq!(packet.timestamp, 0);
        assert!(packet.payload.is_empty());
    }

    // Integration tests need a real DTLS listener (certificate files);
    // packet-handling unit tests live in nexus-common/src/voice.rs.
}
