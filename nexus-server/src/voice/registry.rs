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

/// Hot-path snapshot for the per-packet UDP relay: exactly the fields the
/// relay needs, so validating a packet clones two small strings instead of
/// the whole `VoiceSession` (`target` Vec) and re-derived folded keys.
pub struct VoiceRelayInfo {
    pub nickname: String,
    pub nickname_folded: String,
    pub target_key_folded: String,
    pub session_id: u32,
    pub udp_addr: Option<SocketAddr>,
}

/// Result of binding a voice session to a UDP remote address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindUdpAddrOutcome {
    /// The token exists and the address is now bound to that session.
    Bound,
    /// The token does not match any active voice session.
    TokenNotFound,
    /// The token is already bound to a different UDP socket address.
    TokenAlreadyBound,
    /// Another active voice session already owns this exact socket address.
    AddrInUse,
}

/// Notification context returned from a voice-session removal.
pub struct VoiceLeaveInfo {
    pub session: VoiceSession,
    pub self_target: String,
    /// Whether this was the last session for the nickname.
    pub should_broadcast: bool,
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
        let target_key = session.target_key_folded.clone();
        let nickname_lower = session.nickname_folded.clone();

        let mut sessions = self.sessions.write().await;
        let mut id_to_token = self.session_id_to_token.write().await;

        if id_to_token.contains_key(&session_id) {
            return None;
        }

        let mut target_to_tokens = self.target_to_tokens.write().await;
        let nickname_already_in_target =
            nickname_in_target_locked(&sessions, &target_to_tokens, &target_key, &nickname_lower);

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
        let (session, nickname_still_in_voice) = {
            let mut sessions = self.sessions.write().await;
            let mut id_to_token = self.session_id_to_token.write().await;
            let mut target_to_tokens = self.target_to_tokens.write().await;

            let session = sessions.remove(&token)?;
            id_to_token.remove(&session.session_id);
            let still =
                unindex_and_check_nickname_remains(&sessions, &mut target_to_tokens, &session);
            (session, still)
        };

        Some(Self::compute_leave_info(session, nickname_still_in_voice))
    }

    /// `None` when no session matches the TCP session id.
    pub async fn remove_by_session_id(&self, session_id: u32) -> Option<VoiceLeaveInfo> {
        let (session, nickname_still_in_voice) = {
            let mut sessions = self.sessions.write().await;
            let mut id_to_token = self.session_id_to_token.write().await;
            let mut target_to_tokens = self.target_to_tokens.write().await;

            let token = id_to_token.remove(&session_id)?;
            let session = sessions.remove(&token)?;
            let still =
                unindex_and_check_nickname_remains(&sessions, &mut target_to_tokens, &session);
            (session, still)
        };

        Some(Self::compute_leave_info(session, nickname_still_in_voice))
    }

    /// `None` when no session is bound to the UDP remote address.
    pub async fn remove_by_udp_addr(&self, addr: SocketAddr) -> Option<VoiceLeaveInfo> {
        let (session, nickname_still_in_voice) = {
            let mut sessions = self.sessions.write().await;
            let mut id_to_token = self.session_id_to_token.write().await;
            let mut target_to_tokens = self.target_to_tokens.write().await;

            let token = sessions
                .iter()
                .find_map(|(token, session)| (session.udp_addr == Some(addr)).then_some(*token))?;

            let session = sessions.remove(&token)?;
            id_to_token.remove(&session.session_id);
            let still =
                unindex_and_check_nickname_remains(&sessions, &mut target_to_tokens, &session);
            (session, still)
        };

        Some(Self::compute_leave_info(session, nickname_still_in_voice))
    }

    /// Shared by every disconnect path so the broadcast-target /
    /// should-broadcast logic isn't duplicated.
    fn compute_leave_info(session: VoiceSession, nickname_still_in_voice: bool) -> VoiceLeaveInfo {
        let is_channel = session.is_channel();

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

        let (should_broadcast, broadcast_target) = if nickname_still_in_voice {
            (false, String::new())
        } else {
            let target = if is_channel {
                session.target.first().cloned().unwrap_or_default()
            } else {
                // User messages: the other participant gets the leaver's nickname.
                session.nickname.clone()
            };
            (true, target)
        };

        VoiceLeaveInfo {
            session,
            self_target,
            should_broadcast,
            broadcast_target,
        }
    }

    /// Full-session lookup by token. Test-only: the production packet path
    /// uses the narrower [`Self::relay_info_by_token`].
    #[cfg(test)]
    pub async fn get_by_token(&self, token: Uuid) -> Option<VoiceSession> {
        let sessions = self.sessions.read().await;
        sessions.get(&token).cloned()
    }

    /// Per-packet relay lookup; see [`VoiceRelayInfo`]. `None` if the token
    /// is unknown (session already left).
    pub async fn relay_info_by_token(&self, token: Uuid) -> Option<VoiceRelayInfo> {
        let sessions = self.sessions.read().await;
        sessions.get(&token).map(|s| VoiceRelayInfo {
            nickname: s.nickname.clone(),
            nickname_folded: s.nickname_folded.clone(),
            target_key_folded: s.target_key_folded.clone(),
            session_id: s.session_id,
            udp_addr: s.udp_addr,
        })
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

    /// UDP addresses of the relay recipients for the **pre-folded** target
    /// key: every session with a bound `udp_addr` whose folded nickname
    /// differs from `exclude_folded` (the sender, so it isn't echoed its own
    /// audio). Both keys come from the caches on `VoiceSession` /
    /// [`VoiceRelayInfo`], so the per-packet path allocates nothing here —
    /// only the address Vec is returned, never full session clones.
    pub(super) async fn relay_recipients(
        &self,
        target_key_folded: &str,
        exclude_folded: &str,
    ) -> Vec<SocketAddr> {
        let sessions = self.sessions.read().await;
        let target_to_tokens = self.target_to_tokens.read().await;

        target_to_tokens
            .get(target_key_folded)
            .into_iter()
            .flatten()
            .filter_map(|token| {
                let session = sessions.get(token)?;
                // Never echo back to the sender (same folded nickname).
                if session.nickname_folded == exclude_folded {
                    return None;
                }
                session.udp_addr
            })
            .collect()
    }

    /// Bind a token-authenticated voice session to a UDP remote address.
    ///
    /// The key is the full socket address (IP + port), so multiple users behind
    /// the same NAT remain distinct. A single socket address cannot belong to
    /// two active sessions at once; allowing that would make relay and
    /// disconnect cleanup ambiguous.
    pub async fn bind_udp_addr(&self, token: Uuid, addr: SocketAddr) -> BindUdpAddrOutcome {
        let mut sessions = self.sessions.write().await;
        let Some(current_addr) = sessions.get(&token).map(|session| session.udp_addr) else {
            return BindUdpAddrOutcome::TokenNotFound;
        };

        if let Some(current_addr) = current_addr {
            return if current_addr == addr {
                BindUdpAddrOutcome::Bound
            } else {
                BindUdpAddrOutcome::TokenAlreadyBound
            };
        }

        let addr_owner = sessions.iter().find_map(|(candidate_token, session)| {
            (session.udp_addr == Some(addr)).then_some(*candidate_token)
        });
        if addr_owner.is_some_and(|owner| owner != token) {
            return BindUdpAddrOutcome::AddrInUse;
        }

        sessions
            .get_mut(&token)
            .expect("token existence checked above")
            .set_udp_addr(addr);
        BindUdpAddrOutcome::Bound
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
        let new_folded = fold_name(new_nickname);
        let mut sessions = self.sessions.write().await;
        let mut target_to_tokens = self.target_to_tokens.write().await;

        for session in sessions.values_mut() {
            if session.nickname_folded == old_lower {
                session.nickname = new_nickname.to_string();
                session.nickname_folded = new_folded.clone();
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
                session.target_key_folded = fold_name(&session.target.join(":"));
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
    session.target_key_folded.clone()
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

/// Whether any session in the `target_key_lower` bucket carries
/// `nickname_lower`. Both keys must already be folded (`fold_name`). Runs on
/// already-locked maps so join (`add`) and the leave paths can compute the
/// broadcast decision atomically inside their critical section.
fn nickname_in_target_locked(
    sessions: &HashMap<Uuid, VoiceSession>,
    target_to_tokens: &HashMap<String, Vec<Uuid>>,
    target_key_lower: &str,
    nickname_lower: &str,
) -> bool {
    target_to_tokens
        .get(target_key_lower)
        .is_some_and(|tokens| {
            tokens.iter().any(|token| {
                sessions
                    .get(token)
                    .is_some_and(|s| fold_name(&s.nickname) == nickname_lower)
            })
        })
}

/// Unindex a just-removed session and report whether its nickname is still
/// present in that target via another session — the leave-broadcast decision,
/// computed while the caller still holds the `sessions` / `target_to_tokens`
/// write locks so it's atomic with the removal (mirrors `add`).
fn unindex_and_check_nickname_remains(
    sessions: &HashMap<Uuid, VoiceSession>,
    target_to_tokens: &mut HashMap<String, Vec<Uuid>>,
    session: &VoiceSession,
) -> bool {
    remove_from_target_index(target_to_tokens, session);
    nickname_in_target_locked(
        sessions,
        target_to_tokens,
        &target_index_key(session),
        &fold_name(&session.nickname),
    )
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
    async fn update_nickname_maintains_folded_caches() {
        let registry = VoiceRegistry::new();
        let alice = create_test_session("Alice", "Alice:bob", 1);
        let alice_token = alice.token;
        let bob = create_test_session("bob", "Alice:bob", 2);
        let bob_token = bob.token;
        registry.add(alice).await.expect("add alice");
        registry.add(bob).await.expect("add bob");

        assert_eq!(
            registry
                .bind_udp_addr(alice_token, "10.0.0.1:1000".parse().unwrap())
                .await,
            BindUdpAddrOutcome::Bound
        );
        assert_eq!(
            registry
                .bind_udp_addr(bob_token, "10.0.0.2:2000".parse().unwrap())
                .await,
            BindUdpAddrOutcome::Bound
        );

        registry.update_nickname("alice", "БОРИС").await;

        let renamed = registry
            .get_by_token(alice_token)
            .await
            .expect("alice session");
        assert_eq!(renamed.nickname, "БОРИС");
        assert_eq!(renamed.nickname_folded, fold_name("БОРИС"));
        assert_eq!(
            renamed.target_key_folded,
            fold_name(&renamed.target.join(":"))
        );

        // Relay routing runs entirely on the caches after the rename: the
        // renamed sender is excluded, the DM peer is still reachable.
        let recipients = registry
            .relay_recipients(&renamed.target_key_folded, &renamed.nickname_folded)
            .await;
        assert_eq!(
            recipients,
            vec!["10.0.0.2:2000".parse::<SocketAddr>().unwrap()]
        );
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
        assert_eq!(
            registry.bind_udp_addr(token, addr).await,
            BindUdpAddrOutcome::Bound
        );

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
        assert_eq!(
            registry.bind_udp_addr(first_token, first_addr).await,
            BindUdpAddrOutcome::Bound
        );
        assert_eq!(
            registry.bind_udp_addr(second_token, second_addr).await,
            BindUdpAddrOutcome::Bound
        );

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
    async fn test_relay_recipients() {
        let registry = VoiceRegistry::new();

        // alice (sender) + bob (recipient) + carol (no bound UDP addr) in
        // #general; dave in a different target.
        let alice = create_test_session("alice", "#general", 1);
        let bob = create_test_session("bob", "#general", 2);
        let carol = create_test_session("carol", "#general", 3);
        let dave = create_test_session("dave", "#other", 4);
        let (alice_token, bob_token, dave_token) = (alice.token, bob.token, dave.token);

        for session in [alice, bob, carol, dave] {
            registry
                .add(session)
                .await
                .expect("test setup: session_id is unique");
        }

        let alice_addr: SocketAddr = "10.0.0.1:1".parse().unwrap();
        let bob_addr: SocketAddr = "10.0.0.2:2".parse().unwrap();
        let dave_addr: SocketAddr = "10.0.0.4:4".parse().unwrap();
        registry.bind_udp_addr(alice_token, alice_addr).await;
        registry.bind_udp_addr(bob_token, bob_addr).await;
        registry.bind_udp_addr(dave_token, dave_addr).await;
        // carol intentionally never binds a UDP address.

        // Sender (alice) excluded by folded nickname; carol has no bound addr;
        // dave is in another target. Only bob qualifies.
        let recipients = registry
            .relay_recipients("#general", &fold_name("alice"))
            .await;
        assert_eq!(recipients, vec![bob_addr]);

        // The contract takes pre-folded keys: a caller folding an uppercase
        // target and sender resolves identically.
        let folded = registry
            .relay_recipients(&fold_name("#GENERAL"), &fold_name("ALICE"))
            .await;
        assert_eq!(folded, vec![bob_addr]);
    }

    #[tokio::test]
    async fn test_relay_recipients_excludes_same_nickname_co_sessions() {
        // A user with two sessions in one target: when one sends, neither of
        // their sessions receives the relay (folded-nickname exclusion covers
        // co-sessions). This is why exclusion is by nickname, not token/address.
        let registry = VoiceRegistry::new();

        let alice1 = create_test_session("alice", "#general", 1);
        let alice2 = create_test_session("alice", "#general", 2);
        let bob = create_test_session("bob", "#general", 3);
        let (alice1_token, alice2_token, bob_token) = (alice1.token, alice2.token, bob.token);

        for session in [alice1, alice2, bob] {
            registry
                .add(session)
                .await
                .expect("test setup: session_id is unique");
        }

        let alice1_addr: SocketAddr = "10.0.0.1:1".parse().unwrap();
        let alice2_addr: SocketAddr = "10.0.0.2:2".parse().unwrap();
        let bob_addr: SocketAddr = "10.0.0.3:3".parse().unwrap();
        registry.bind_udp_addr(alice1_token, alice1_addr).await;
        registry.bind_udp_addr(alice2_token, alice2_addr).await;
        registry.bind_udp_addr(bob_token, bob_addr).await;

        // alice sends: both of her sessions are excluded; only bob receives.
        let recipients = registry
            .relay_recipients("#general", &fold_name("alice"))
            .await;
        assert_eq!(recipients, vec![bob_addr]);
    }

    #[tokio::test]
    async fn test_bind_udp_addr() {
        let registry = VoiceRegistry::new();
        let session = create_test_session("alice", "#general", 1);
        let token = session.token;

        registry
            .add(session)
            .await
            .expect("test setup: session_id is unique");

        let addr: std::net::SocketAddr = "192.168.1.1:12345".parse().unwrap();
        assert_eq!(
            registry.bind_udp_addr(token, addr).await,
            BindUdpAddrOutcome::Bound
        );

        let updated = registry.get_by_token(token).await.unwrap();
        assert_eq!(updated.udp_addr, Some(addr));

        assert_eq!(
            registry.bind_udp_addr(token, addr).await,
            BindUdpAddrOutcome::Bound,
            "same-token rebinding to the same socket address is idempotent"
        );
        let other_addr: std::net::SocketAddr = "192.168.1.2:12345".parse().unwrap();
        assert_eq!(
            registry.bind_udp_addr(token, other_addr).await,
            BindUdpAddrOutcome::TokenAlreadyBound,
            "true source-address migration is not supported by the current DTLS demux"
        );
        let updated = registry.get_by_token(token).await.unwrap();
        assert_eq!(updated.udp_addr, Some(addr));
        assert_eq!(
            registry.bind_udp_addr(Uuid::new_v4(), addr).await,
            BindUdpAddrOutcome::TokenNotFound
        );
    }

    #[tokio::test]
    async fn test_bind_udp_addr_rejects_address_owned_by_other_session() {
        let registry = VoiceRegistry::new();
        let first = create_test_session("alice", "#general", 1);
        let first_token = first.token;
        let second = create_test_session("bob", "#general", 2);
        let second_token = second.token;
        let addr: std::net::SocketAddr = "192.168.1.1:12345".parse().unwrap();

        registry
            .add(first)
            .await
            .expect("test setup: first session_id is unique");
        registry
            .add(second)
            .await
            .expect("test setup: second session_id is unique");

        assert_eq!(
            registry.bind_udp_addr(first_token, addr).await,
            BindUdpAddrOutcome::Bound
        );
        assert_eq!(
            registry.bind_udp_addr(second_token, addr).await,
            BindUdpAddrOutcome::AddrInUse
        );

        assert_eq!(
            registry.get_by_token(first_token).await.unwrap().udp_addr,
            Some(addr)
        );
        assert_eq!(
            registry.get_by_token(second_token).await.unwrap().udp_addr,
            None
        );

        let removed = registry
            .remove_by_udp_addr(addr)
            .await
            .expect("address owner should be removed");
        assert_eq!(removed.session.session_id, 1);
        assert!(registry.get_by_session_id(2).await.is_some());
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
                .get_participants("#general")
                .await
                .iter()
                .any(|n| n == "alice"),
            "alice still in voice via her second session"
        );

        registry.remove_by_session_id(2).await;
        assert!(
            !registry
                .get_participants("#general")
                .await
                .iter()
                .any(|n| n == "alice"),
            "alice fully left voice"
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
    /// `broadcast_joined = true`. A non-atomic pre-add check could race
    /// so both saw "not present" and the handler emitted duplicate
    /// `VoiceUserJoined` broadcasts.
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

    /// Symmetric to the join serialization: two concurrent same-nickname
    /// leaves from one target must yield exactly one `should_broadcast`
    /// (the last leaver). The leave decision is computed inside the
    /// removal critical section, so it can't double- or zero-broadcast.
    #[tokio::test]
    async fn test_remove_serializes_should_broadcast_for_same_nickname_target() {
        let registry = VoiceRegistry::new();

        let t1 = registry
            .add(create_test_session("alice", "#general", 1))
            .await
            .expect("session_id 1 unique")
            .token;
        let t2 = registry
            .add(create_test_session("alice", "#general", 2))
            .await
            .expect("session_id 2 unique")
            .token;

        let (r1, r2) = tokio::join!(registry.remove_by_token(t1), registry.remove_by_token(t2));
        let leave1 = r1.expect("token 1 present");
        let leave2 = r2.expect("token 2 present");
        let broadcasters = [leave1.should_broadcast, leave2.should_broadcast]
            .iter()
            .filter(|x| **x)
            .count();
        assert_eq!(
            broadcasters, 1,
            "exactly one of two concurrent same-nickname leaves broadcasts"
        );
    }

    /// Folded keys: a case-variant of the same nickname/target is treated
    /// as already present, so the second join doesn't re-broadcast.
    #[tokio::test]
    async fn test_add_nickname_match_is_case_insensitive() {
        let registry = VoiceRegistry::new();

        registry
            .add(create_test_session("Alice", "#General", 1))
            .await
            .expect("session_id 1 unique");
        let second = registry
            .add(create_test_session("alice", "#general", 2))
            .await
            .expect("session_id 2 unique");

        assert!(
            !second.broadcast_joined,
            "a case-variant of an existing nickname in the same target is not a new joiner"
        );
    }
}
