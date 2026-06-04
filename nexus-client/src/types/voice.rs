//! Voice state types for tracking active voice UI state

use std::collections::HashSet;

use nexus_common::names::fold_name;
use uuid::Uuid;

/// Active voice state for UI display
///
/// Tracks the local view of a voice session for a connection, including the target
/// (channel or user message), participants, speaking indicators, and mute state.
/// This is distinct from the server's VoiceSession which tracks authentication and routing.
#[derive(Debug, Clone)]
pub struct VoiceState {
    /// Voice join token for the current accepted session, when known.
    pub token: Option<Uuid>,
    /// Whether the client has already sent VoiceLeave for this session.
    pub leave_sent: bool,
    /// Whether this client may transmit audio in the current voice session.
    pub can_transmit: bool,
    /// Whether the voice thread was started with capture/encoder resources.
    pub transmit_initialized: bool,
    /// Target channel (e.g., "#general") or other user's nickname for user message voice
    pub target: String,
    /// Nicknames of users currently in this voice session
    pub participants: Vec<String>,
    /// Nicknames of users currently speaking (lowercase for case-insensitive lookup)
    pub speaking_users: HashSet<String>,
    /// Nicknames of users muted by the local user (lowercase for case-insensitive lookup)
    /// This is client-side only - stops playing audio from these users
    pub muted_users: HashSet<String>,
}

impl VoiceState {
    /// Create a new voice state
    pub fn new(target: String, participants: Vec<String>) -> Self {
        Self {
            token: None,
            leave_sent: false,
            can_transmit: false,
            transmit_initialized: false,
            target,
            participants,
            speaking_users: HashSet::new(),
            muted_users: HashSet::new(),
        }
    }

    /// Create a new accepted voice state tied to a specific join token.
    pub fn new_with_token(
        target: String,
        participants: Vec<String>,
        token: Uuid,
        can_transmit: bool,
    ) -> Self {
        Self {
            token: Some(token),
            leave_sent: false,
            can_transmit,
            transmit_initialized: can_transmit,
            target,
            participants,
            speaking_users: HashSet::new(),
            muted_users: HashSet::new(),
        }
    }

    /// Add a participant to the session
    pub fn add_participant(&mut self, nickname: String) {
        let nickname_lower = fold_name(&nickname);
        if !self
            .participants
            .iter()
            .any(|n| fold_name(n) == nickname_lower)
        {
            self.participants.push(nickname);
            self.participants.sort_by_key(|a| fold_name(a));
        }
    }

    /// Remove a participant from the session
    pub fn remove_participant(&mut self, nickname: &str) {
        let nickname_lower = fold_name(nickname);
        // Fold both sides (matching add_participant) so a leave whose casing differs
        // from the stored entry — e.g. an order-edge "alice" against "Alice" — still
        // clears it instead of leaving a ghost participant.
        self.participants.retain(|n| fold_name(n) != nickname_lower);
        // Clear transient per-nickname state. A later shared-account session may
        // reuse the same nickname, so mute state cannot safely outlive a leave.
        self.speaking_users.remove(&nickname_lower);
        self.muted_users.remove(&nickname_lower);
    }

    /// Re-key this session's per-user state when a participant — or the DM peer —
    /// changes nickname. `target` is rewritten only when it equals `old` (so a
    /// `#channel` target is left alone); `muted_users` holds folded names, so a
    /// muted user stays muted across a rename. Speaking state is intentionally
    /// cleared because voice data and rename notices have no cross-transport
    /// ordering guarantee; fresh voice packets/events will rebuild it.
    pub fn rename_user(&mut self, old: &str, new: &str) {
        let old_lower = fold_name(old);
        let new_lower = fold_name(new);
        if fold_name(&self.target) == old_lower {
            self.target = new.to_string();
        }
        let had_old_participant = self.participants.iter().any(|n| fold_name(n) == old_lower);
        if had_old_participant {
            self.participants.retain(|n| fold_name(n) != old_lower);
            if !self.participants.iter().any(|n| fold_name(n) == new_lower) {
                self.participants.push(new.to_string());
            }
            self.participants.sort_by_key(|a| fold_name(a));
        }
        self.speaking_users.remove(&old_lower);
        if had_old_participant {
            self.speaking_users.remove(&new_lower);
        }
        let had_old_mute = self.muted_users.remove(&old_lower);
        if had_old_mute
            && had_old_participant
            && self.participants.iter().any(|n| fold_name(n) == new_lower)
        {
            self.muted_users.insert(new_lower);
        }
    }

    /// Get the number of participants
    pub fn participant_count(&self) -> usize {
        self.participants.len()
    }

    /// Mark a user as speaking
    pub fn set_speaking(&mut self, nickname: &str) {
        self.speaking_users.insert(fold_name(nickname));
    }

    /// Mark a user as not speaking
    pub fn set_not_speaking(&mut self, nickname: &str) {
        self.speaking_users.remove(&fold_name(nickname));
    }

    /// Check if a user is currently speaking
    pub fn is_speaking(&self, nickname: &str) -> bool {
        self.speaking_users.contains(&fold_name(nickname))
    }

    /// Get the number of users currently speaking
    pub fn speaking_count(&self) -> usize {
        self.speaking_users.len()
    }

    /// Mute a user (client-side, stops playing their audio)
    pub fn mute_user(&mut self, nickname: &str) {
        let key = fold_name(nickname);
        if self.participants.iter().any(|n| fold_name(n) == key) {
            self.muted_users.insert(key);
        } else {
            self.muted_users.remove(&key);
        }
    }

    /// Unmute a user
    pub fn unmute_user(&mut self, nickname: &str) {
        self.muted_users.remove(&fold_name(nickname));
    }

    /// Check if a user is muted
    pub fn is_muted(&self, nickname: &str) -> bool {
        self.muted_users.contains(&fold_name(nickname))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_voice_state_starts_without_transmit_capability() {
        let state = VoiceState::new("#general".to_string(), vec!["Alice".to_string()]);

        assert!(state.token.is_none());
        assert!(!state.can_transmit);
        assert!(!state.transmit_initialized);
    }

    #[test]
    fn accepted_voice_state_records_transmit_capability() {
        let token = Uuid::new_v4();
        let state = VoiceState::new_with_token(
            "#general".to_string(),
            vec!["Alice".to_string()],
            token,
            true,
        );

        assert_eq!(state.token, Some(token));
        assert!(state.can_transmit);
        assert!(state.transmit_initialized);
    }

    /// Regression: `remove_participant` once compared exactly while the rest of
    /// VoiceState folds, so a differently-cased leave (e.g. "alice" against a stored
    /// "Alice") left a ghost participant. Both the participant list and the speaking set
    /// must clear regardless of casing.
    #[test]
    fn remove_participant_folds_case() {
        let mut state = VoiceState::new(
            "#general".to_string(),
            vec!["Alice".to_string(), "Bob".to_string()],
        );
        state.set_speaking("Alice");

        // A differently-cased leave clears both the participant and the speaking entry.
        state.remove_participant("alice");
        assert_eq!(state.participants, vec!["Bob".to_string()]);
        assert!(!state.is_speaking("Alice"));

        // A removal that matches no one (already gone) is a no-op.
        state.remove_participant("ALICE");
        assert_eq!(state.participants, vec!["Bob".to_string()]);
    }

    #[test]
    fn remove_participant_clears_mute_state() {
        let mut state = VoiceState::new("#general".to_string(), vec!["Alice".to_string()]);
        state.mute_user("Alice");

        state.remove_participant("alice");

        assert!(!state.is_muted("Alice"));
    }

    #[test]
    fn mute_user_ignores_unknown_participant() {
        let mut state = VoiceState::new("#general".to_string(), vec!["Alice".to_string()]);
        state.muted_users.insert(fold_name("Bob"));

        state.mute_user("Bob");

        assert!(!state.is_muted("Bob"));
    }

    #[test]
    fn rename_user_rekeys_participant_and_mute_but_clears_speaking() {
        let mut state = VoiceState::new(
            "Alice".to_string(),
            vec!["Alice".to_string(), "Bob".to_string()],
        );
        state.set_speaking("Alice");
        state.set_speaking("Alicia");
        state.mute_user("Alice");

        state.rename_user("Alice", "Alicia");

        assert_eq!(state.target, "Alicia");
        assert_eq!(
            state.participants,
            vec!["Alicia".to_string(), "Bob".to_string()]
        );
        assert!(!state.is_speaking("Alice"));
        assert!(!state.is_speaking("Alicia"));
        assert!(state.is_muted("Alicia"));
    }

    #[test]
    fn rename_user_deduplicates_participant_collision() {
        let mut state = VoiceState::new(
            "#general".to_string(),
            vec!["Alice".to_string(), "Alicia".to_string()],
        );

        state.rename_user("Alice", "Alicia");

        assert_eq!(state.participants, vec!["Alicia".to_string()]);
    }

    #[test]
    fn duplicate_rename_does_not_clear_rebuilt_new_speaking() {
        let mut state = VoiceState::new("#general".to_string(), vec!["Alicia".to_string()]);
        state.set_speaking("Alicia");

        state.rename_user("Alice", "Alicia");

        assert!(state.is_speaking("Alicia"));
    }

    #[test]
    fn duplicate_rename_does_not_inherit_stale_old_mute() {
        let mut state = VoiceState::new("#general".to_string(), vec!["Alicia".to_string()]);
        state.muted_users.insert(fold_name("Alice"));

        state.rename_user("Alice", "Alicia");

        assert!(!state.is_muted("Alice"));
        assert!(!state.is_muted("Alicia"));
    }
}
