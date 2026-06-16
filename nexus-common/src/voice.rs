//! Voice packet format for UDP voice communication
//!
//! This module defines the packet format used for voice data over DTLS/UDP.
//! Voice packets are sent at ~100 packets/second with 10ms Opus frames.

use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::validators::{MAX_NICKNAME_LENGTH, MAX_USERNAME_LENGTH};

/// Maximum payload size for voice data (Opus-encoded audio)
///
/// At 96 kbps (very high quality) with 10ms frames:
/// 96000 bits/sec * 0.010 sec / 8 = 120 bytes typical
/// We allow up to 1000 bytes for flexibility and future expansion.
pub const MAX_VOICE_PAYLOAD: usize = 1000;

/// Voice packet header size (type + token + sequence + timestamp)
/// - Type: 1 byte
/// - Token: 16 bytes (UUID)
/// - Sequence: 4 bytes (u32)
/// - Timestamp: 4 bytes (u32, in samples at 48kHz)
pub const VOICE_HEADER_SIZE: usize = 1 + 16 + 4 + 4;

/// Maximum total voice packet size
pub const MAX_VOICE_PACKET_SIZE: usize = VOICE_HEADER_SIZE + MAX_VOICE_PAYLOAD;

/// Keepalive interval for voice sessions (15 seconds)
pub const VOICE_KEEPALIVE_INTERVAL_SECS: u64 = 15;

/// Timeout for voice sessions with no packets (60 seconds)
pub const VOICE_SESSION_TIMEOUT_SECS: u64 = 60;

/// Sample rate for voice audio (48kHz, required by Opus)
pub const VOICE_SAMPLE_RATE: u32 = 48000;

/// Frame duration in milliseconds (10ms, required by WebRTC audio processing)
pub const VOICE_FRAME_DURATION_MS: u32 = 10;

/// Number of samples per frame at 48kHz with 10ms frames (480 samples)
pub const VOICE_SAMPLES_PER_FRAME: u32 = VOICE_SAMPLE_RATE * VOICE_FRAME_DURATION_MS / 1000;

/// Number of audio channels for voice input (mono)
pub const VOICE_CHANNELS: u16 = 1;

/// Mono channel count
pub const MONO_CHANNELS: u16 = 1;

/// Stereo channel count
pub const STEREO_CHANNELS: u16 = 2;

/// Minimum jitter buffer size in milliseconds
pub const JITTER_BUFFER_MS: u32 = 20;

/// Voice quality presets (Opus bitrate in bits per second)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum VoiceQuality {
    /// Low quality: 16 kbps - minimal bandwidth usage
    Low = 16000,
    /// Medium quality: 32 kbps - good balance
    Medium = 32000,
    /// High quality: 64 kbps - recommended default
    #[default]
    High = 64000,
    /// Very high quality: 96 kbps - best quality
    VeryHigh = 96000,
}

impl VoiceQuality {
    /// Get the bitrate in bits per second
    pub fn bitrate(self) -> i32 {
        self as i32
    }

    /// Get all quality levels
    pub fn all() -> &'static [VoiceQuality] {
        &[
            VoiceQuality::Low,
            VoiceQuality::Medium,
            VoiceQuality::High,
            VoiceQuality::VeryHigh,
        ]
    }

    /// Get a display string for this quality level
    fn display_string(self) -> &'static str {
        match self {
            VoiceQuality::Low => "Low (16 kbps)",
            VoiceQuality::Medium => "Medium (32 kbps)",
            VoiceQuality::High => "High (64 kbps)",
            VoiceQuality::VeryHigh => "Very High (96 kbps)",
        }
    }
}

impl fmt::Display for VoiceQuality {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_string())
    }
}

/// Message types for voice UDP packets
///
/// Uses a single byte for type identification, allowing 256 possible types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum VoiceMessageType {
    /// Voice audio data (Opus-encoded)
    VoiceData = 0x01,
    /// Keepalive packet (no payload)
    Keepalive = 0x02,
    /// User started speaking (for UI indicators)
    SpeakingStarted = 0x03,
    /// User stopped speaking (for UI indicators)
    SpeakingStopped = 0x04,
}

impl VoiceMessageType {
    /// Convert from byte value
    pub fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0x01 => Some(VoiceMessageType::VoiceData),
            0x02 => Some(VoiceMessageType::Keepalive),
            0x03 => Some(VoiceMessageType::SpeakingStarted),
            0x04 => Some(VoiceMessageType::SpeakingStopped),
            _ => None,
        }
    }

    /// Convert to byte value
    pub fn to_byte(self) -> u8 {
        self as u8
    }
}

/// Voice packet sent over UDP/DTLS (client → server)
///
/// Wire format (binary, big-endian):
/// ```text
/// +----------------+----------------+----------------+----------------+
/// |     Type (1)   |                  Token (16 bytes)                |
/// +----------------+----------------+----------------+----------------+
/// |  Token cont'd  |           Sequence Number (4 bytes)              |
/// +----------------+----------------+----------------+----------------+
/// |                         Timestamp (4 bytes)                       |
/// +----------------+----------------+----------------+----------------+
/// |                      Opus Payload (variable)                      |
/// +----------------+----------------+----------------+----------------+
/// ```
#[derive(Debug, Clone)]
pub struct VoicePacket {
    /// Message type
    pub msg_type: VoiceMessageType,
    /// Authentication token (from VoiceJoinResponse)
    pub token: Uuid,
    /// Sequence number for ordering and loss detection
    pub sequence: u32,
    /// Timestamp in samples (48kHz) for synchronization
    pub timestamp: u32,
    /// Opus-encoded audio data (empty for non-audio messages)
    pub payload: Vec<u8>,
}

impl VoicePacket {
    /// Create a new voice data packet with audio
    pub fn voice_data(token: Uuid, sequence: u32, timestamp: u32, payload: Vec<u8>) -> Self {
        Self {
            msg_type: VoiceMessageType::VoiceData,
            token,
            sequence,
            timestamp,
            payload,
        }
    }

    /// Create a keepalive packet
    pub fn keepalive(token: Uuid, sequence: u32) -> Self {
        Self {
            msg_type: VoiceMessageType::Keepalive,
            token,
            sequence,
            timestamp: 0,
            payload: Vec::new(),
        }
    }

    /// Create a speaking started packet
    pub fn speaking_started(token: Uuid, sequence: u32) -> Self {
        Self {
            msg_type: VoiceMessageType::SpeakingStarted,
            token,
            sequence,
            timestamp: 0,
            payload: Vec::new(),
        }
    }

    /// Create a speaking stopped packet
    pub fn speaking_stopped(token: Uuid, sequence: u32) -> Self {
        Self {
            msg_type: VoiceMessageType::SpeakingStopped,
            token,
            sequence,
            timestamp: 0,
            payload: Vec::new(),
        }
    }

    /// Check if this is a keepalive packet
    pub fn is_keepalive(&self) -> bool {
        self.msg_type == VoiceMessageType::Keepalive
    }

    /// Serialize the packet to bytes for transmission
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(VOICE_HEADER_SIZE + self.payload.len());

        // Message type (1 byte)
        bytes.push(self.msg_type.to_byte());

        // Token (16 bytes, big-endian UUID)
        bytes.extend_from_slice(self.token.as_bytes());

        // Sequence number (4 bytes, big-endian)
        bytes.extend_from_slice(&self.sequence.to_be_bytes());

        // Timestamp (4 bytes, big-endian)
        bytes.extend_from_slice(&self.timestamp.to_be_bytes());

        // Payload
        bytes.extend_from_slice(&self.payload);

        bytes
    }

    /// Deserialize a packet from bytes
    ///
    /// Returns `None` if the packet is malformed or too short.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let packet = VoicePacketRef::from_bytes(bytes)?;

        Some(Self {
            msg_type: packet.msg_type,
            token: packet.token,
            sequence: packet.sequence,
            timestamp: packet.timestamp,
            payload: packet.payload.to_vec(),
        })
    }

    /// Get the total packet size in bytes
    pub fn size(&self) -> usize {
        VOICE_HEADER_SIZE + self.payload.len()
    }
}

/// Borrowed view of a voice packet sent over UDP/DTLS (client → server).
#[derive(Debug, Clone, Copy)]
pub struct VoicePacketRef<'a> {
    /// Message type
    pub msg_type: VoiceMessageType,
    /// Authentication token (from VoiceJoinResponse)
    pub token: Uuid,
    /// Sequence number for ordering and loss detection
    pub sequence: u32,
    /// Timestamp in samples (48kHz) for synchronization
    pub timestamp: u32,
    /// Opus-encoded audio data (empty for non-audio messages)
    pub payload: &'a [u8],
}

impl<'a> VoicePacketRef<'a> {
    /// Deserialize a borrowed packet view from bytes.
    ///
    /// Returns `None` if the packet is malformed or too short.
    pub fn from_bytes(bytes: &'a [u8]) -> Option<Self> {
        if bytes.len() < VOICE_HEADER_SIZE {
            return None;
        }

        if bytes.len() > MAX_VOICE_PACKET_SIZE {
            return None;
        }

        // Message type (1 byte)
        let msg_type = VoiceMessageType::from_byte(bytes[0])?;

        // Token (16 bytes)
        let token = Uuid::from_slice(&bytes[1..17]).ok()?;

        // Sequence number (4 bytes)
        let sequence = u32::from_be_bytes([bytes[17], bytes[18], bytes[19], bytes[20]]);

        // Timestamp (4 bytes)
        let timestamp = u32::from_be_bytes([bytes[21], bytes[22], bytes[23], bytes[24]]);

        Some(Self {
            msg_type,
            token,
            sequence,
            timestamp,
            payload: &bytes[VOICE_HEADER_SIZE..],
        })
    }

    /// Get the total packet size in bytes
    pub fn size(&self) -> usize {
        VOICE_HEADER_SIZE + self.payload.len()
    }
}

/// Packet received from a voice session, with sender info added by server (server → client)
///
/// When the server relays a voice packet to recipients, it strips the token
/// and adds the sender's nickname so recipients know who's speaking.
///
/// Wire format:
/// ```text
/// +----------------+
/// |     type (1)   |
/// +----------------+
/// | sender_len (1) |
/// +----------------+
/// |  sender (var)  |
/// +----------------+
/// |  sequence (4)  |
/// +----------------+
/// | timestamp (4)  |
/// +----------------+
/// |  payload (var) |
/// +----------------+
/// ```
#[derive(Debug, Clone)]
pub struct RelayedVoicePacket {
    /// Message type
    pub msg_type: VoiceMessageType,
    /// Nickname of the sender
    pub sender: String,
    /// Sequence number for ordering
    pub sequence: u32,
    /// Timestamp in samples
    pub timestamp: u32,
    /// Opus-encoded audio data
    pub payload: Vec<u8>,
}

impl RelayedVoicePacket {
    /// Maximum sender nickname length in bytes (UTF-8).
    ///
    /// Voice senders are validated Nexus usernames or nicknames. Those are
    /// capped in Unicode scalar values, and UTF-8 uses at most 4 bytes per
    /// scalar, so this can carry every valid sender without truncation.
    pub const MAX_SENDER_LEN: usize = if MAX_USERNAME_LENGTH >= MAX_NICKNAME_LENGTH {
        MAX_USERNAME_LENGTH * 4
    } else {
        MAX_NICKNAME_LENGTH * 4
    };

    /// Maximum encoded packet size.
    pub const MAX_PACKET_SIZE: usize = 2 + Self::MAX_SENDER_LEN + 8 + MAX_VOICE_PAYLOAD;

    /// Create a new relayed packet from a voice packet and sender nickname
    pub fn from_voice_packet(packet: &VoicePacket, sender: String) -> Self {
        Self {
            msg_type: packet.msg_type,
            sender,
            sequence: packet.sequence,
            timestamp: packet.timestamp,
            payload: packet.payload.clone(),
        }
    }

    /// Serialize a borrowed voice packet directly into the relayed wire format.
    pub fn to_bytes_from_voice_packet(packet: &VoicePacketRef<'_>, sender: &str) -> Vec<u8> {
        Self::to_bytes_from_parts(
            packet.msg_type,
            sender,
            packet.sequence,
            packet.timestamp,
            packet.payload,
        )
    }

    /// Serialize the relayed packet to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        Self::to_bytes_from_parts(
            self.msg_type,
            &self.sender,
            self.sequence,
            self.timestamp,
            &self.payload,
        )
    }

    fn to_bytes_from_parts(
        msg_type: VoiceMessageType,
        sender: &str,
        sequence: u32,
        timestamp: u32,
        payload: &[u8],
    ) -> Vec<u8> {
        let sender_bytes = sender.as_bytes();
        debug_assert!(
            sender_bytes.len() <= Self::MAX_SENDER_LEN,
            "validated voice sender names must fit in the relayed sender field"
        );
        let sender_len = sender_prefix_len(sender, Self::MAX_SENDER_LEN) as u8;

        let mut bytes = Vec::with_capacity(2 + sender_len as usize + 8 + payload.len());

        // Message type (1 byte)
        bytes.push(msg_type.to_byte());

        // Sender length (1 byte)
        bytes.push(sender_len);

        // Sender nickname (variable, up to MAX_SENDER_LEN)
        bytes.extend_from_slice(&sender_bytes[..sender_len as usize]);

        // Sequence number (4 bytes, big-endian)
        bytes.extend_from_slice(&sequence.to_be_bytes());

        // Timestamp (4 bytes, big-endian)
        bytes.extend_from_slice(&timestamp.to_be_bytes());

        // Payload
        bytes.extend_from_slice(payload);

        bytes
    }

    /// Deserialize a relayed packet from bytes
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 2 {
            return None;
        }

        if bytes.len() > Self::MAX_PACKET_SIZE {
            return None;
        }

        // Message type (1 byte)
        let msg_type = VoiceMessageType::from_byte(bytes[0])?;

        let sender_len = bytes[1] as usize;
        if sender_len > Self::MAX_SENDER_LEN {
            return None;
        }

        let min_len = 2 + sender_len + 8; // type + sender_len + sender + seq + ts

        if bytes.len() < min_len {
            return None;
        }

        if bytes.len() - min_len > MAX_VOICE_PAYLOAD {
            return None;
        }

        // Sender nickname
        let sender = std::str::from_utf8(&bytes[2..2 + sender_len])
            .ok()?
            .to_string();

        let offset = 2 + sender_len;

        // Sequence number
        let sequence = u32::from_be_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]);

        // Timestamp
        let timestamp = u32::from_be_bytes([
            bytes[offset + 4],
            bytes[offset + 5],
            bytes[offset + 6],
            bytes[offset + 7],
        ]);

        // Payload
        let payload = bytes[offset + 8..].to_vec();

        Some(Self {
            msg_type,
            sender,
            sequence,
            timestamp,
            payload,
        })
    }
}

const _: () = assert!(RelayedVoicePacket::MAX_SENDER_LEN <= u8::MAX as usize);

fn sender_prefix_len(sender: &str, max_len: usize) -> usize {
    if sender.len() <= max_len {
        return sender.len();
    }

    let mut len = max_len;
    while !sender.is_char_boundary(len) {
        len -= 1;
    }
    len
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_voice_quality_bitrate() {
        assert_eq!(VoiceQuality::Low.bitrate(), 16000);
        assert_eq!(VoiceQuality::Medium.bitrate(), 32000);
        assert_eq!(VoiceQuality::High.bitrate(), 64000);
        assert_eq!(VoiceQuality::VeryHigh.bitrate(), 96000);
    }

    #[test]
    fn test_voice_quality_default() {
        assert_eq!(VoiceQuality::default(), VoiceQuality::High);
    }

    #[test]
    fn test_voice_message_type_roundtrip() {
        for byte in 0x01..=0x04 {
            let msg_type = VoiceMessageType::from_byte(byte).expect("valid type");
            assert_eq!(msg_type.to_byte(), byte);
        }
    }

    #[test]
    fn test_voice_message_type_invalid() {
        assert!(VoiceMessageType::from_byte(0x00).is_none());
        assert!(VoiceMessageType::from_byte(0x05).is_none());
        assert!(VoiceMessageType::from_byte(0xFF).is_none());
    }

    #[test]
    fn test_voice_packet_roundtrip() {
        let token = Uuid::new_v4();
        let packet = VoicePacket::voice_data(token, 42, 12345, vec![1, 2, 3, 4, 5]);

        let bytes = packet.to_bytes();
        let decoded = VoicePacket::from_bytes(&bytes).expect("should decode");

        assert_eq!(decoded.msg_type, VoiceMessageType::VoiceData);
        assert_eq!(decoded.token, token);
        assert_eq!(decoded.sequence, 42);
        assert_eq!(decoded.timestamp, 12345);
        assert_eq!(decoded.payload, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_voice_packet_ref_borrows_payload() {
        let token = Uuid::new_v4();
        let packet = VoicePacket::voice_data(token, 42, 12345, vec![1, 2, 3, 4, 5]);

        let bytes = packet.to_bytes();
        let decoded = VoicePacketRef::from_bytes(&bytes).expect("should decode");

        assert_eq!(decoded.msg_type, VoiceMessageType::VoiceData);
        assert_eq!(decoded.token, token);
        assert_eq!(decoded.sequence, 42);
        assert_eq!(decoded.timestamp, 12345);
        assert_eq!(decoded.payload, &[1, 2, 3, 4, 5]);
        assert_eq!(
            decoded.payload.as_ptr(),
            bytes[VOICE_HEADER_SIZE..].as_ptr()
        );
    }

    #[test]
    fn test_voice_packet_keepalive() {
        let token = Uuid::new_v4();
        let packet = VoicePacket::keepalive(token, 100);

        assert!(packet.is_keepalive());
        assert!(packet.payload.is_empty());
        assert_eq!(packet.msg_type, VoiceMessageType::Keepalive);

        let bytes = packet.to_bytes();
        let decoded = VoicePacket::from_bytes(&bytes).expect("should decode");

        assert!(decoded.is_keepalive());
        assert_eq!(decoded.token, token);
        assert_eq!(decoded.sequence, 100);
    }

    #[test]
    fn test_voice_packet_speaking_started() {
        let token = Uuid::new_v4();
        let packet = VoicePacket::speaking_started(token, 50);

        assert_eq!(packet.msg_type, VoiceMessageType::SpeakingStarted);

        let bytes = packet.to_bytes();
        let decoded = VoicePacket::from_bytes(&bytes).expect("should decode");

        assert_eq!(decoded.msg_type, VoiceMessageType::SpeakingStarted);
        assert_eq!(decoded.token, token);
        assert_eq!(decoded.sequence, 50);
    }

    #[test]
    fn test_voice_packet_speaking_stopped() {
        let token = Uuid::new_v4();
        let packet = VoicePacket::speaking_stopped(token, 51);

        assert_eq!(packet.msg_type, VoiceMessageType::SpeakingStopped);

        let bytes = packet.to_bytes();
        let decoded = VoicePacket::from_bytes(&bytes).expect("should decode");

        assert_eq!(decoded.msg_type, VoiceMessageType::SpeakingStopped);
    }

    #[test]
    fn test_voice_packet_too_short() {
        let bytes = vec![0u8; VOICE_HEADER_SIZE - 1];
        assert!(VoicePacket::from_bytes(&bytes).is_none());
    }

    #[test]
    fn test_voice_packet_too_long() {
        let bytes = vec![0u8; MAX_VOICE_PACKET_SIZE + 1];
        assert!(VoicePacket::from_bytes(&bytes).is_none());
    }

    #[test]
    fn test_voice_packet_invalid_type() {
        let mut bytes = vec![0u8; VOICE_HEADER_SIZE];
        bytes[0] = 0xFF; // Invalid type
        assert!(VoicePacket::from_bytes(&bytes).is_none());
    }

    #[test]
    fn test_voice_packet_size() {
        let token = Uuid::new_v4();
        let payload = vec![0u8; 100];
        let packet = VoicePacket::voice_data(token, 0, 0, payload);

        assert_eq!(packet.size(), VOICE_HEADER_SIZE + 100);
    }

    #[test]
    fn test_relayed_packet_roundtrip() {
        let packet = RelayedVoicePacket {
            msg_type: VoiceMessageType::VoiceData,
            sender: "alice".to_string(),
            sequence: 123,
            timestamp: 48000,
            payload: vec![10, 20, 30],
        };

        let bytes = packet.to_bytes();
        let decoded = RelayedVoicePacket::from_bytes(&bytes).expect("should decode");

        assert_eq!(decoded.msg_type, VoiceMessageType::VoiceData);
        assert_eq!(decoded.sender, "alice");
        assert_eq!(decoded.sequence, 123);
        assert_eq!(decoded.timestamp, 48000);
        assert_eq!(decoded.payload, vec![10, 20, 30]);
    }

    #[test]
    fn test_relayed_packet_speaking_indicator() {
        let packet = RelayedVoicePacket {
            msg_type: VoiceMessageType::SpeakingStarted,
            sender: "bob".to_string(),
            sequence: 1,
            timestamp: 0,
            payload: vec![],
        };

        let bytes = packet.to_bytes();
        let decoded = RelayedVoicePacket::from_bytes(&bytes).expect("should decode");

        assert_eq!(decoded.msg_type, VoiceMessageType::SpeakingStarted);
        assert_eq!(decoded.sender, "bob");
    }

    #[test]
    fn test_relayed_packet_empty() {
        assert!(RelayedVoicePacket::from_bytes(&[]).is_none());
        assert!(RelayedVoicePacket::from_bytes(&[0x01]).is_none()); // Only type, no sender_len
    }

    #[test]
    fn test_relayed_packet_invalid_type() {
        let bytes = vec![0xFF, 0x00]; // Invalid type, zero sender len
        assert!(RelayedVoicePacket::from_bytes(&bytes).is_none());
    }

    #[test]
    fn test_relayed_packet_rejects_sender_over_limit() {
        let sender_len = RelayedVoicePacket::MAX_SENDER_LEN + 1;
        let mut bytes = Vec::with_capacity(2 + sender_len + 8);
        bytes.push(VoiceMessageType::VoiceData.to_byte());
        bytes.push(sender_len as u8);
        bytes.extend(std::iter::repeat_n(b'a', sender_len));
        bytes.extend_from_slice(&1_u32.to_be_bytes());
        bytes.extend_from_slice(&2_u32.to_be_bytes());

        assert!(RelayedVoicePacket::from_bytes(&bytes).is_none());
    }

    #[test]
    fn test_relayed_packet_rejects_payload_over_limit() {
        let packet = RelayedVoicePacket {
            msg_type: VoiceMessageType::VoiceData,
            sender: "alice".to_string(),
            sequence: 1,
            timestamp: 2,
            payload: vec![0; MAX_VOICE_PAYLOAD + 1],
        };

        assert!(RelayedVoicePacket::from_bytes(&packet.to_bytes()).is_none());
    }

    #[test]
    fn test_relayed_packet_accepts_max_sender_and_payload() {
        let sender = "𐐀".repeat(MAX_USERNAME_LENGTH);
        assert_eq!(sender.len(), RelayedVoicePacket::MAX_SENDER_LEN);
        let payload = vec![7; MAX_VOICE_PAYLOAD];
        let packet = RelayedVoicePacket {
            msg_type: VoiceMessageType::VoiceData,
            sender: sender.clone(),
            sequence: 1,
            timestamp: 2,
            payload: payload.clone(),
        };
        let bytes = packet.to_bytes();

        assert_eq!(bytes.len(), RelayedVoicePacket::MAX_PACKET_SIZE);
        let decoded =
            RelayedVoicePacket::from_bytes(&bytes).expect("max-size packet should decode");
        assert_eq!(decoded.sender, sender);
        assert_eq!(decoded.payload, payload);
    }

    #[test]
    fn test_relayed_packet_unicode_sender() {
        let packet = RelayedVoicePacket {
            msg_type: VoiceMessageType::VoiceData,
            sender: "用户名".to_string(), // Chinese characters
            sequence: 1,
            timestamp: 0,
            payload: vec![],
        };

        let bytes = packet.to_bytes();
        let decoded = RelayedVoicePacket::from_bytes(&bytes).expect("should decode");

        assert_eq!(decoded.sender, "用户名");
    }

    #[test]
    fn test_relayed_packet_preserves_max_length_unicode_sender() {
        let sender = "𐐀".repeat(MAX_USERNAME_LENGTH);
        assert_eq!(sender.len(), RelayedVoicePacket::MAX_SENDER_LEN);

        let packet = RelayedVoicePacket {
            msg_type: VoiceMessageType::VoiceData,
            sender: sender.clone(),
            sequence: 1,
            timestamp: 0,
            payload: vec![],
        };

        let bytes = packet.to_bytes();
        let decoded = RelayedVoicePacket::from_bytes(&bytes).expect("should decode");

        assert_eq!(decoded.sender, sender);
    }

    #[test]
    fn test_relayed_from_voice_packet() {
        let token = Uuid::new_v4();
        let voice_packet = VoicePacket::voice_data(token, 10, 20, vec![1, 2, 3]);

        let relayed = RelayedVoicePacket::from_voice_packet(&voice_packet, "sender".to_string());

        assert_eq!(relayed.msg_type, VoiceMessageType::VoiceData);
        assert_eq!(relayed.sender, "sender");
        assert_eq!(relayed.sequence, 10);
        assert_eq!(relayed.timestamp, 20);
        assert_eq!(relayed.payload, vec![1, 2, 3]);
    }

    #[test]
    fn test_relayed_bytes_from_voice_packet_ref() {
        let token = Uuid::new_v4();
        let voice_packet = VoicePacket::voice_data(token, 10, 20, vec![1, 2, 3]);
        let voice_packet_bytes = voice_packet.to_bytes();
        let voice_packet_ref =
            VoicePacketRef::from_bytes(&voice_packet_bytes).expect("should decode");

        let owned_bytes =
            RelayedVoicePacket::from_voice_packet(&voice_packet, "sender".to_string()).to_bytes();
        let borrowed_bytes =
            RelayedVoicePacket::to_bytes_from_voice_packet(&voice_packet_ref, "sender");

        assert_eq!(borrowed_bytes, owned_bytes);
    }

    #[test]
    fn test_constants() {
        // Verify frame calculations
        assert_eq!(VOICE_SAMPLES_PER_FRAME, 480); // 48000 * 10 / 1000

        // Verify header size (type + token + seq + ts = 1 + 16 + 4 + 4)
        assert_eq!(VOICE_HEADER_SIZE, 25);
    }
}
