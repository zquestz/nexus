//! Channel types for multi-channel chat
//!
//! This module contains the core data structures used by the channel manager.

use std::collections::HashSet;

/// State for a single channel
#[derive(Debug, Clone)]
pub struct Channel {
    /// Channel name (e.g., "#general")
    pub name: String,
    /// Channel topic (optional)
    pub topic: Option<String>,
    /// Username who set the topic
    pub topic_set_by: Option<String>,
    /// Whether the channel is secret (hidden from non-members)
    pub secret: bool,
    /// Session IDs of members in this channel
    pub members: HashSet<u32>,
}

impl Channel {
    /// Create a new channel with the given name
    pub fn new(name: String) -> Self {
        Self {
            name,
            topic: None,
            topic_set_by: None,
            secret: false,
            members: HashSet::new(),
        }
    }

    /// Create a channel with settings
    pub fn with_settings(
        name: String,
        topic: Option<String>,
        topic_set_by: Option<String>,
        secret: bool,
    ) -> Self {
        Self {
            name,
            topic,
            topic_set_by,
            secret,
            members: HashSet::new(),
        }
    }

    /// Check if the channel is empty
    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    /// Check if a session is a member of this channel
    pub fn has_member(&self, session_id: u32) -> bool {
        self.members.contains(&session_id)
    }

    /// Add a member to the channel
    pub fn add_member(&mut self, session_id: u32) -> bool {
        self.members.insert(session_id)
    }

    /// Remove a member from the channel
    pub fn remove_member(&mut self, session_id: u32) -> bool {
        self.members.remove(&session_id)
    }
}

/// Error when joining a channel fails
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JoinError {
    /// User is already a member of too many channels
    TooManyChannels,
}

/// Result of joining a channel
#[derive(Debug)]
pub struct JoinResult {
    /// Whether the user was already a member
    pub already_member: bool,
    /// Current channel topic
    pub topic: Option<String>,
    /// Who set the topic
    pub topic_set_by: Option<String>,
    /// Whether the channel is secret
    pub secret: bool,
    /// Current member session IDs (for looking up nicknames)
    pub member_session_ids: Vec<u32>,
}

/// Result of leaving a channel
#[derive(Debug)]
pub struct LeaveResult {
    /// Remaining member session IDs (for broadcasting leave)
    pub remaining_member_session_ids: Vec<u32>,
}

/// Info about a channel for listing
#[derive(Debug, Clone)]
pub struct ChannelListInfo {
    pub name: String,
    pub topic: Option<String>,
    pub member_count: u32,
    pub secret: bool,
}
