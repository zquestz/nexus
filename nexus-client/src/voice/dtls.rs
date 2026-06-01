//! DTLS client for voice chat UDP connection
//!
//! Establishes a DTLS-encrypted UDP connection to the server for
//! real-time voice packet transmission.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use dtls::config::Config as DtlsConfig;
use dtls::conn::DTLSConn;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use uuid::Uuid;
use webrtc_util::Conn;

use nexus_common::voice::{
    RelayedVoicePacket, VOICE_KEEPALIVE_INTERVAL_SECS, VoiceMessageType, VoicePacket,
};

// =============================================================================
// Constants
// =============================================================================

/// Buffer size for receiving packets
const RECV_BUFFER_SIZE: usize = 2048;

/// Connection timeout for DTLS handshake
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// Receive poll timeout in milliseconds (allows select! to check other branches)
const RECV_POLL_TIMEOUT_MS: u64 = 100;

// =============================================================================
// Voice DTLS Client
// =============================================================================

/// DTLS client for voice chat
///
/// Handles the encrypted UDP connection to the server, sending
/// voice packets and receiving relayed packets from other users.
pub struct VoiceDtlsClient {
    /// The DTLS connection (stored as trait object for Conn methods)
    conn: Arc<dyn Conn + Send + Sync>,
    /// Voice session token for authentication
    token: Uuid,
    /// Current sequence number for outgoing packets
    sequence: u32,
    /// Current timestamp for outgoing packets (in samples)
    timestamp: u32,
    /// Pre-allocated receive buffer (reused across recv calls to avoid heap churn)
    recv_buf: Vec<u8>,
}

impl VoiceDtlsClient {
    /// Connect to the voice server
    ///
    /// # Arguments
    /// * `server_addr` - Server address (host:port)
    /// * `token` - Voice session token from VoiceJoinResponse
    ///
    /// # Returns
    /// * `Ok(VoiceDtlsClient)` - Connected client
    /// * `Err(String)` - Error message if connection failed
    pub async fn connect(server_addr: SocketAddr, token: Uuid) -> Result<Self, String> {
        // Create UDP socket bound to any available port
        // Use the appropriate address family based on server address (IPv4 vs IPv6)
        let bind_addr = if server_addr.is_ipv6() {
            "[::]:0"
        } else {
            "0.0.0.0:0"
        };
        let socket = UdpSocket::bind(bind_addr)
            .await
            .map_err(|e| format!("Failed to bind UDP socket: {}", e))?;

        // Connect UDP socket to server address
        socket
            .connect(server_addr)
            .await
            .map_err(|e| format!("Failed to connect UDP socket: {}", e))?;

        let socket = Arc::new(socket);

        // Create DTLS config for client mode
        // We use insecure_skip_verify because we've already verified
        // the server's certificate via the TCP connection using TOFU
        let config = DtlsConfig {
            insecure_skip_verify: true,
            ..Default::default()
        };

        // Create the UDP connection wrapper
        let udp_conn = Arc::new(TokioUdpConn { socket });

        // Create DTLS connection with timeout
        // is_client=true for client-side connection
        let dtls_conn =
            tokio::time::timeout(CONNECT_TIMEOUT, DTLSConn::new(udp_conn, config, true, None))
                .await
                .map_err(|_| "DTLS handshake timeout".to_string())?
                .map_err(|e| format!("DTLS handshake failed: {}", e))?;

        // Store as trait object to use Conn methods
        let conn: Arc<dyn Conn + Send + Sync> = Arc::new(dtls_conn);

        Ok(Self {
            conn,
            token,
            sequence: 0,
            timestamp: 0,
            recv_buf: vec![0u8; RECV_BUFFER_SIZE],
        })
    }

    /// Send a voice data packet
    ///
    /// # Arguments
    /// * `opus_data` - Opus-encoded audio data
    ///
    /// # Returns
    /// * `Ok(())` - Packet sent successfully
    /// * `Err(String)` - Error if send failed
    pub async fn send_voice_data(&mut self, opus_data: Vec<u8>) -> Result<(), String> {
        let packet = VoicePacket::voice_data(self.token, self.sequence, self.timestamp, opus_data);

        self.send_packet(&packet).await?;

        // Increment sequence and timestamp
        self.sequence = self.sequence.wrapping_add(1);
        self.timestamp = self
            .timestamp
            .wrapping_add(nexus_common::voice::VOICE_SAMPLES_PER_FRAME);

        Ok(())
    }

    /// Send a keepalive packet
    ///
    /// Should be called periodically when not speaking to maintain the session.
    pub async fn send_keepalive(&mut self) -> Result<(), String> {
        let packet = VoicePacket::keepalive(self.token, self.sequence);
        self.send_packet(&packet).await
    }

    /// Send a speaking started indicator
    pub async fn send_speaking_started(&mut self) -> Result<(), String> {
        let packet = VoicePacket::speaking_started(self.token, self.sequence);
        self.send_packet(&packet).await
    }

    /// Send a speaking stopped indicator
    pub async fn send_speaking_stopped(&mut self) -> Result<(), String> {
        let packet = VoicePacket::speaking_stopped(self.token, self.sequence);
        self.send_packet(&packet).await
    }

    /// Send a packet over the DTLS connection
    async fn send_packet(&self, packet: &VoicePacket) -> Result<(), String> {
        let bytes = packet.to_bytes();
        self.conn
            .send(&bytes)
            .await
            .map_err(|e| format!("Failed to send voice packet: {}", e))?;
        Ok(())
    }

    /// Receive a relayed voice packet
    ///
    /// Uses a pre-allocated buffer to avoid per-recv heap allocation.
    /// Over a long voice session (~864k recv calls/day), this eliminates
    /// heap fragmentation from transient 2KB allocations.
    ///
    /// # Returns
    /// * `Ok(Some(packet))` - Received a packet
    /// * `Ok(None)` - Connection closed
    /// * `Err(String)` - Error receiving
    pub async fn recv(&mut self) -> Result<Option<RelayedVoicePacket>, String> {
        let len = self
            .conn
            .recv(&mut self.recv_buf)
            .await
            .map_err(|e| format!("Failed to receive: {}", e))?;

        if len == 0 {
            return Ok(None);
        }

        let packet = RelayedVoicePacket::from_bytes(&self.recv_buf[..len])
            .ok_or_else(|| "Invalid relayed packet".to_string())?;

        Ok(Some(packet))
    }

    /// Receive with timeout
    ///
    /// # Arguments
    /// * `timeout` - Maximum time to wait
    ///
    /// # Returns
    /// * `Ok(Some(packet))` - Received a packet
    /// * `Ok(None)` - Timeout or connection closed
    /// * `Err(String)` - Error receiving
    pub async fn recv_timeout(
        &mut self,
        timeout: Duration,
    ) -> Result<Option<RelayedVoicePacket>, String> {
        match tokio::time::timeout(timeout, self.recv()).await {
            Ok(result) => result,
            Err(_) => Ok(None), // Timeout
        }
    }

    /// Close the connection
    pub async fn close(&self) -> Result<(), String> {
        self.conn
            .close()
            .await
            .map_err(|e| format!("Failed to close connection: {}", e))
    }
}

// =============================================================================
// Tokio UDP Connection Wrapper
// =============================================================================

use std::any::Any;

/// Wrapper for tokio UdpSocket to implement webrtc_util::Conn trait
struct TokioUdpConn {
    socket: Arc<UdpSocket>,
}

#[async_trait::async_trait]
impl webrtc_util::Conn for TokioUdpConn {
    async fn connect(&self, _addr: SocketAddr) -> webrtc_util::Result<()> {
        // Already connected in constructor
        Ok(())
    }

    async fn recv(&self, buf: &mut [u8]) -> webrtc_util::Result<usize> {
        self.socket
            .recv(buf)
            .await
            .map_err(|e| webrtc_util::Error::Other(e.to_string()))
    }

    async fn recv_from(&self, buf: &mut [u8]) -> webrtc_util::Result<(usize, SocketAddr)> {
        self.socket
            .recv_from(buf)
            .await
            .map_err(|e| webrtc_util::Error::Other(e.to_string()))
    }

    async fn send(&self, buf: &[u8]) -> webrtc_util::Result<usize> {
        self.socket
            .send(buf)
            .await
            .map_err(|e| webrtc_util::Error::Other(e.to_string()))
    }

    async fn send_to(&self, buf: &[u8], target: SocketAddr) -> webrtc_util::Result<usize> {
        self.socket
            .send_to(buf, target)
            .await
            .map_err(|e| webrtc_util::Error::Other(e.to_string()))
    }

    fn local_addr(&self) -> webrtc_util::Result<SocketAddr> {
        self.socket
            .local_addr()
            .map_err(|e| webrtc_util::Error::Other(e.to_string()))
    }

    fn remote_addr(&self) -> Option<SocketAddr> {
        None
    }

    async fn close(&self) -> webrtc_util::Result<()> {
        Ok(())
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }
}

// =============================================================================
// Voice Client Runner
// =============================================================================

/// Events from the voice DTLS client
#[derive(Debug, Clone)]
pub enum VoiceDtlsEvent {
    /// Successfully connected to voice server
    Connected,
    /// Received a voice packet from another user
    VoiceReceived {
        sender: String,
        sequence: u32,
        timestamp: u32,
        payload: Vec<u8>,
    },
    /// Received a speaking started indicator
    SpeakingStarted { sender: String },
    /// Received a speaking stopped indicator
    SpeakingStopped { sender: String },
    /// Connection error
    Error(String),
    /// Connection closed
    Disconnected,
}

/// Commands to send to the voice DTLS client
#[derive(Debug)]
pub enum VoiceDtlsCommand {
    /// Send voice data
    SendVoice(Vec<u8>),
    /// Send speaking started
    SendSpeakingStarted,
    /// Send speaking stopped
    SendSpeakingStopped,
    /// Disconnect
    Disconnect,
}

/// Run the voice DTLS client as a background task
///
/// # Arguments
/// * `server_addr` - Server address to connect to
/// * `token` - Voice session token
/// * `event_tx` - Channel to send events
/// * `command_rx` - Channel to receive commands
pub async fn run_voice_client(
    server_addr: SocketAddr,
    token: Uuid,
    event_tx: mpsc::UnboundedSender<VoiceDtlsEvent>,
    mut command_rx: mpsc::UnboundedReceiver<VoiceDtlsCommand>,
) {
    // Connect to server
    let mut client = match VoiceDtlsClient::connect(server_addr, token).await {
        Ok(c) => c,
        Err(e) => {
            let _ = event_tx.send(VoiceDtlsEvent::Error(e));
            return;
        }
    };

    // Notify connected
    if event_tx.send(VoiceDtlsEvent::Connected).is_err() {
        return;
    }

    // Keepalive interval
    let mut keepalive_interval =
        tokio::time::interval(Duration::from_secs(VOICE_KEEPALIVE_INTERVAL_SECS));

    // Receive timeout
    let recv_timeout = Duration::from_millis(RECV_POLL_TIMEOUT_MS);

    loop {
        tokio::select! {
            // Handle incoming commands
            cmd = command_rx.recv() => {
                match cmd {
                    Some(VoiceDtlsCommand::SendVoice(data)) => {
                        if let Err(e) = client.send_voice_data(data).await {
                            let _ = event_tx.send(VoiceDtlsEvent::Error(e));
                        }
                    }
                    Some(VoiceDtlsCommand::SendSpeakingStarted) => {
                        if let Err(e) = client.send_speaking_started().await {
                            let _ = event_tx.send(VoiceDtlsEvent::Error(e));
                        }
                    }
                    Some(VoiceDtlsCommand::SendSpeakingStopped) => {
                        if let Err(e) = client.send_speaking_stopped().await {
                            let _ = event_tx.send(VoiceDtlsEvent::Error(e));
                        }
                    }
                    Some(VoiceDtlsCommand::Disconnect) | None => {
                        let _ = client.close().await;
                        let _ = event_tx.send(VoiceDtlsEvent::Disconnected);
                        return;
                    }
                }
            }

            // Handle incoming packets
            result = client.recv_timeout(recv_timeout) => {
                match result {
                    Ok(Some(packet)) => {
                        let event = match packet.msg_type {
                            VoiceMessageType::VoiceData => VoiceDtlsEvent::VoiceReceived {
                                sender: packet.sender,
                                sequence: packet.sequence,
                                timestamp: packet.timestamp,
                                payload: packet.payload,
                            },
                            VoiceMessageType::SpeakingStarted => VoiceDtlsEvent::SpeakingStarted {
                                sender: packet.sender,
                            },
                            VoiceMessageType::SpeakingStopped => VoiceDtlsEvent::SpeakingStopped {
                                sender: packet.sender,
                            },
                            VoiceMessageType::Keepalive => continue, // Ignore keepalives
                        };
                        if event_tx.send(event).is_err() {
                            return;
                        }
                    }
                    Ok(None) => {
                        // Timeout or closed, continue
                    }
                    Err(e) => {
                        // DTLS Alert messages are normal during disconnect, not errors
                        if !e.contains("Alert") {
                            let _ = event_tx.send(VoiceDtlsEvent::Error(e));
                        }
                        let _ = event_tx.send(VoiceDtlsEvent::Disconnected);
                        return;
                    }
                }
            }

            // Send keepalive periodically
            _ = keepalive_interval.tick() => {
                if let Err(e) = client.send_keepalive().await {
                    let _ = event_tx.send(VoiceDtlsEvent::Error(e));
                }
            }
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_voice_dtls_event_variants() {
        // Just verify the enum variants compile
        let _ = VoiceDtlsEvent::Connected;
        let _ = VoiceDtlsEvent::VoiceReceived {
            sender: "test".to_string(),
            sequence: 0,
            timestamp: 0,
            payload: vec![],
        };
        let _ = VoiceDtlsEvent::SpeakingStarted {
            sender: "test".to_string(),
        };
        let _ = VoiceDtlsEvent::SpeakingStopped {
            sender: "test".to_string(),
        };
        let _ = VoiceDtlsEvent::Error("test".to_string());
        let _ = VoiceDtlsEvent::Disconnected;
    }

    #[test]
    fn test_voice_dtls_command_variants() {
        // Just verify the enum variants compile
        let _ = VoiceDtlsCommand::SendVoice(vec![]);
        let _ = VoiceDtlsCommand::SendSpeakingStarted;
        let _ = VoiceDtlsCommand::SendSpeakingStopped;
        let _ = VoiceDtlsCommand::Disconnect;
    }
}
