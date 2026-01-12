//! Channel state for multi-channel chat support

use super::ChatMessage;

/// State for a single chat channel
#[derive(Debug, Clone)]
pub struct ChannelState {
    /// Channel topic (None if no topic set)
    pub topic: Option<String>,
    /// Username who set the topic
    pub topic_set_by: Option<String>,
    /// Whether the channel is secret (hidden from /channels list for non-members)
    pub secret: bool,
    /// Nicknames of channel members (sorted alphabetically)
    pub members: Vec<String>,
    /// Chat history for this channel
    pub messages: Vec<ChatMessage>,
}

impl ChannelState {
    /// Create a new channel state with the given details
    pub fn new(
        topic: Option<String>,
        topic_set_by: Option<String>,
        secret: bool,
        members: Vec<String>,
    ) -> Self {
        Self {
            topic,
            topic_set_by,
            secret,
            members,
            messages: Vec::new(),
        }
    }

    /// Add a member to the channel (maintains sorted order)
    pub fn add_member(&mut self, nickname: String) {
        // Check if already a member (case-insensitive)
        let nickname_lower = nickname.to_lowercase();
        if self
            .members
            .iter()
            .any(|m| m.to_lowercase() == nickname_lower)
        {
            return;
        }

        // Insert in sorted position (case-insensitive sort)
        let pos = self
            .members
            .iter()
            .position(|m| m.to_lowercase() > nickname_lower)
            .unwrap_or(self.members.len());
        self.members.insert(pos, nickname);
    }

    /// Remove a member from the channel
    pub fn remove_member(&mut self, nickname: &str) {
        let nickname_lower = nickname.to_lowercase();
        self.members.retain(|m| m.to_lowercase() != nickname_lower);
    }

    /// Check if a nickname is a member of this channel (case-insensitive)
    #[cfg(test)] // Currently only used in tests - will be used for tab completion
    pub fn is_member(&self, nickname: &str) -> bool {
        let nickname_lower = nickname.to_lowercase();
        self.members
            .iter()
            .any(|m| m.to_lowercase() == nickname_lower)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_channel() {
        let channel = ChannelState::new(
            Some("Test topic".to_string()),
            Some("admin".to_string()),
            false,
            vec!["alice".to_string(), "bob".to_string()],
        );

        assert_eq!(channel.topic, Some("Test topic".to_string()));
        assert_eq!(channel.topic_set_by, Some("admin".to_string()));
        assert!(!channel.secret);
        assert_eq!(channel.members, vec!["alice", "bob"]);
        assert!(channel.messages.is_empty());
    }

    #[test]
    fn test_add_member_sorted() {
        let mut channel = ChannelState::new(None, None, false, vec![]);

        channel.add_member("charlie".to_string());
        channel.add_member("alice".to_string());
        channel.add_member("bob".to_string());

        assert_eq!(channel.members, vec!["alice", "bob", "charlie"]);
    }

    #[test]
    fn test_add_member_no_duplicate() {
        let mut channel = ChannelState::new(None, None, false, vec![]);

        channel.add_member("alice".to_string());
        channel.add_member("Alice".to_string()); // Same name, different case
        channel.add_member("ALICE".to_string()); // Same name, different case

        assert_eq!(channel.members.len(), 1);
        assert_eq!(channel.members[0], "alice"); // First one wins
    }

    #[test]
    fn test_remove_member() {
        let mut channel = ChannelState::new(
            None,
            None,
            false,
            vec![
                "alice".to_string(),
                "bob".to_string(),
                "charlie".to_string(),
            ],
        );

        channel.remove_member("bob");
        assert_eq!(channel.members, vec!["alice", "charlie"]);

        // Case-insensitive removal
        channel.remove_member("ALICE");
        assert_eq!(channel.members, vec!["charlie"]);
    }

    #[test]
    fn test_is_member() {
        let channel = ChannelState::new(
            None,
            None,
            false,
            vec!["alice".to_string(), "bob".to_string()],
        );

        assert!(channel.is_member("alice"));
        assert!(channel.is_member("Alice")); // Case-insensitive
        assert!(channel.is_member("ALICE")); // Case-insensitive
        assert!(channel.is_member("bob"));
        assert!(!channel.is_member("charlie"));
    }
}
