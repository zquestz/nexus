//! A single user's participation in a voice channel or user-message conversation.

use std::{fmt, net::SocketAddr};

use nexus_common::names::fold_name;

use crate::constants::ERR_SYSTEM_TIME_BEFORE_EPOCH_CHECK_CLOCK;
use uuid::Uuid;

/// At most one per user per server (session-rules invariant).
#[derive(Clone)]
pub struct VoiceSession {
    /// Authenticates UDP voice packets.
    pub token: Uuid,
    pub nickname: String,
    /// `["#channel"]` for channels, sorted `["alice", "bob"]` for user messages.
    pub target: Vec<String>,
    /// Unix create time; used to expire sessions that never opened DTLS.
    pub joined_at: i64,
    /// Set on the first UDP packet from the client.
    pub udp_addr: Option<SocketAddr>,
    /// Correlates with the BBS connection for permission checks.
    pub session_id: u32,
}

impl fmt::Debug for VoiceSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VoiceSession")
            .field("token", &"<REDACTED>")
            .field("nickname", &self.nickname)
            .field("target", &self.target)
            .field("joined_at", &self.joined_at)
            .field("udp_addr", &self.udp_addr)
            .field("session_id", &self.session_id)
            .finish()
    }
}

impl VoiceSession {
    /// Permissions aren't cached here — they're resolved dynamically by
    /// session_id via UserManager so permission changes take effect at once.
    pub fn new(nickname: String, target: Vec<String>, session_id: u32) -> Self {
        Self {
            token: Uuid::new_v4(),
            nickname,
            target,
            joined_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect(ERR_SYSTEM_TIME_BEFORE_EPOCH_CHECK_CLOCK)
                .as_secs() as i64,
            udp_addr: None,
            session_id,
        }
    }

    /// Single element starting with `#`.
    pub fn is_channel(&self) -> bool {
        self.target.len() == 1 && self.target[0].starts_with('#')
    }

    /// Colon-joined target, the key used for registry lookups.
    pub fn target_key(&self) -> String {
        self.target.join(":")
    }

    /// Case-insensitive match against a channel name.
    pub fn target_matches_channel(&self, channel: &str) -> bool {
        self.is_channel()
            && self
                .target
                .first()
                .map(|t| fold_name(t) == fold_name(channel))
                .unwrap_or(false)
    }

    /// Set on the first inbound UDP packet.
    pub fn set_udp_addr(&mut self, addr: SocketAddr) {
        self.udp_addr = Some(addr);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_session_generates_token() {
        let session = VoiceSession::new("alice".to_string(), vec!["#general".to_string()], 1);

        assert_eq!(session.token.get_version_num(), 4);
    }

    #[test]
    fn test_debug_redacts_token() {
        let session = VoiceSession::new("alice".to_string(), vec!["#general".to_string()], 1);
        let token = session.token.to_string();

        let dbg = format!("{session:?}");
        assert!(!dbg.contains(&token), "{dbg}");
        assert!(dbg.contains("<REDACTED>"), "{dbg}");
        assert!(dbg.contains("alice"), "{dbg}");
    }

    #[test]
    fn test_is_channel() {
        let channel_session =
            VoiceSession::new("alice".to_string(), vec!["#general".to_string()], 1);
        assert!(channel_session.is_channel());

        let user_msg_session = VoiceSession::new(
            "alice".to_string(),
            vec!["alice".to_string(), "bob".to_string()],
            1,
        );
        assert!(!user_msg_session.is_channel());
    }

    #[test]
    fn test_target_key() {
        let channel_session =
            VoiceSession::new("alice".to_string(), vec!["#general".to_string()], 1);
        assert_eq!(channel_session.target_key(), "#general");

        let user_msg_session = VoiceSession::new(
            "alice".to_string(),
            vec!["alice".to_string(), "bob".to_string()],
            1,
        );
        assert_eq!(user_msg_session.target_key(), "alice:bob");
    }

    #[test]
    fn test_set_udp_addr() {
        let mut session = VoiceSession::new("alice".to_string(), vec!["#general".to_string()], 1);

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

        let session = VoiceSession::new("alice".to_string(), vec!["#general".to_string()], 1);

        let after = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        assert!(session.joined_at >= before);
        assert!(session.joined_at <= after);
    }
}
