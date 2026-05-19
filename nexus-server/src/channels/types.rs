//! Channel types for multi-channel chat

use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct Channel {
    pub name: String,
    pub topic: Option<String>,
    /// Username (not nickname) who set the topic.
    pub topic_set_by: Option<String>,
    /// Hidden from non-members.
    pub secret: bool,
    /// Session IDs of members.
    pub members: HashSet<u32>,
}

impl Channel {
    pub fn new(name: String) -> Self {
        Self {
            name,
            topic: None,
            topic_set_by: None,
            secret: false,
            members: HashSet::new(),
        }
    }

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

    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    pub fn has_member(&self, session_id: u32) -> bool {
        self.members.contains(&session_id)
    }

    pub fn add_member(&mut self, session_id: u32) -> bool {
        self.members.insert(session_id)
    }

    pub fn remove_member(&mut self, session_id: u32) -> bool {
        self.members.remove(&session_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JoinError {
    TooManyChannels,
    ChannelDoesNotExist,
}

/// Whether `ChannelManager::join` may create the channel when missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinPolicy {
    CreateIfMissing,
    ExistingOnly,
}

#[derive(Debug)]
pub struct JoinResult {
    pub already_member: bool,
    pub topic: Option<String>,
    pub topic_set_by: Option<String>,
    pub secret: bool,
    /// Session IDs, for looking up nicknames.
    pub member_session_ids: Vec<u32>,
}

#[derive(Debug)]
pub struct LeaveResult {
    /// Session IDs, for broadcasting the leave.
    pub remaining_member_session_ids: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct ChannelListInfo {
    pub name: String,
    pub topic: Option<String>,
    pub member_count: u32,
    pub secret: bool,
}
