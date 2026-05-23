//! Voice state types for tracking active voice UI state

use std::collections::HashSet;

use nexus_common::names::fold_name;

/// Active voice state for UI display
///
/// Tracks the local view of a voice session for a connection, including the target
/// (channel or user message), participants, speaking indicators, and mute state.
/// This is distinct from the server's VoiceSession which tracks authentication and routing.
#[derive(Debug, Clone)]
pub struct VoiceState {
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
            target,
            participants,
            speaking_users: HashSet::new(),
            muted_users: HashSet::new(),
        }
    }

    /// Check if the target is a channel (starts with #)
    #[allow(dead_code)] // Available for UI logic
    pub fn is_channel(&self) -> bool {
        self.target.starts_with('#')
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
        self.participants.retain(|n| n != nickname);
        // Clear speaking state for the removed user (but keep muted state
        // in case they rejoin - user's mute preference should persist)
        self.speaking_users.remove(&fold_name(nickname));
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
        self.muted_users.insert(fold_name(nickname));
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
