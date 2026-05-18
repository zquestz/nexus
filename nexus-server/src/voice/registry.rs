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
use crate::constants::ERR_SYSTEM_TIME_BEFORE_EPOCH_CHECK_CLOCK;

/// Result of a successful `VoiceRegistry::add`.
pub struct AddOutcome {
    pub token: Uuid,
    pub broadcast_joined: bool,
}

/// Notification context returned from a voice-session removal.
pub struct VoiceLeaveInfo {
    pub session: VoiceSession,
    pub self_target: String,
    /// Whether this was the last session for the nickname.
    pub should_broadcast: bool,
    pub remaining_participants: Vec<String>,
    pub broadcast_target: String,
}

/// In-memory registry of active voice sessions (not persisted).
#[derive(Clone)]
pub struct VoiceRegistry {
    sessions: Arc<RwLock<HashMap<Uuid, VoiceSession>>>,
    session_id_to_token: Arc<RwLock<HashMap<u32, Uuid>>>,
    active_ips: Arc<RwLock<HashSet<IpAddr>>>,
}

impl VoiceRegistry {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            session_id_to_token: Arc::new(RwLock::new(HashMap::new())),
            active_ips: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Atomically register a voice session. Returns `None` if the
    /// `session_id` is already registered.
    #[must_use]
    pub async fn add(&self, session: VoiceSession) -> Option<AddOutcome> {
        let token = session.token;
        let session_id = session.session_id;
        let ip = session.ip;
        let target_key_lower = session.target_key().to_lowercase();
        let nickname_lower = session.nickname.to_lowercase();

        let mut sessions = self.sessions.write().await;
        let mut id_to_token = self.session_id_to_token.write().await;
        let mut active_ips = self.active_ips.write().await;

        if id_to_token.contains_key(&session_id) {
            return None;
        }

        let nickname_already_in_target = sessions.values().any(|s| {
            s.target_key().to_lowercase() == target_key_lower
                && s.nickname.to_lowercase() == nickname_lower
        });

        sessions.insert(token, session);
        id_to_token.insert(session_id, token);
        active_ips.insert(ip);

        Some(AddOutcome {
            token,
            broadcast_joined: !nickname_already_in_target,
        })
    }

    /// `None` when no session matches the token.
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

    /// `None` when no session matches the TCP session id.
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

    /// Shared by every disconnect path so the broadcast-target /
    /// should-broadcast logic isn't duplicated.
    async fn compute_leave_info(&self, session: VoiceSession) -> VoiceLeaveInfo {
        let target_key = session.target_key();
        let is_channel = session.is_channel();

        let nickname_still_in_voice = self
            .is_nickname_in_target(&target_key, &session.nickname, None)
            .await;

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

    pub async fn get_by_token(&self, token: Uuid) -> Option<VoiceSession> {
        let sessions = self.sessions.read().await;
        sessions.get(&token).cloned()
    }

    pub async fn get_by_session_id(&self, session_id: u32) -> Option<VoiceSession> {
        // Lock order: sessions → id_to_token, matching every writer
        // (`add`, `remove_by_*`, `update_nickname`). Reversing would
        // create an AB-BA deadlock under contention.
        let sessions = self.sessions.read().await;
        let id_to_token = self.session_id_to_token.read().await;

        id_to_token
            .get(&session_id)
            .and_then(|token| sessions.get(token).cloned())
    }

    pub async fn has_session(&self, session_id: u32) -> bool {
        let id_to_token = self.session_id_to_token.read().await;
        id_to_token.contains_key(&session_id)
    }

    /// Used to gate DTLS connections — only IPs that joined voice
    /// via TCP signaling may connect via UDP.
    pub async fn has_session_for_ip(&self, ip: IpAddr) -> bool {
        let active_ips = self.active_ips.read().await;
        active_ips.contains(&ip)
    }

    /// Used by leave paths to decide whether to broadcast — only
    /// the last session of a nickname triggers a notification.
    /// Joins now compute this inside [`Self::add`] atomically.
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

    /// Nicknames currently in voice for the given target.
    pub async fn get_participants(&self, target_key: &str) -> Vec<String> {
        let sessions = self.sessions.read().await;
        let target_lower = target_key.to_lowercase();

        sessions
            .values()
            .filter(|s| s.target_key().to_lowercase() == target_lower)
            .map(|s| s.nickname.clone())
            .collect()
    }

    /// Cloned sessions for the target (for broadcasting voice events).
    pub async fn get_sessions_for_target(&self, target_key: &str) -> Vec<VoiceSession> {
        let sessions = self.sessions.read().await;
        let target_lower = target_key.to_lowercase();

        sessions
            .values()
            .filter(|s| s.target_key().to_lowercase() == target_lower)
            .cloned()
            .collect()
    }

    /// Called when the first UDP packet arrives from a client.
    pub async fn set_udp_addr(&self, token: Uuid, addr: std::net::SocketAddr) -> bool {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(&token) {
            session.set_udp_addr(addr);
            true
        } else {
            false
        }
    }

    /// Called when a user's username changes (regular accounts only;
    /// shared accounts keep their per-session nickname).
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

    /// Tokens of sessions that signaled `VoiceJoin` but never opened
    /// a DTLS connection within the timeout — for cleanup.
    pub async fn find_stale_sessions(&self, timeout_secs: u64) -> Vec<Uuid> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect(ERR_SYSTEM_TIME_BEFORE_EPOCH_CHECK_CLOCK)
            .as_secs() as i64;

        let sessions = self.sessions.read().await;
        sessions
            .iter()
            .filter(|(_, session)| {
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

        registry
            .add(session)
            .await
            .expect("test setup: session_id is unique");

        let retrieved = registry.get_by_token(token).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().nickname, "alice");
    }

    #[tokio::test]
    async fn test_get_by_session_id() {
        let registry = VoiceRegistry::new();
        let session = create_test_session("alice", "#general", 42);

        registry
            .add(session)
            .await
            .expect("test setup: session_id is unique");

        let retrieved = registry.get_by_session_id(42).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().nickname, "alice");

        assert!(registry.get_by_session_id(999).await.is_none());
    }

    #[tokio::test]
    async fn test_remove_by_token() {
        let registry = VoiceRegistry::new();
        let session = create_test_session("alice", "#general", 1);
        let token = session.token;

        registry
            .add(session)
            .await
            .expect("test setup: session_id is unique");
        assert!(registry.has_session(1).await);

        let removed = registry.remove_by_token(token).await;
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().session.nickname, "alice");

        assert!(!registry.has_session(1).await);
        assert!(registry.get_by_token(token).await.is_none());
    }

    #[tokio::test]
    async fn test_remove_by_session_id() {
        let registry = VoiceRegistry::new();
        let session = create_test_session("alice", "#general", 1);
        let token = session.token;

        registry
            .add(session)
            .await
            .expect("test setup: session_id is unique");

        let removed = registry.remove_by_session_id(1).await;
        assert!(removed.is_some());

        assert!(registry.get_by_token(token).await.is_none());
        assert!(registry.get_by_session_id(1).await.is_none());
    }

    #[tokio::test]
    async fn test_has_session() {
        let registry = VoiceRegistry::new();

        assert!(!registry.has_session(1).await);

        let session = create_test_session("alice", "#general", 1);
        registry
            .add(session)
            .await
            .expect("test setup: session_id is unique");

        assert!(registry.has_session(1).await);
        assert!(!registry.has_session(2).await);
    }

    #[tokio::test]
    async fn test_get_participants() {
        let registry = VoiceRegistry::new();

        registry
            .add(create_test_session("alice", "#general", 1))
            .await
            .expect("test setup: session_id is unique");
        registry
            .add(create_test_session("bob", "#general", 2))
            .await
            .expect("test setup: session_id is unique");
        registry
            .add(create_test_session("charlie", "#other", 3))
            .await
            .expect("test setup: session_id is unique");

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
            .await
            .expect("test setup: session_id is unique");
        registry
            .add(create_test_session("bob", "#general", 2))
            .await
            .expect("test setup: session_id is unique");

        let participants = registry.get_participants("#GENERAL").await;
        assert_eq!(participants.len(), 2);
    }

    #[tokio::test]
    async fn test_get_sessions_for_target() {
        let registry = VoiceRegistry::new();

        registry
            .add(create_test_session("alice", "#general", 1))
            .await
            .expect("test setup: session_id is unique");
        registry
            .add(create_test_session("bob", "#general", 2))
            .await
            .expect("test setup: session_id is unique");
        registry
            .add(create_test_session("charlie", "#other", 3))
            .await
            .expect("test setup: session_id is unique");

        let sessions = registry.get_sessions_for_target("#general").await;
        assert_eq!(sessions.len(), 2);
    }

    #[tokio::test]
    async fn test_set_udp_addr() {
        let registry = VoiceRegistry::new();
        let session = create_test_session("alice", "#general", 1);
        let token = session.token;

        registry
            .add(session)
            .await
            .expect("test setup: session_id is unique");

        let addr: std::net::SocketAddr = "192.168.1.1:12345".parse().unwrap();
        assert!(registry.set_udp_addr(token, addr).await);

        let updated = registry.get_by_token(token).await.unwrap();
        assert_eq!(updated.udp_addr, Some(addr));

        assert!(!registry.set_udp_addr(Uuid::new_v4(), addr).await);
    }

    #[tokio::test]
    async fn test_session_count() {
        let registry = VoiceRegistry::new();

        assert_eq!(registry.session_count().await, 0);

        registry
            .add(create_test_session("alice", "#general", 1))
            .await
            .expect("test setup: session_id is unique");
        assert_eq!(registry.session_count().await, 1);

        registry
            .add(create_test_session("bob", "#general", 2))
            .await
            .expect("test setup: session_id is unique");
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
        registry
            .add(alice_session)
            .await
            .expect("test setup: session_id is unique");
        registry
            .add(bob_session)
            .await
            .expect("test setup: session_id is unique");

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

        assert!(
            !registry
                .is_nickname_in_target("#general", "alice", None)
                .await
        );

        registry
            .add(create_test_session("alice", "#general", 1))
            .await
            .expect("test setup: session_id is unique");

        assert!(
            registry
                .is_nickname_in_target("#general", "alice", None)
                .await
        );

        assert!(
            !registry
                .is_nickname_in_target("#other", "alice", None)
                .await
        );

        assert!(
            !registry
                .is_nickname_in_target("#general", "bob", None)
                .await
        );

        assert!(
            registry
                .is_nickname_in_target("#GENERAL", "ALICE", None)
                .await
        );
    }

    #[tokio::test]
    async fn test_is_nickname_in_target_with_exclude() {
        let registry = VoiceRegistry::new();

        registry
            .add(create_test_session("alice", "#general", 1))
            .await
            .expect("test setup: session_id is unique");

        assert!(
            registry
                .is_nickname_in_target("#general", "alice", None)
                .await
        );

        assert!(
            !registry
                .is_nickname_in_target("#general", "alice", Some(1))
                .await
        );

        registry
            .add(create_test_session("alice", "#general", 2))
            .await
            .expect("test setup: session_id is unique");

        assert!(
            registry
                .is_nickname_in_target("#general", "alice", Some(1))
                .await
        );

        assert!(
            registry
                .is_nickname_in_target("#general", "alice", Some(2))
                .await
        );

        registry.remove_by_session_id(1).await;

        assert!(
            !registry
                .is_nickname_in_target("#general", "alice", Some(2))
                .await
        );
    }

    #[tokio::test]
    async fn test_multi_session_same_nickname() {
        let registry = VoiceRegistry::new();

        registry
            .add(create_test_session("alice", "#general", 1))
            .await
            .expect("test setup: session_id is unique");
        registry
            .add(create_test_session("alice", "#general", 2))
            .await
            .expect("test setup: session_id is unique");

        let participants = registry.get_participants("#general").await;
        assert_eq!(participants.iter().filter(|n| *n == "alice").count(), 2);

        registry.remove_by_session_id(1).await;
        assert!(
            registry
                .is_nickname_in_target("#general", "alice", None)
                .await
        );

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

        registry
            .add(create_test_session("alice", "#general", 1))
            .await
            .expect("test setup: session_id is unique");

        let participants = registry.get_participants("#general").await;
        assert!(participants.contains(&"alice".to_string()));
        assert!(!participants.contains(&"alicia".to_string()));

        let updated = registry.update_nickname(1, "alicia".to_string()).await;
        assert!(updated);

        let participants = registry.get_participants("#general").await;
        assert!(!participants.contains(&"alice".to_string()));
        assert!(participants.contains(&"alicia".to_string()));
    }

    #[tokio::test]
    async fn test_update_nickname_not_in_voice() {
        let registry = VoiceRegistry::new();

        let updated = registry.update_nickname(999, "bob".to_string()).await;
        assert!(!updated);
    }

    /// Two concurrent `add` calls for the same `session_id` must
    /// produce exactly one winner. Without the atomic check both
    /// would insert into `sessions` (different tokens), then the
    /// second `id_to_token.insert` would overwrite the first,
    /// orphaning a `VoiceSession` reachable only by its token.
    #[tokio::test]
    async fn test_add_rejects_duplicate_session_id() {
        let registry = VoiceRegistry::new();

        let s1 = create_test_session("alice", "#general", 42);
        let s2 = create_test_session("alicia", "#general", 42);

        let (r1, r2) = tokio::join!(registry.add(s1), registry.add(s2));

        let winners = [r1.is_some(), r2.is_some()].iter().filter(|x| **x).count();
        assert_eq!(winners, 1, "exactly one concurrent add must win");
        assert_eq!(registry.session_count().await, 1);
    }

    /// Two concurrent same-nickname joins to the same target (with
    /// distinct `session_id`s) must yield exactly one
    /// `broadcast_joined = true`. The pre-add `is_nickname_in_target`
    /// check this replaces could race so both saw "not present" and
    /// the handler emitted duplicate `VoiceUserJoined` broadcasts.
    #[tokio::test]
    async fn test_add_serializes_broadcast_joined_for_same_nickname_target() {
        let registry = VoiceRegistry::new();

        let s1 = create_test_session("alice", "#general", 1);
        let s2 = create_test_session("alice", "#general", 2);

        let (r1, r2) = tokio::join!(registry.add(s1), registry.add(s2));

        let o1 = r1.expect("session_id 1 unique");
        let o2 = r2.expect("session_id 2 unique");
        let broadcasters = [o1.broadcast_joined, o2.broadcast_joined]
            .iter()
            .filter(|x| **x)
            .count();
        assert_eq!(
            broadcasters, 1,
            "exactly one of two concurrent same-nickname joins broadcasts"
        );
    }
}
