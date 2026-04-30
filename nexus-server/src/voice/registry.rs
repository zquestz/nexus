//! Voice registry for managing active voice sessions
//!
//! The registry tracks all active voice sessions on the server and provides
//! methods for adding, removing, and querying sessions.

use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::sync::Arc;

use tokio::sync::RwLock;
use uuid::Uuid;

use super::session::VoiceSession;
use crate::constants::ERR_SYSTEM_TIME_AFTER_EPOCH;

/// Information needed to send VoiceUserLeft notifications after removing a session.
///
/// This struct encapsulates all the computed values needed for notifications,
/// avoiding duplicated logic across different disconnect paths.
pub struct VoiceLeaveInfo {
    /// The removed voice session
    pub session: VoiceSession,
    /// Target string to send to the leaving user (channel name or other user's nickname)
    pub self_target: String,
    /// Whether to broadcast to remaining participants (true if this was the last session for this nickname)
    pub should_broadcast: bool,
    /// Remaining participant nicknames to notify (empty if should_broadcast is false)
    pub remaining_participants: Vec<String>,
    /// Target string for broadcast messages (channel name or leaving user's nickname)
    pub broadcast_target: String,
}

/// Manages all active voice sessions on the server
///
/// The registry is entirely in-memory - voice state is not persisted.
/// When the server restarts, all voice sessions are lost.
#[derive(Clone)]
pub struct VoiceRegistry {
    /// Map of voice token -> VoiceSession
    sessions: Arc<RwLock<HashMap<Uuid, VoiceSession>>>,
    /// Map of session_id -> voice token (for quick lookup by TCP session)
    session_id_to_token: Arc<RwLock<HashMap<u32, Uuid>>>,
    /// Set of IPs with active voice sessions (for O(1) UDP validation)
    active_ips: Arc<RwLock<HashSet<IpAddr>>>,
}

impl VoiceRegistry {
    /// Create a new empty voice registry
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            session_id_to_token: Arc::new(RwLock::new(HashMap::new())),
            active_ips: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Add a voice session to the registry
    ///
    /// Returns the session's token for the client to use in UDP packets.
    pub async fn add(&self, session: VoiceSession) -> Uuid {
        let token = session.token;
        let session_id = session.session_id;
        let ip = session.ip;

        let mut sessions = self.sessions.write().await;
        let mut id_to_token = self.session_id_to_token.write().await;
        let mut active_ips = self.active_ips.write().await;

        sessions.insert(token, session);
        id_to_token.insert(session_id, token);
        active_ips.insert(ip);

        token
    }

    /// Remove a voice session by its token and compute notification info.
    ///
    /// Returns `VoiceLeaveInfo` with all the data needed to send notifications,
    /// or `None` if no session was found.
    pub async fn remove_by_token(&self, token: Uuid) -> Option<VoiceLeaveInfo> {
        let session = {
            let mut sessions = self.sessions.write().await;
            let mut id_to_token = self.session_id_to_token.write().await;
            let mut active_ips = self.active_ips.write().await;

            if let Some(session) = sessions.remove(&token) {
                id_to_token.remove(&session.session_id);
                if !sessions.values().any(|s| s.ip == session.ip) {
                    active_ips.remove(&session.ip);
                }
                session
            } else {
                return None;
            }
        };

        Some(self.compute_leave_info(session).await)
    }

    /// Remove a voice session by TCP session ID and compute notification info.
    ///
    /// Returns `VoiceLeaveInfo` with all the data needed to send notifications,
    /// or `None` if no session was found.
    pub async fn remove_by_session_id(&self, session_id: u32) -> Option<VoiceLeaveInfo> {
        let session = {
            let mut sessions = self.sessions.write().await;
            let mut id_to_token = self.session_id_to_token.write().await;
            let mut active_ips = self.active_ips.write().await;

            if let Some(token) = id_to_token.remove(&session_id)
                && let Some(session) = sessions.remove(&token)
            {
                if !sessions.values().any(|s| s.ip == session.ip) {
                    active_ips.remove(&session.ip);
                }
                session
            } else {
                return None;
            }
        };

        Some(self.compute_leave_info(session).await)
    }

    /// Compute all the notification info for a voice leave event.
    ///
    /// This centralizes the logic for determining targets and whether to broadcast,
    /// avoiding duplication across different disconnect paths.
    async fn compute_leave_info(&self, session: VoiceSession) -> VoiceLeaveInfo {
        let target_key = session.target_key();
        let is_channel = session.is_channel();

        // Check if this nickname still has other sessions in voice for this target
        let nickname_still_in_voice = self
            .is_nickname_in_target(&target_key, &session.nickname, None)
            .await;

        // Compute target string for the leaving user's notification
        let self_target = if is_channel {
            session.target.first().cloned().unwrap_or_default()
        } else {
            // For user messages, send the other user's nickname
            session
                .target
                .iter()
                .find(|n| n.to_lowercase() != session.nickname.to_lowercase())
                .cloned()
                .unwrap_or_default()
        };

        // Compute broadcast info
        let (should_broadcast, remaining_participants, broadcast_target) =
            if nickname_still_in_voice {
                (false, Vec::new(), String::new())
            } else {
                let participants = self.get_participants(&target_key).await;
                let target = if is_channel {
                    session.target.first().cloned().unwrap_or_default()
                } else {
                    // For user messages, send the leaving user's nickname to remaining participants
                    session.nickname.clone()
                };
                (true, participants, target)
            };

        VoiceLeaveInfo {
            session,
            self_target,
            should_broadcast,
            remaining_participants,
            broadcast_target,
        }
    }

    /// Get a voice session by its token
    pub async fn get_by_token(&self, token: Uuid) -> Option<VoiceSession> {
        let sessions = self.sessions.read().await;
        sessions.get(&token).cloned()
    }

    /// Get a voice session by TCP session ID
    pub async fn get_by_session_id(&self, session_id: u32) -> Option<VoiceSession> {
        let id_to_token = self.session_id_to_token.read().await;
        let sessions = self.sessions.read().await;

        id_to_token
            .get(&session_id)
            .and_then(|token| sessions.get(token).cloned())
    }

    /// Check if a user (by session ID) is already in a voice session
    pub async fn has_session(&self, session_id: u32) -> bool {
        let id_to_token = self.session_id_to_token.read().await;
        id_to_token.contains_key(&session_id)
    }

    /// Check if any voice session exists for the given IP address
    ///
    /// Used to validate DTLS connections - only IPs that have joined voice
    /// via TCP signaling should be allowed to connect via UDP.
    ///
    /// This is O(1) using the cached IP set.
    pub async fn has_session_for_ip(&self, ip: IpAddr) -> bool {
        let active_ips = self.active_ips.read().await;
        active_ips.contains(&ip)
    }

    /// Check if a nickname is already present in voice for a target
    ///
    /// Used to prevent duplicate join/leave broadcasts when the same user
    /// has multiple sessions. Only broadcasts on first join / last leave.
    ///
    /// # Arguments
    /// * `target_key` - The voice target key (e.g., "#general" or "alice:bob")
    /// * `nickname` - The nickname to check for
    /// * `exclude_session_id` - Optional session ID to exclude from the check
    pub async fn is_nickname_in_target(
        &self,
        target_key: &str,
        nickname: &str,
        exclude_session_id: Option<u32>,
    ) -> bool {
        let sessions = self.sessions.read().await;
        let target_lower = target_key.to_lowercase();
        let nickname_lower = nickname.to_lowercase();

        sessions.values().any(|s| {
            s.target_key().to_lowercase() == target_lower
                && s.nickname.to_lowercase() == nickname_lower
                && exclude_session_id != Some(s.session_id)
        })
    }

    /// Get all participants in a voice target (channel or user message)
    ///
    /// Returns a list of nicknames of users in voice for the given target.
    pub async fn get_participants(&self, target_key: &str) -> Vec<String> {
        let sessions = self.sessions.read().await;
        let target_lower = target_key.to_lowercase();

        sessions
            .values()
            .filter(|s| s.target_key().to_lowercase() == target_lower)
            .map(|s| s.nickname.clone())
            .collect()
    }

    /// Get all voice sessions for a target (channel or user message)
    ///
    /// Returns cloned sessions for broadcasting voice events.
    pub async fn get_sessions_for_target(&self, target_key: &str) -> Vec<VoiceSession> {
        let sessions = self.sessions.read().await;
        let target_lower = target_key.to_lowercase();

        sessions
            .values()
            .filter(|s| s.target_key().to_lowercase() == target_lower)
            .cloned()
            .collect()
    }

    /// Update the UDP address for a session (identified by token)
    ///
    /// Called when the first UDP packet is received from a client.
    pub async fn set_udp_addr(&self, token: Uuid, addr: std::net::SocketAddr) -> bool {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(&token) {
            session.set_udp_addr(addr);
            true
        } else {
            false
        }
    }

    /// Update the nickname for all sessions belonging to a given session_id.
    ///
    /// Called when a user's username changes (for regular accounts, nickname == username).
    /// Returns true if a session was updated.
    pub async fn update_nickname(&self, session_id: u32, new_nickname: String) -> bool {
        let mut sessions = self.sessions.write().await;
        let id_to_token = self.session_id_to_token.read().await;

        if let Some(token) = id_to_token.get(&session_id)
            && let Some(session) = sessions.get_mut(token)
        {
            session.nickname = new_nickname;
            return true;
        }
        false
    }

    /// Find sessions that never established a UDP connection and are older than the timeout.
    ///
    /// Returns tokens of stale sessions that should be cleaned up.
    /// This handles the case where a client sends VoiceJoin but fails to connect via DTLS.
    pub async fn find_stale_sessions(&self, timeout_secs: u64) -> Vec<Uuid> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect(ERR_SYSTEM_TIME_AFTER_EPOCH)
            .as_secs() as i64;

        let sessions = self.sessions.read().await;
        sessions
            .iter()
            .filter(|(_, session)| {
                // Session never got a UDP connection and is older than timeout
                session.udp_addr.is_none() && (now - session.joined_at) > timeout_secs as i64
            })
            .map(|(token, _)| *token)
            .collect()
    }
}

impl Default for VoiceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// Test-only methods
#[cfg(test)]
impl VoiceRegistry {
    /// Get the number of active voice sessions (test-only)
    pub async fn session_count(&self) -> usize {
        self.sessions.read().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_session(nickname: &str, target: &str, session_id: u32) -> VoiceSession {
        // Parse target: if it starts with #, it's a channel (single element)
        // Otherwise, assume it's a user message key like "alice:bob"
        let target_vec = if target.starts_with('#') {
            vec![target.to_string()]
        } else if target.contains(':') {
            target.split(':').map(|s| s.to_string()).collect()
        } else {
            // Single nickname - create a pair with test user
            vec![nickname.to_string(), target.to_string()]
        };
        let ip: std::net::IpAddr = "192.168.1.1".parse().unwrap();
        VoiceSession::new(nickname.to_string(), target_vec, session_id, ip)
    }

    #[tokio::test]
    async fn test_add_and_get_session() {
        let registry = VoiceRegistry::new();
        let session = create_test_session("alice", "#general", 1);
        let token = session.token;

        registry.add(session).await;

        let retrieved = registry.get_by_token(token).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().nickname, "alice");
    }

    #[tokio::test]
    async fn test_get_by_session_id() {
        let registry = VoiceRegistry::new();
        let session = create_test_session("alice", "#general", 42);

        registry.add(session).await;

        let retrieved = registry.get_by_session_id(42).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().nickname, "alice");

        // Non-existent session ID
        assert!(registry.get_by_session_id(999).await.is_none());
    }

    #[tokio::test]
    async fn test_remove_by_token() {
        let registry = VoiceRegistry::new();
        let session = create_test_session("alice", "#general", 1);
        let token = session.token;

        registry.add(session).await;
        assert!(registry.has_session(1).await);

        let removed = registry.remove_by_token(token).await;
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().session.nickname, "alice");

        // Session should be gone
        assert!(!registry.has_session(1).await);
        assert!(registry.get_by_token(token).await.is_none());
    }

    #[tokio::test]
    async fn test_remove_by_session_id() {
        let registry = VoiceRegistry::new();
        let session = create_test_session("alice", "#general", 1);
        let token = session.token;

        registry.add(session).await;

        let removed = registry.remove_by_session_id(1).await;
        assert!(removed.is_some());

        // Both lookups should fail now
        assert!(registry.get_by_token(token).await.is_none());
        assert!(registry.get_by_session_id(1).await.is_none());
    }

    #[tokio::test]
    async fn test_has_session() {
        let registry = VoiceRegistry::new();

        assert!(!registry.has_session(1).await);

        let session = create_test_session("alice", "#general", 1);
        registry.add(session).await;

        assert!(registry.has_session(1).await);
        assert!(!registry.has_session(2).await);
    }

    #[tokio::test]
    async fn test_get_participants() {
        let registry = VoiceRegistry::new();

        // Add multiple users to the same channel
        registry
            .add(create_test_session("alice", "#general", 1))
            .await;
        registry
            .add(create_test_session("bob", "#general", 2))
            .await;
        registry
            .add(create_test_session("charlie", "#other", 3))
            .await;

        let participants = registry.get_participants("#general").await;
        assert_eq!(participants.len(), 2);
        assert!(participants.contains(&"alice".to_string()));
        assert!(participants.contains(&"bob".to_string()));
        assert!(!participants.contains(&"charlie".to_string()));
    }

    #[tokio::test]
    async fn test_get_participants_case_insensitive() {
        let registry = VoiceRegistry::new();

        registry
            .add(create_test_session("alice", "#general", 1))
            .await;
        registry
            .add(create_test_session("bob", "#general", 2))
            .await;

        // Should find both regardless of case
        let participants = registry.get_participants("#GENERAL").await;
        assert_eq!(participants.len(), 2);
    }

    #[tokio::test]
    async fn test_get_sessions_for_target() {
        let registry = VoiceRegistry::new();

        registry
            .add(create_test_session("alice", "#general", 1))
            .await;
        registry
            .add(create_test_session("bob", "#general", 2))
            .await;
        registry
            .add(create_test_session("charlie", "#other", 3))
            .await;

        let sessions = registry.get_sessions_for_target("#general").await;
        assert_eq!(sessions.len(), 2);
    }

    #[tokio::test]
    async fn test_set_udp_addr() {
        let registry = VoiceRegistry::new();
        let session = create_test_session("alice", "#general", 1);
        let token = session.token;

        registry.add(session).await;

        let addr: std::net::SocketAddr = "192.168.1.1:12345".parse().unwrap();
        assert!(registry.set_udp_addr(token, addr).await);

        let updated = registry.get_by_token(token).await.unwrap();
        assert_eq!(updated.udp_addr, Some(addr));

        // Non-existent token should return false
        assert!(!registry.set_udp_addr(Uuid::new_v4(), addr).await);
    }

    #[tokio::test]
    async fn test_session_count() {
        let registry = VoiceRegistry::new();

        assert_eq!(registry.session_count().await, 0);

        registry
            .add(create_test_session("alice", "#general", 1))
            .await;
        assert_eq!(registry.session_count().await, 1);

        registry
            .add(create_test_session("bob", "#general", 2))
            .await;
        assert_eq!(registry.session_count().await, 2);

        registry.remove_by_session_id(1).await;
        assert_eq!(registry.session_count().await, 1);
    }

    #[tokio::test]
    async fn test_user_message_voice_session() {
        let registry = VoiceRegistry::new();

        // User message voice session uses canonical sorted target ["alice", "bob"]
        // Both users should end up in the same voice session
        let ip: std::net::IpAddr = "192.168.1.1".parse().unwrap();
        let alice_session = VoiceSession::new(
            "alice".to_string(),
            vec!["alice".to_string(), "bob".to_string()],
            1,
            ip,
        );
        let bob_session = VoiceSession::new(
            "bob".to_string(),
            vec!["alice".to_string(), "bob".to_string()],
            2,
            ip,
        );
        registry.add(alice_session).await;
        registry.add(bob_session).await;

        // Both should be in the same voice session
        let participants = registry.get_participants("alice:bob").await;
        assert_eq!(participants.len(), 2);
        assert!(participants.contains(&"alice".to_string()));
        assert!(participants.contains(&"bob".to_string()));
    }

    #[tokio::test]
    async fn test_default() {
        let registry = VoiceRegistry::default();
        assert_eq!(registry.session_count().await, 0);
    }

    #[tokio::test]
    async fn test_is_nickname_in_target() {
        let registry = VoiceRegistry::new();

        // No sessions yet
        assert!(
            !registry
                .is_nickname_in_target("#general", "alice", None)
                .await
        );

        // Add alice to #general
        registry
            .add(create_test_session("alice", "#general", 1))
            .await;

        // Alice is in #general
        assert!(
            registry
                .is_nickname_in_target("#general", "alice", None)
                .await
        );

        // Alice is not in #other
        assert!(
            !registry
                .is_nickname_in_target("#other", "alice", None)
                .await
        );

        // Bob is not in #general
        assert!(
            !registry
                .is_nickname_in_target("#general", "bob", None)
                .await
        );

        // Case insensitive check
        assert!(
            registry
                .is_nickname_in_target("#GENERAL", "ALICE", None)
                .await
        );
    }

    #[tokio::test]
    async fn test_is_nickname_in_target_with_exclude() {
        let registry = VoiceRegistry::new();

        // Add alice session 1 to #general
        registry
            .add(create_test_session("alice", "#general", 1))
            .await;

        // Alice is in #general when not excluding any session
        assert!(
            registry
                .is_nickname_in_target("#general", "alice", None)
                .await
        );

        // Alice is NOT in #general when excluding session 1 (her only session)
        assert!(
            !registry
                .is_nickname_in_target("#general", "alice", Some(1))
                .await
        );

        // Add alice session 2 to #general (multi-session case)
        registry
            .add(create_test_session("alice", "#general", 2))
            .await;

        // Alice is still in #general when excluding session 1 (session 2 remains)
        assert!(
            registry
                .is_nickname_in_target("#general", "alice", Some(1))
                .await
        );

        // Alice is still in #general when excluding session 2 (session 1 remains)
        assert!(
            registry
                .is_nickname_in_target("#general", "alice", Some(2))
                .await
        );

        // Remove session 1
        registry.remove_by_session_id(1).await;

        // Alice is NOT in #general when excluding session 2 (only remaining session)
        assert!(
            !registry
                .is_nickname_in_target("#general", "alice", Some(2))
                .await
        );
    }

    #[tokio::test]
    async fn test_multi_session_same_nickname() {
        let registry = VoiceRegistry::new();

        // Simulate a regular user with two sessions joining voice
        registry
            .add(create_test_session("alice", "#general", 1))
            .await;
        registry
            .add(create_test_session("alice", "#general", 2))
            .await;

        // Participant list should contain alice (possibly twice, but that's for display)
        let participants = registry.get_participants("#general").await;
        assert_eq!(participants.iter().filter(|n| *n == "alice").count(), 2);

        // When session 1 leaves, alice is still in voice via session 2
        registry.remove_by_session_id(1).await;
        assert!(
            registry
                .is_nickname_in_target("#general", "alice", None)
                .await
        );

        // When session 2 leaves, alice is no longer in voice
        registry.remove_by_session_id(2).await;
        assert!(
            !registry
                .is_nickname_in_target("#general", "alice", None)
                .await
        );
    }

    #[tokio::test]
    async fn test_update_nickname() {
        let registry = VoiceRegistry::new();

        // Add a session for alice
        registry
            .add(create_test_session("alice", "#general", 1))
            .await;

        // Verify alice is in participants
        let participants = registry.get_participants("#general").await;
        assert!(participants.contains(&"alice".to_string()));
        assert!(!participants.contains(&"alicia".to_string()));

        // Update nickname (simulating username change)
        let updated = registry.update_nickname(1, "alicia".to_string()).await;
        assert!(updated);

        // Verify alicia is now in participants, not alice
        let participants = registry.get_participants("#general").await;
        assert!(!participants.contains(&"alice".to_string()));
        assert!(participants.contains(&"alicia".to_string()));
    }

    #[tokio::test]
    async fn test_update_nickname_not_in_voice() {
        let registry = VoiceRegistry::new();

        // Try to update nickname for a session that's not in voice
        let updated = registry.update_nickname(999, "bob".to_string()).await;
        assert!(!updated);
    }
}
