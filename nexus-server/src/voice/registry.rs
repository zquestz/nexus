//! In-memory registry of active voice sessions on the server.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use nexus_common::names::fold_name;
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
    /// Folded target key → tokens for sessions in that target. Writers update
    /// this while holding `sessions`, so readers lock `sessions` before this.
    target_to_tokens: Arc<RwLock<HashMap<String, Vec<Uuid>>>>,
}

impl VoiceRegistry {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            session_id_to_token: Arc::new(RwLock::new(HashMap::new())),
            target_to_tokens: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Atomically register a voice session. Returns `None` if the
    /// `session_id` is already registered.
    #[must_use]
    pub async fn add(&self, session: VoiceSession) -> Option<AddOutcome> {
        let token = session.token;
        let session_id = session.session_id;
        let target_key = target_index_key(&session);
        let nickname_lower = fold_name(&session.nickname);

        let mut sessions = self.sessions.write().await;
        let mut id_to_token = self.session_id_to_token.write().await;

        if id_to_token.contains_key(&session_id) {
            return None;
        }

        let mut target_to_tokens = self.target_to_tokens.write().await;
        let nickname_already_in_target = target_to_tokens.get(&target_key).is_some_and(|tokens| {
            tokens.iter().any(|token| {
                sessions
                    .get(token)
                    .is_some_and(|s| fold_name(&s.nickname) == nickname_lower)
            })
        });

        sessions.insert(token, session);
        id_to_token.insert(session_id, token);
        target_to_tokens.entry(target_key).or_default().push(token);

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
            let mut target_to_tokens = self.target_to_tokens.write().await;

            if let Some(session) = sessions.remove(&token) {
                id_to_token.remove(&session.session_id);
                remove_from_target_index(&mut target_to_tokens, &session);
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
            let mut target_to_tokens = self.target_to_tokens.write().await;

            if let Some(token) = id_to_token.remove(&session_id)
                && let Some(session) = sessions.remove(&token)
            {
                remove_from_target_index(&mut target_to_tokens, &session);
                session
            } else {
                return None;
            }
        };

        Some(self.compute_leave_info(session).await)
    }

    /// `None` when no session is bound to the UDP remote address.
    pub async fn remove_by_udp_addr(&self, addr: SocketAddr) -> Option<VoiceLeaveInfo> {
        let session = {
            let mut sessions = self.sessions.write().await;
            let mut id_to_token = self.session_id_to_token.write().await;
            let mut target_to_tokens = self.target_to_tokens.write().await;

            let token = sessions
                .iter()
                .find_map(|(token, session)| (session.udp_addr == Some(addr)).then_some(*token))?;

            let session = sessions.remove(&token)?;

            id_to_token.remove(&session.session_id);
            remove_from_target_index(&mut target_to_tokens, &session);
            session
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
            // User messages: report the other user's nickname.
            session
                .target
                .iter()
                .find(|n| fold_name(n) != fold_name(&session.nickname))
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
                    // User messages: remaining participants get the leaver's nickname.
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
        // (`add`, `remove_by_*`). Reversing would create an AB-BA
        // deadlock under contention.
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
        let target_to_tokens = self.target_to_tokens.read().await;
        let target_key_lower = fold_name(target_key);
        let nickname_lower = fold_name(nickname);

        target_to_tokens
            .get(&target_key_lower)
            .is_some_and(|tokens| {
                tokens.iter().any(|token| {
                    sessions.get(token).is_some_and(|s| {
                        fold_name(&s.nickname) == nickname_lower
                            && exclude_session_id != Some(s.session_id)
                    })
                })
            })
    }

    /// Nicknames currently in voice for the given target.
    pub async fn get_participants(&self, target_key: &str) -> Vec<String> {
        let sessions = self.sessions.read().await;
        let target_to_tokens = self.target_to_tokens.read().await;
        let target_key_lower = fold_name(target_key);

        target_to_tokens
            .get(&target_key_lower)
            .into_iter()
            .flatten()
            .filter_map(|token| sessions.get(token).map(|s| s.nickname.clone()))
            .collect()
    }

    /// Cloned sessions for the target (for broadcasting voice events).
    pub async fn get_sessions_for_target(&self, target_key: &str) -> Vec<VoiceSession> {
        let sessions = self.sessions.read().await;
        let target_to_tokens = self.target_to_tokens.read().await;
        let target_key_lower = fold_name(target_key);

        target_to_tokens
            .get(&target_key_lower)
            .into_iter()
            .flatten()
            .filter_map(|token| sessions.get(token).cloned())
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

    /// Apply a nickname change across all voice state in one write-lock pass:
    /// every session whose nickname matches `old_nickname` takes `new_nickname`,
    /// and any DM `target` containing the old nickname is re-keyed and re-sorted so
    /// the canonical `target_key` matches what a fresh join would build (both the
    /// renamed user's own DM session and the peer's, so an ongoing call and any
    /// later rejoin still line up). Channel targets (`#name`) never match. The
    /// voice registry only stores nicknames; the caller passes the username, which
    /// equals the nickname for the regular accounts this is gated to (shared
    /// accounts keep their per-session nicknames, so it's never called for them).
    pub async fn update_nickname(&self, old_nickname: &str, new_nickname: &str) {
        let old_lower = fold_name(old_nickname);
        let mut sessions = self.sessions.write().await;
        let mut target_to_tokens = self.target_to_tokens.write().await;

        for session in sessions.values_mut() {
            if fold_name(&session.nickname) == old_lower {
                session.nickname = new_nickname.to_string();
            }
            let mut target_changed = false;
            for entry in &mut session.target {
                if fold_name(entry) == old_lower {
                    *entry = new_nickname.to_string();
                    target_changed = true;
                }
            }
            if target_changed {
                session.target.sort_by_key(|a| fold_name(a));
            }
        }

        *target_to_tokens = build_target_index(&sessions);
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

fn target_index_key(session: &VoiceSession) -> String {
    fold_name(&session.target_key())
}

fn build_target_index(sessions: &HashMap<Uuid, VoiceSession>) -> HashMap<String, Vec<Uuid>> {
    let mut target_to_tokens: HashMap<String, Vec<Uuid>> = HashMap::new();
    for (token, session) in sessions {
        target_to_tokens
            .entry(target_index_key(session))
            .or_default()
            .push(*token);
    }
    target_to_tokens
}

fn remove_from_target_index(
    target_to_tokens: &mut HashMap<String, Vec<Uuid>>,
    session: &VoiceSession,
) {
    let target_key = target_index_key(session);
    if let Some(tokens) = target_to_tokens.get_mut(&target_key) {
        tokens.retain(|token| *token != session.token);
        if tokens.is_empty() {
            target_to_tokens.remove(&target_key);
        }
    }
}

impl Default for VoiceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn session_count(registry: &VoiceRegistry) -> usize {
        registry.sessions.read().await.len()
    }

    async fn indexed_token_count(registry: &VoiceRegistry, target_key: &str) -> usize {
        registry
            .target_to_tokens
            .read()
            .await
            .get(&fold_name(target_key))
            .map_or(0, |tokens| tokens.len())
    }

    fn create_test_session(nickname: &str, target: &str, session_id: u32) -> VoiceSession {
        // `#name` → channel; `a:b` → user-message key; bare name → pair with nickname.
        let target_vec = if target.starts_with('#') {
            vec![target.to_string()]
        } else if target.contains(':') {
            target.split(':').map(|s| s.to_string()).collect()
        } else {
            vec![nickname.to_string(), target.to_string()]
        };
        VoiceSession::new(nickname.to_string(), target_vec, session_id)
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
    async fn test_update_nickname() {
        let registry = VoiceRegistry::new();
        // The renamed user appears twice: in a channel and in a DM with bob.
        // carol is an unrelated channel member who must stay untouched.
        registry
            .add(create_test_session("alice", "#general", 1))
            .await
            .expect("test setup: session_id is unique");
        registry
            .add(create_test_session("alice", "alice:bob", 2))
            .await
            .expect("test setup: session_id is unique");
        registry
            .add(create_test_session("bob", "alice:bob", 3))
            .await
            .expect("test setup: session_id is unique");
        registry
            .add(create_test_session("carol", "#general", 4))
            .await
            .expect("test setup: session_id is unique");

        // "zara" sorts after "bob", so the DM canonical key must re-sort to
        // "bob:zara" (not the stale "zara:bob") on both peers.
        registry.update_nickname("alice", "zara").await;

        let in_channel = registry.get_by_session_id(1).await.expect("session 1");
        let in_dm = registry.get_by_session_id(2).await.expect("session 2");
        let peer = registry.get_by_session_id(3).await.expect("session 3");
        let unrelated = registry.get_by_session_id(4).await.expect("session 4");

        // Both of the renamed user's sessions take the new nickname.
        assert_eq!(in_channel.nickname, "zara");
        assert_eq!(in_dm.nickname, "zara");
        // Channel target carries no nickname and is untouched.
        assert_eq!(in_channel.target_key(), "#general");
        // The DM key re-keys + re-sorts consistently on both peers.
        assert_eq!(in_dm.target_key(), "bob:zara");
        assert_eq!(peer.target_key(), "bob:zara");
        assert_eq!(peer.nickname, "bob", "the peer is not the one renamed");
        // Unrelated session fully untouched.
        assert_eq!(unrelated.nickname, "carol");
        assert_eq!(unrelated.target_key(), "#general");

        // Participant listing reflects the new nickname, not the old.
        let participants = registry.get_participants("#general").await;
        assert!(participants.contains(&"zara".to_string()));
        assert!(!participants.contains(&"alice".to_string()));

        // The target index follows the DM re-key: the old target is empty and
        // the new canonical target finds both sides of the conversation.
        assert!(registry.get_participants("alice:bob").await.is_empty());
        let dm_participants = registry.get_participants("bob:zara").await;
        assert_eq!(dm_participants.len(), 2);
        assert!(dm_participants.contains(&"zara".to_string()));
        assert!(dm_participants.contains(&"bob".to_string()));
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
    async fn test_remove_by_udp_addr() {
        let registry = VoiceRegistry::new();
        let session = create_test_session("alice", "#general", 1);
        let token = session.token;
        let addr: std::net::SocketAddr = "192.168.1.1:12345".parse().unwrap();

        registry
            .add(session)
            .await
            .expect("test setup: session_id is unique");
        assert!(registry.set_udp_addr(token, addr).await);

        let removed = registry.remove_by_udp_addr(addr).await;
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().session.nickname, "alice");

        assert!(registry.get_by_token(token).await.is_none());
        assert!(registry.get_by_session_id(1).await.is_none());
        assert!(registry.remove_by_udp_addr(addr).await.is_none());
    }

    #[tokio::test]
    async fn test_remove_by_udp_addr_broadcasts_only_for_last_nickname_session() {
        let registry = VoiceRegistry::new();
        let first = create_test_session("alice", "#general", 1);
        let first_token = first.token;
        let second = create_test_session("alice", "#general", 2);
        let second_token = second.token;
        let first_addr: std::net::SocketAddr = "192.168.1.1:12345".parse().unwrap();
        let second_addr: std::net::SocketAddr = "192.168.1.2:12345".parse().unwrap();

        registry
            .add(first)
            .await
            .expect("test setup: session_id 1 is unique");
        registry
            .add(second)
            .await
            .expect("test setup: session_id 2 is unique");
        assert!(registry.set_udp_addr(first_token, first_addr).await);
        assert!(registry.set_udp_addr(second_token, second_addr).await);

        let first_removed = registry
            .remove_by_udp_addr(first_addr)
            .await
            .expect("first session removed");
        assert!(
            !first_removed.should_broadcast,
            "another alice session is still in the same voice target"
        );
        assert!(registry.get_by_session_id(2).await.is_some());

        let second_removed = registry
            .remove_by_udp_addr(second_addr)
            .await
            .expect("second session removed");
        assert!(
            second_removed.should_broadcast,
            "last alice session leaving should notify peers"
        );
        assert!(registry.get_by_session_id(2).await.is_none());
    }

    #[tokio::test]
    async fn test_target_index_removes_empty_targets() {
        let registry = VoiceRegistry::new();

        let alice = create_test_session("alice", "#general", 1);
        let bob = create_test_session("bob", "#general", 2);
        let charlie = create_test_session("charlie", "#other", 3);
        let charlie_token = charlie.token;

        registry
            .add(alice)
            .await
            .expect("test setup: session_id is unique");
        registry
            .add(bob)
            .await
            .expect("test setup: session_id is unique");
        registry
            .add(charlie)
            .await
            .expect("test setup: session_id is unique");

        assert_eq!(indexed_token_count(&registry, "#general").await, 2);
        assert_eq!(indexed_token_count(&registry, "#other").await, 1);

        registry.remove_by_token(charlie_token).await;
        assert_eq!(indexed_token_count(&registry, "#other").await, 0);

        registry.remove_by_session_id(1).await;
        assert_eq!(indexed_token_count(&registry, "#general").await, 1);

        registry.remove_by_session_id(2).await;
        assert_eq!(indexed_token_count(&registry, "#general").await, 0);
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

        assert_eq!(session_count(&registry).await, 0);

        registry
            .add(create_test_session("alice", "#general", 1))
            .await
            .expect("test setup: session_id is unique");
        assert_eq!(session_count(&registry).await, 1);

        registry
            .add(create_test_session("bob", "#general", 2))
            .await
            .expect("test setup: session_id is unique");
        assert_eq!(session_count(&registry).await, 2);

        registry.remove_by_session_id(1).await;
        assert_eq!(session_count(&registry).await, 1);
    }

    #[tokio::test]
    async fn test_user_message_voice_session() {
        let registry = VoiceRegistry::new();

        // Both users share the canonical sorted target ["alice", "bob"].
        let alice_session = VoiceSession::new(
            "alice".to_string(),
            vec!["alice".to_string(), "bob".to_string()],
            1,
        );
        let bob_session = VoiceSession::new(
            "bob".to_string(),
            vec!["alice".to_string(), "bob".to_string()],
            2,
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
        assert_eq!(session_count(&registry).await, 0);
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
        assert_eq!(session_count(&registry).await, 1);
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
