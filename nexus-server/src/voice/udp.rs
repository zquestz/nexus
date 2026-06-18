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

use tracing::{debug, warn};

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
/// A pending DTLS handshake holds a slot in the bounded handshake pool until it
/// completes or this fires. Kept short so a spoofed-ClientHello flood (whose
/// handshakes can never complete) churns out of the pool quickly and new voice
/// joins recover fast.
const DTLS_HANDSHAKE_TIMEOUT_SECS: u64 = 15;
const MILLIS_PER_SECOND: u64 = 1_000;

static VOICE_IDLE_CLOCK_START: OnceLock<Instant> = OnceLock::new();

use crate::channels::ChannelManager;
use crate::connection_tracker::{ConnectionTracker, VoiceGuard};
use crate::constants::*;
use crate::db::Permission;
use crate::ip_rule_cache::IpRuleState;
use crate::users::UserManager;

use super::demux::{PendingVoiceConnection, VoiceUdpConnHandle, VoiceUdpListener};
use super::registry::BindUdpAddrOutcome;
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
                    // `accept()` errors only when the demux has torn down and
                    // dropped `accept_tx`; there is no recovery path for this listener.
                    debug!(err = %e, "{}", LOG_VOICE_ACCEPT_ERROR);
                    break;
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

        if self
            .remove_client_if_current(remote_addr, &client)
            .await
            .is_some()
        {
            self.remove_voice_session_by_udp_addr(remote_addr, LOG_VOICE_DISCONNECTED_SESSION)
                .await;
        }
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
        let Some(packet) = Self::parse_packet(remote_addr, data) else {
            return true;
        };
        client.mark_packet_received();
        self.handle_parsed_packet(remote_addr, &packet).await
    }

    fn parse_packet<'a>(remote_addr: SocketAddr, data: &'a [u8]) -> Option<VoicePacketRef<'a>> {
        let packet = VoicePacketRef::from_bytes(data);
        if packet.is_none() {
            // debug, not warn: this is logged before the token check, so any
            // unauthenticated DTLS peer (e.g. a scanner) can flood it per packet,
            // and there's no remedy worth a per-occurrence warning.
            debug!(ip = %remote_addr, "{}", LOG_VOICE_INVALID_PACKET);
        }
        packet
    }

    #[cfg(test)]
    async fn handle_packet_for_test(&self, remote_addr: SocketAddr, data: &[u8]) -> bool {
        let Some(packet) = Self::parse_packet(remote_addr, data) else {
            return true;
        };
        self.handle_parsed_packet(remote_addr, &packet).await
    }

    async fn handle_parsed_packet(
        &self,
        remote_addr: SocketAddr,
        packet: &VoicePacketRef<'_>,
    ) -> bool {
        // Validate the token on every packet — the session may have been
        // removed via VoiceLeave since the connection opened.
        let Some(session) = self.registry.get_by_token(packet.token).await else {
            debug!(ip = %remote_addr, "{}", LOG_VOICE_SESSION_NOT_FOUND);
            return false; // Session gone, close connection
        };

        let sender_nickname = session.nickname.clone();
        let target_key = session.target_key();
        let session_id = session.session_id;

        match session.udp_addr {
            Some(bound_addr) if bound_addr == remote_addr => {}
            Some(_) => {
                debug!(ip = %remote_addr, "{}", LOG_VOICE_SESSION_ADDR_MISMATCH);
                return false;
            }
            None => match self.registry.bind_udp_addr(packet.token, remote_addr).await {
                BindUdpAddrOutcome::Bound => {}
                BindUdpAddrOutcome::TokenNotFound => {
                    debug!(ip = %remote_addr, "{}", LOG_VOICE_SESSION_NOT_FOUND);
                    return false;
                }
                BindUdpAddrOutcome::TokenAlreadyBound => {
                    debug!(ip = %remote_addr, "{}", LOG_VOICE_SESSION_ADDR_MISMATCH);
                    return false;
                }
                BindUdpAddrOutcome::AddrInUse => {
                    debug!(ip = %remote_addr, "{}", LOG_VOICE_SESSION_ADDR_IN_USE);
                    return false;
                }
            },
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
                        self.relay_packet(packet, &sender_nickname, &target_key)
                            .await;
                    }
                    Some(false) => {
                        // debug, not warn: a session without voice_talk can send
                        // this per packet, so per-occurrence it's noise.
                        debug!(user = %sender_nickname, "{}", LOG_VOICE_NO_PERMISSION);
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
        let recipients = self
            .registry
            .relay_recipients(target_key, &sender_folded)
            .await;

        let conns: Vec<_> = {
            let clients = self.clients.read().await;
            recipients
                .into_iter()
                .filter_map(|udp_addr| clients.get(&udp_addr).map(|client| client.conn.clone()))
                .collect()
        };

        // A failed relay send is ignored on purpose: it's per-packet and
        // self-limiting — a UDP send to a vanished peer succeeds, and a
        // genuinely-errored conn is already being torn down (idle recipients
        // are reaped within VOICE_SESSION_TIMEOUT_SECS).
        for conn in conns {
            let _ = conn.send(&relayed_bytes).await;
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

                self.remove_voice_session_by_udp_addr(addr, LOG_VOICE_TIMED_OUT_SESSION)
                    .await;

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

    async fn remove_voice_session_by_udp_addr(&self, addr: SocketAddr, log_message: &'static str) {
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
            log_message
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
    use std::any::Any;
    use std::collections::HashSet;
    use std::net::SocketAddr;
    use std::sync::Arc;
    use std::time::Duration;

    use dtls::config::Config as DtlsConfig;
    use nexus_common::protocol::ServerMessage;
    use nexus_common::voice::{RelayedVoicePacket, VoiceMessageType, VoicePacket};
    use tokio::net::UdpSocket;
    use tokio::sync::mpsc;
    use tokio::time::timeout;
    use webrtc_util::conn::Conn;
    use webrtc_util::{Error as WebRtcError, Result as WebRtcResult};

    use crate::db::Permission;
    use crate::handlers::testing::{create_test_context, login_user};
    use crate::voice::VoiceSession;
    use crate::voice::demux::{VoiceUdpConnHandle, VoiceUdpListener};

    use super::{
        BindUdpAddrOutcome, DtlsClient, VoiceControlCommand, VoiceControlHandle, VoiceListener,
        VoiceUdpServer, relayed_speaking_stopped_bytes,
    };

    const TEST_TIMEOUT: Duration = Duration::from_millis(500);

    struct RecordingConn {
        local_addr: SocketAddr,
        remote_addr: SocketAddr,
        sent_tx: mpsc::UnboundedSender<Vec<u8>>,
    }

    #[async_trait::async_trait]
    impl Conn for RecordingConn {
        async fn connect(&self, _addr: SocketAddr) -> WebRtcResult<()> {
            Ok(())
        }

        async fn recv(&self, _buf: &mut [u8]) -> WebRtcResult<usize> {
            Err(WebRtcError::ErrUseClosedNetworkConn)
        }

        async fn recv_from(&self, _buf: &mut [u8]) -> WebRtcResult<(usize, SocketAddr)> {
            Err(WebRtcError::ErrUseClosedNetworkConn)
        }

        async fn send(&self, buf: &[u8]) -> WebRtcResult<usize> {
            self.sent_tx
                .send(buf.to_vec())
                .map_err(|_| WebRtcError::ErrUseClosedNetworkConn)?;
            Ok(buf.len())
        }

        async fn send_to(&self, buf: &[u8], _target: SocketAddr) -> WebRtcResult<usize> {
            self.send(buf).await
        }

        fn local_addr(&self) -> WebRtcResult<SocketAddr> {
            Ok(self.local_addr)
        }

        fn remote_addr(&self) -> Option<SocketAddr> {
            Some(self.remote_addr)
        }

        async fn close(&self) -> WebRtcResult<()> {
            Ok(())
        }

        fn as_any(&self) -> &(dyn Any + Send + Sync) {
            self
        }
    }

    struct ClosingConn {
        local_addr: SocketAddr,
        remote_addr: SocketAddr,
    }

    #[async_trait::async_trait]
    impl Conn for ClosingConn {
        async fn connect(&self, _addr: SocketAddr) -> WebRtcResult<()> {
            Ok(())
        }

        async fn recv(&self, _buf: &mut [u8]) -> WebRtcResult<usize> {
            Ok(0)
        }

        async fn recv_from(&self, _buf: &mut [u8]) -> WebRtcResult<(usize, SocketAddr)> {
            Ok((0, self.remote_addr))
        }

        async fn send(&self, buf: &[u8]) -> WebRtcResult<usize> {
            Ok(buf.len())
        }

        async fn send_to(&self, buf: &[u8], _target: SocketAddr) -> WebRtcResult<usize> {
            Ok(buf.len())
        }

        fn local_addr(&self) -> WebRtcResult<SocketAddr> {
            Ok(self.local_addr)
        }

        fn remote_addr(&self) -> Option<SocketAddr> {
            Some(self.remote_addr)
        }

        async fn close(&self) -> WebRtcResult<()> {
            Ok(())
        }

        fn as_any(&self) -> &(dyn Any + Send + Sync) {
            self
        }
    }

    async fn expect_voice_user_left(
        rx: &mut crate::users::user::SessionRx,
        nickname: &str,
        target: &str,
    ) {
        let event = timeout(TEST_TIMEOUT, rx.recv())
            .await
            .expect("timed out waiting for voice leave notification")
            .expect("voice leave notification")
            .expect_message();
        let (message, message_id) = event;

        assert_eq!(message_id, None);
        match message {
            ServerMessage::VoiceUserLeft {
                nickname: actual_nickname,
                target: actual_target,
            } => {
                assert_eq!(actual_nickname, nickname);
                assert_eq!(actual_target, target);
            }
            other => panic!("Expected VoiceUserLeft, got {:?}", other),
        }
    }

    fn client_hello_probe() -> Vec<u8> {
        vec![22, 0xfe, 0xfd, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1]
    }

    async fn raw_voice_conn_handle() -> VoiceUdpConnHandle {
        let listener = VoiceUdpListener::bind("127.0.0.1:0".parse().unwrap())
            .await
            .expect("bind listener");
        let client = UdpSocket::bind("127.0.0.1:0")
            .await
            .expect("bind UDP client");
        client
            .connect(listener.local_addr().expect("listener addr"))
            .await
            .expect("connect UDP client");
        client
            .send(&client_hello_probe())
            .await
            .expect("send client hello probe");

        let pending = timeout(TEST_TIMEOUT, listener.accept())
            .await
            .expect("accept should complete")
            .expect("accept should succeed");
        let (_conn, raw_conn, pending_permit) = pending.into_parts();
        drop(pending_permit);
        raw_conn
    }

    async fn test_voice_server(test_ctx: &crate::handlers::testing::TestContext) -> VoiceUdpServer {
        let listener = VoiceListener {
            demux: VoiceUdpListener::bind("127.0.0.1:0".parse().unwrap())
                .await
                .expect("bind listener"),
            dtls_config: DtlsConfig::default(),
        };
        let (_control, control_rx) = VoiceControlHandle::channel();
        VoiceUdpServer::new(
            listener,
            test_ctx.voice_registry.clone(),
            test_ctx.ip_rule_cache.clone(),
            test_ctx.user_manager.clone(),
            test_ctx.channel_manager.clone(),
            test_ctx.connection_tracker.clone(),
            control_rx,
        )
    }

    #[tokio::test]
    async fn udp_packet_closes_on_address_binding_conflict() {
        let mut test_ctx = create_test_context().await;
        let alice_session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::VoiceListen, Permission::VoiceTalk],
            false,
        )
        .await;
        let bob_session_id = login_user(
            &mut test_ctx,
            "bob",
            "password",
            &[Permission::VoiceListen, Permission::VoiceTalk],
            false,
        )
        .await;
        let server = test_voice_server(&test_ctx).await;

        let alice_voice = VoiceSession::new(
            "alice".to_string(),
            vec!["#general".to_string()],
            alice_session_id,
        );
        let alice_token = alice_voice.token;
        test_ctx
            .voice_registry
            .add(alice_voice)
            .await
            .expect("alice voice session is unique");

        let bob_voice = VoiceSession::new(
            "bob".to_string(),
            vec!["#general".to_string()],
            bob_session_id,
        );
        let bob_token = bob_voice.token;
        test_ctx
            .voice_registry
            .add(bob_voice)
            .await
            .expect("bob voice session is unique");

        let alice_addr: SocketAddr = "127.0.0.1:43000".parse().unwrap();
        let rebound_addr: SocketAddr = "127.0.0.1:43001".parse().unwrap();
        assert_eq!(
            test_ctx
                .voice_registry
                .bind_udp_addr(alice_token, alice_addr)
                .await,
            BindUdpAddrOutcome::Bound
        );

        let alice_packet_from_other_addr = VoicePacket::keepalive(alice_token, 1).to_bytes();
        assert!(
            !server
                .handle_packet_for_test(rebound_addr, &alice_packet_from_other_addr)
                .await,
            "same token from a different address should close that connection"
        );
        assert_eq!(
            test_ctx
                .voice_registry
                .get_by_token(alice_token)
                .await
                .expect("alice session must remain")
                .udp_addr,
            Some(alice_addr),
            "collision must not mutate the established binding"
        );

        let bob_packet_from_alice_addr = VoicePacket::keepalive(bob_token, 1).to_bytes();
        assert!(
            !server
                .handle_packet_for_test(alice_addr, &bob_packet_from_alice_addr)
                .await,
            "another token using an owned address should close that connection"
        );
        assert_eq!(
            test_ctx
                .voice_registry
                .get_by_token(alice_token)
                .await
                .expect("alice session must remain")
                .udp_addr,
            Some(alice_addr)
        );
        assert_eq!(
            test_ctx
                .voice_registry
                .get_by_token(bob_token)
                .await
                .expect("bob session must remain")
                .udp_addr,
            None,
            "rejected collision must not bind the second session"
        );
    }

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

    #[tokio::test]
    async fn udp_voice_talk_revoke_blocks_transmit_packets_but_allows_keepalive() {
        let mut test_ctx = create_test_context().await;
        let alice_session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::VoiceListen, Permission::VoiceTalk],
            false,
        )
        .await;
        let bob_session_id = login_user(
            &mut test_ctx,
            "bob",
            "password",
            &[Permission::VoiceListen],
            false,
        )
        .await;
        let alice_user = test_ctx
            .db
            .users
            .get_user_by_username("alice")
            .await
            .unwrap()
            .expect("alice user");
        let server = test_voice_server(&test_ctx).await;

        let alice_voice = VoiceSession::new(
            "alice".to_string(),
            vec!["#general".to_string()],
            alice_session_id,
        );
        let alice_token = alice_voice.token;
        test_ctx
            .voice_registry
            .add(alice_voice)
            .await
            .expect("alice voice session is unique");

        let bob_addr: SocketAddr = "127.0.0.1:41000".parse().unwrap();
        let bob_voice = VoiceSession::new(
            "bob".to_string(),
            vec!["#general".to_string()],
            bob_session_id,
        );
        let bob_token = bob_voice.token;
        test_ctx
            .voice_registry
            .add(bob_voice)
            .await
            .expect("bob voice session is unique");
        assert!(
            test_ctx
                .voice_registry
                .bind_udp_addr(bob_token, bob_addr)
                .await
                == BindUdpAddrOutcome::Bound
        );

        let (sent_tx, mut sent_rx) = mpsc::unbounded_channel();
        let recipient_conn: Arc<dyn Conn + Send + Sync> = Arc::new(RecordingConn {
            local_addr: "127.0.0.1:0".parse().unwrap(),
            remote_addr: bob_addr,
            sent_tx,
        });
        server.clients.write().await.insert(
            bob_addr,
            Arc::new(DtlsClient::new(
                recipient_conn,
                raw_voice_conn_handle().await,
            )),
        );

        let alice_addr: SocketAddr = "127.0.0.1:42000".parse().unwrap();
        let first_voice_data =
            VoicePacket::voice_data(alice_token, 1, 480, vec![1, 2, 3]).to_bytes();
        assert!(
            server
                .handle_packet_for_test(alice_addr, &first_voice_data)
                .await
        );
        let relayed =
            RelayedVoicePacket::from_bytes(&sent_rx.try_recv().expect("voice data relay"))
                .expect("relayed voice packet decodes");
        assert_eq!(relayed.msg_type, VoiceMessageType::VoiceData);
        assert_eq!(relayed.sender, "alice");
        assert_eq!(relayed.sequence, 1);
        assert_eq!(relayed.timestamp, 480);
        assert_eq!(relayed.payload, vec![1, 2, 3]);

        test_ctx
            .user_manager
            .update_permissions(alice_user.id, HashSet::from([Permission::VoiceListen]))
            .await;

        let denied_packets = [
            VoicePacket::voice_data(alice_token, 2, 960, vec![4, 5, 6]).to_bytes(),
            VoicePacket::speaking_started(alice_token, 3).to_bytes(),
            VoicePacket::speaking_stopped(alice_token, 4).to_bytes(),
        ];
        for packet in denied_packets {
            assert!(server.handle_packet_for_test(alice_addr, &packet).await);
        }
        assert!(
            sent_rx.try_recv().is_err(),
            "voice_talk revoke must stop relaying VoiceData and speaking indicators"
        );

        let keepalive = VoicePacket::keepalive(alice_token, 5).to_bytes();
        assert!(server.handle_packet_for_test(alice_addr, &keepalive).await);
        assert!(
            sent_rx.try_recv().is_err(),
            "keepalive should not relay to other participants"
        );
        assert!(
            test_ctx.voice_registry.has_session(alice_session_id).await,
            "revoked voice_talk must leave the listener-side voice session active"
        );
    }

    #[tokio::test]
    async fn dtls_disconnect_removes_bound_voice_session_and_notifies() {
        let mut test_ctx = create_test_context().await;
        let alice_session_id = login_user(
            &mut test_ctx,
            "alice",
            "password",
            &[Permission::VoiceListen, Permission::VoiceTalk],
            false,
        )
        .await;
        let bob_session_id = login_user(
            &mut test_ctx,
            "bob",
            "password",
            &[Permission::VoiceListen],
            false,
        )
        .await;
        let server = test_voice_server(&test_ctx).await;

        for session_id in [alice_session_id, bob_session_id] {
            test_ctx
                .channel_manager
                .join(
                    "#general",
                    session_id,
                    crate::channels::JoinPolicy::CreateIfMissing,
                )
                .await
                .expect("join channel");
        }

        let alice_addr: SocketAddr = "127.0.0.1:42010".parse().unwrap();
        let alice_voice = VoiceSession::new(
            "alice".to_string(),
            vec!["#general".to_string()],
            alice_session_id,
        );
        let alice_token = alice_voice.token;
        test_ctx
            .voice_registry
            .add(alice_voice)
            .await
            .expect("alice voice session is unique");
        assert!(
            test_ctx
                .voice_registry
                .bind_udp_addr(alice_token, alice_addr)
                .await
                == BindUdpAddrOutcome::Bound
        );

        let closing_conn: Arc<dyn Conn + Send + Sync> = Arc::new(ClosingConn {
            local_addr: "127.0.0.1:0".parse().unwrap(),
            remote_addr: alice_addr,
        });
        let client = Arc::new(DtlsClient::new(closing_conn, raw_voice_conn_handle().await));
        server
            .clients
            .write()
            .await
            .insert(alice_addr, client.clone());

        server.handle_connection(client, alice_addr).await;

        assert!(
            !test_ctx.voice_registry.has_session(alice_session_id).await,
            "DTLS disconnect must remove the bound voice registry session"
        );
        expect_voice_user_left(&mut test_ctx.rx, "alice", "#general").await;
    }

    // Integration tests need a real DTLS listener (certificate files);
    // packet-handling unit tests live in nexus-common/src/voice.rs.
}
