//! Voice session types
//!
//! A VoiceSession represents a single user's participation in a voice channel
//! or user message conversation.

use std::net::{IpAddr, SocketAddr};

use crate::constants::ERR_SYSTEM_TIME_AFTER_EPOCH;
use uuid::Uuid;

/// Represents a user's active voice session
///
/// Each user can have at most one voice session per server.
/// The session tracks their authentication token, target (channel/user message),
/// and UDP endpoint for voice packets.
#[derive(Debug, Clone)]
pub struct VoiceSession {
    /// Unique token for authenticating UDP voice packets
    pub token: Uuid,
    /// Display name of the user
    pub nickname: String,
    /// Target as array: ["#channel"] for channels, ["alice", "bob"] for user messages (sorted)
    pub target: Vec<String>,
    /// Unix timestamp when the session was created (used for stale session cleanup)
    pub joined_at: i64,
    /// Client's UDP address for sending voice packets (set when first UDP packet received)
    pub udp_addr: Option<SocketAddr>,
    /// TCP session ID (for correlating with the BBS connection and permission checks)
    pub session_id: u32,
    /// Client's IP address (for validating DTLS connections)
    pub ip: IpAddr,
}

impl VoiceSession {
    /// Create a new voice session
    ///
    /// Permissions (voice_listen, voice_talk) are checked dynamically via
    /// UserManager using the session_id, not cached here. This ensures
    /// permission changes take effect immediately.
    pub fn new(nickname: String, target: Vec<String>, session_id: u32, ip: IpAddr) -> Self {
        Self {
            token: Uuid::new_v4(),
            nickname,
            target,
            joined_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect(ERR_SYSTEM_TIME_AFTER_EPOCH)
                .as_secs() as i64,
            udp_addr: None,
            session_id,
            ip,
        }
    }

    /// Check if this session is for a channel (single element starting with '#')
    pub fn is_channel(&self) -> bool {
        self.target.len() == 1 && self.target[0].starts_with('#')
    }

    /// Check if this session is for a user message (two elements, neither starting with '#')
    #[allow(dead_code)] // API completeness, complement to is_channel()
    pub fn is_user_message(&self) -> bool {
        self.target.len() == 2
    }

    /// Get the target as a string key for registry lookups
    pub fn target_key(&self) -> String {
        self.target.join(":")
    }

    /// Check if this session's target matches a given channel name (case-insensitive)
    pub fn target_matches_channel(&self, channel: &str) -> bool {
        self.is_channel()
            && self
                .target
                .first()
                .map(|t| t.to_lowercase() == channel.to_lowercase())
                .unwrap_or(false)
    }

    /// Set the UDP address when first packet is received
    pub fn set_udp_addr(&mut self, addr: SocketAddr) {
        self.udp_addr = Some(addr);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_session_generates_token() {
        let ip: std::net::IpAddr = "192.168.1.1".parse().unwrap();
        let session = VoiceSession::new("alice".to_string(), vec!["#general".to_string()], 1, ip);

        // Token should be a valid UUID v4
        assert_eq!(session.token.get_version_num(), 4);
    }

    #[test]
    fn test_is_channel() {
        let ip: std::net::IpAddr = "192.168.1.1".parse().unwrap();
        let channel_session =
            VoiceSession::new("alice".to_string(), vec!["#general".to_string()], 1, ip);
        assert!(channel_session.is_channel());
        assert!(!channel_session.is_user_message());

        let user_msg_session = VoiceSession::new(
            "alice".to_string(),
            vec!["alice".to_string(), "bob".to_string()],
            1,
            ip,
        );
        assert!(!user_msg_session.is_channel());
        assert!(user_msg_session.is_user_message());
    }

    #[test]
    fn test_target_key() {
        let ip: std::net::IpAddr = "192.168.1.1".parse().unwrap();
        let channel_session =
            VoiceSession::new("alice".to_string(), vec!["#general".to_string()], 1, ip);
        assert_eq!(channel_session.target_key(), "#general");

        let user_msg_session = VoiceSession::new(
            "alice".to_string(),
            vec!["alice".to_string(), "bob".to_string()],
            1,
            ip,
        );
        assert_eq!(user_msg_session.target_key(), "alice:bob");
    }

    #[test]
    fn test_set_udp_addr() {
        let ip: std::net::IpAddr = "192.168.1.1".parse().unwrap();
        let mut session =
            VoiceSession::new("alice".to_string(), vec!["#general".to_string()], 1, ip);

        assert!(session.udp_addr.is_none());

        let addr: SocketAddr = "192.168.1.1:12345".parse().unwrap();
        session.set_udp_addr(addr);

        assert_eq!(session.udp_addr, Some(addr));
    }

    #[test]
    fn test_joined_at_is_recent() {
        let before = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let ip: std::net::IpAddr = "192.168.1.1".parse().unwrap();
        let session = VoiceSession::new("alice".to_string(), vec!["#general".to_string()], 1, ip);

        let after = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        assert!(session.joined_at >= before);
        assert!(session.joined_at <= after);
    }
}
