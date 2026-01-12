//! Channel manager for tracking chat channels and their members
//!
//! Channels come in two types:
//! - **Persistent channels**: Defined in server config, auto-joined on login,
//!   survive server restart, settings stored in database.
//! - **Ephemeral channels**: Created by users via `/join`, deleted when empty,
//!   settings stored in-memory only.

use std::collections::{HashMap, HashSet};
use std::io;
use std::sync::Arc;

use tokio::sync::RwLock;

use nexus_common::validators::MAX_CHANNELS_PER_USER;

use super::types::{Channel, ChannelListInfo, JoinError, JoinResult, LeaveResult};
use crate::db::ChannelDb;

/// Manages all chat channels
#[derive(Clone)]
pub struct ChannelManager {
    /// Map of channel name (lowercase) -> Channel
    channels: Arc<RwLock<HashMap<String, Channel>>>,
    /// Set of persistent channel names (lowercase) - these are never deleted when empty
    persistent_channels: Arc<RwLock<HashSet<String>>>,
    /// Database for persisting channel settings
    db: ChannelDb,
}

impl ChannelManager {
    /// Create a new empty channel manager
    ///
    /// Use `initialize_persistent_channels` to set up persistent channels after creation.
    pub fn new(db: ChannelDb) -> Self {
        Self {
            channels: Arc::new(RwLock::new(HashMap::new())),
            persistent_channels: Arc::new(RwLock::new(HashSet::new())),
            db,
        }
    }

    /// Initialize persistent channels
    ///
    /// Creates channels for each name in the list and marks them as persistent.
    /// Called at server startup with channel names from config and settings from DB.
    pub async fn initialize_persistent_channels(&self, channels_with_settings: Vec<Channel>) {
        let mut channels = self.channels.write().await;
        let mut persistent = self.persistent_channels.write().await;

        for channel in channels_with_settings {
            let key = channel.name.to_lowercase();
            persistent.insert(key.clone());
            channels.insert(key, channel);
        }
    }

    /// Reinitialize persistent channels (called when admin changes config)
    ///
    /// This clears the old persistent channel set and initializes new ones.
    /// Channels that were persistent but are no longer will become ephemeral
    /// (and will be deleted if empty). New persistent channels are created.
    pub async fn reinitialize_persistent_channels(&self, channels_with_settings: Vec<Channel>) {
        let mut channels = self.channels.write().await;
        let mut persistent = self.persistent_channels.write().await;

        // Build new persistent set
        let new_persistent: HashSet<String> = channels_with_settings
            .iter()
            .map(|ch| ch.name.to_lowercase())
            .collect();

        // Remove channels that are no longer persistent and are empty
        let old_persistent = std::mem::take(&mut *persistent);
        let mut to_delete = Vec::new();
        for key in &old_persistent {
            if !new_persistent.contains(key)
                && let Some(ch) = channels.get(key)
                && ch.is_empty()
            {
                to_delete.push(key.clone());
            }
        }
        for key in to_delete {
            channels.remove(&key);
        }

        // Add/update persistent channels
        for channel in channels_with_settings {
            let key = channel.name.to_lowercase();
            persistent.insert(key.clone());
            // Only insert if it doesn't exist (preserve members)
            channels.entry(key).or_insert(channel);
        }
    }

    /// Check if a channel is persistent
    #[cfg(test)]
    pub async fn is_persistent(&self, channel_name: &str) -> bool {
        let key = channel_name.to_lowercase();
        let persistent = self.persistent_channels.read().await;
        persistent.contains(&key)
    }

    /// Join a channel, creating it if it doesn't exist
    ///
    /// Returns information about the join operation, or an error if the user
    /// is already a member of too many channels (`MAX_CHANNELS_PER_USER`).
    ///
    /// Note: If the user is already a member, this succeeds (returns `already_member: true`)
    /// without counting against their channel limit.
    pub async fn join(&self, channel_name: &str, session_id: u32) -> Result<JoinResult, JoinError> {
        let key = channel_name.to_lowercase();
        let mut channels = self.channels.write().await;

        // Check if already a member (doesn't count against limit)
        if let Some(ch) = channels.get(&key)
            && ch.has_member(session_id)
        {
            return Ok(JoinResult {
                already_member: true,
                topic: ch.topic.clone(),
                topic_set_by: ch.topic_set_by.clone(),
                secret: ch.secret,
                member_session_ids: ch.members.iter().copied().collect(),
            });
        }

        // Check channel limit before joining
        let current_count = channels
            .values()
            .filter(|ch| ch.has_member(session_id))
            .count();
        if current_count >= MAX_CHANNELS_PER_USER {
            return Err(JoinError::TooManyChannels);
        }

        let channel = channels
            .entry(key)
            .or_insert_with(|| Channel::new(channel_name.to_string()));

        channel.add_member(session_id);

        Ok(JoinResult {
            already_member: false,
            topic: channel.topic.clone(),
            topic_set_by: channel.topic_set_by.clone(),
            secret: channel.secret,
            member_session_ids: channel.members.iter().copied().collect(),
        })
    }

    /// Leave a channel
    ///
    /// Returns None if the user wasn't a member or the channel doesn't exist.
    /// Returns Some(LeaveResult) with info about the leave operation.
    ///
    /// Note: Persistent channels are never deleted even if empty.
    pub async fn leave(&self, channel_name: &str, session_id: u32) -> Option<LeaveResult> {
        let key = channel_name.to_lowercase();
        let mut channels = self.channels.write().await;
        let persistent = self.persistent_channels.read().await;

        let channel = channels.get_mut(&key)?;

        if !channel.remove_member(session_id) {
            // User wasn't a member
            return None;
        }

        let remaining_member_session_ids: Vec<u32> = channel.members.iter().copied().collect();

        // Delete empty channels (except persistent ones)
        let is_persistent = persistent.contains(&key);
        if channel.is_empty() && !is_persistent {
            channels.remove(&key);
        }

        Some(LeaveResult {
            remaining_member_session_ids,
        })
    }

    /// Remove a session from all channels (called on disconnect)
    ///
    /// Returns a list of channel names the user was in.
    /// Caller can use `get_members()` to get remaining members for broadcasting.
    pub async fn remove_from_all(&self, session_id: u32) -> Vec<String> {
        let mut channels = self.channels.write().await;
        let persistent = self.persistent_channels.read().await;
        let mut channel_names = Vec::new();
        let mut to_delete = Vec::new();

        for (key, channel) in channels.iter_mut() {
            if channel.remove_member(session_id) {
                channel_names.push(channel.name.clone());

                // Mark empty channels for deletion (except persistent ones)
                if channel.is_empty() && !persistent.contains(key) {
                    to_delete.push(key.clone());
                }
            }
        }

        // Delete empty channels
        for key in to_delete {
            channels.remove(&key);
        }

        channel_names
    }

    /// List channels visible to a session
    ///
    /// Secret channels are hidden unless the session is a member or is_admin is true.
    pub async fn list(&self, session_id: u32, is_admin: bool) -> Vec<ChannelListInfo> {
        let channels = self.channels.read().await;

        channels
            .values()
            .filter(|ch| {
                // Show non-secret channels, or secret channels if admin or member
                !ch.secret || is_admin || ch.has_member(session_id)
            })
            .map(|ch| ChannelListInfo {
                name: ch.name.clone(),
                topic: ch.topic.clone(),
                member_count: ch.members.len() as u32,
                secret: ch.secret,
            })
            .collect()
    }

    /// Set the secret mode for a channel
    ///
    /// Returns Ok(true) if channel exists and was updated, Ok(false) if channel doesn't exist.
    /// Returns Err on database error (only possible for persistent channels).
    pub async fn set_secret(&self, channel_name: &str, secret: bool) -> io::Result<bool> {
        let key = channel_name.to_lowercase();
        let mut channels = self.channels.write().await;

        let Some(channel) = channels.get_mut(&key) else {
            return Ok(false);
        };

        channel.secret = secret;

        // Persist to database for persistent channels
        let persistent = self.persistent_channels.read().await;
        if persistent.contains(&key) {
            drop(channels); // Release lock before async DB call
            self.db.set_secret(channel_name, secret).await?;
        }

        Ok(true)
    }

    /// Set the topic for a channel
    ///
    /// Returns Ok(true) if channel exists and was updated, Ok(false) if channel doesn't exist.
    /// Returns Err on database error (only possible for persistent channels).
    pub async fn set_topic(
        &self,
        channel_name: &str,
        topic: Option<String>,
        set_by: Option<String>,
    ) -> io::Result<bool> {
        let key = channel_name.to_lowercase();
        let mut channels = self.channels.write().await;

        let Some(channel) = channels.get_mut(&key) else {
            return Ok(false);
        };

        channel.topic = topic.clone();
        channel.topic_set_by = set_by.clone();

        // Persist to database for persistent channels
        let persistent = self.persistent_channels.read().await;
        if persistent.contains(&key) {
            drop(channels); // Release lock before async DB call
            let topic_str = topic.as_deref().unwrap_or("");
            let set_by_str = set_by.as_deref().unwrap_or("");
            self.db
                .set_topic(channel_name, topic_str, set_by_str)
                .await?;
        }

        Ok(true)
    }

    /// Check if a session is a member of a channel
    pub async fn is_member(&self, channel_name: &str, session_id: u32) -> bool {
        let key = channel_name.to_lowercase();
        let channels = self.channels.read().await;

        channels
            .get(&key)
            .is_some_and(|ch| ch.has_member(session_id))
    }

    /// Check if a channel exists
    #[cfg(test)]
    pub async fn exists(&self, channel_name: &str) -> bool {
        let key = channel_name.to_lowercase();
        let channels = self.channels.read().await;
        channels.contains_key(&key)
    }

    /// Get member session IDs for a channel
    ///
    /// Returns None if the channel doesn't exist.
    pub async fn get_members(&self, channel_name: &str) -> Option<Vec<u32>> {
        let key = channel_name.to_lowercase();
        let channels = self.channels.read().await;

        channels
            .get(&key)
            .map(|ch| ch.members.iter().copied().collect())
    }

    /// Get all channels a session is a member of
    ///
    /// If `is_admin` is true, returns all channels including secret ones.
    /// Otherwise, secret channels are excluded from the result.
    ///
    /// Returns channel names sorted alphabetically.
    pub async fn get_channels_for_session(&self, session_id: u32, is_admin: bool) -> Vec<String> {
        let channels = self.channels.read().await;

        let mut result: Vec<String> = channels
            .values()
            .filter(|ch| ch.has_member(session_id))
            .filter(|ch| is_admin || !ch.secret)
            .map(|ch| ch.name.clone())
            .collect();

        result.sort_by_key(|a| a.to_lowercase());
        result
    }

    /// Get channel info (for checking secret status, etc.)
    #[cfg(test)]
    pub async fn get_channel(&self, channel_name: &str) -> Option<Channel> {
        let key = channel_name.to_lowercase();
        let channels = self.channels.read().await;
        channels.get(&key).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::testing::create_test_db;
    use nexus_common::validators::DEFAULT_CHANNEL;

    /// Helper to create a ChannelManager with a test database
    async fn create_test_manager() -> ChannelManager {
        let pool = create_test_db().await;
        let db = ChannelDb::new(pool);
        ChannelManager::new(db)
    }

    #[tokio::test]
    async fn test_new_creates_empty_manager() {
        let manager = create_test_manager().await;
        let channels = manager.list(0, true).await;
        assert!(channels.is_empty());
    }

    #[tokio::test]
    async fn test_initialize_persistent_channels() {
        let manager = create_test_manager().await;

        let channels = vec![
            Channel::new(DEFAULT_CHANNEL.to_string()),
            Channel::with_settings(
                "#support".to_string(),
                Some("Get help here".to_string()),
                Some("admin".to_string()),
                false,
            ),
        ];

        manager.initialize_persistent_channels(channels).await;

        assert!(manager.exists(DEFAULT_CHANNEL).await);
        assert!(manager.exists("#support").await);
        assert!(manager.is_persistent(DEFAULT_CHANNEL).await);
        assert!(manager.is_persistent("#support").await);

        let support = manager.get_channel("#support").await.unwrap();
        assert_eq!(support.topic, Some("Get help here".to_string()));
    }

    #[tokio::test]
    async fn test_join_creates_channel() {
        let manager = create_test_manager().await;

        let result = manager.join("#general", 1).await.unwrap();

        assert!(!result.already_member);
        assert!(manager.exists("#general").await);
    }

    #[tokio::test]
    async fn test_join_existing_channel() {
        let manager = create_test_manager().await;

        // First user creates the channel
        manager.join("#general", 1).await.unwrap();

        // Second user joins existing channel
        let result = manager.join("#general", 2).await.unwrap();

        assert!(!result.already_member);
    }

    #[tokio::test]
    async fn test_join_already_member() {
        let manager = create_test_manager().await;

        manager.join("#general", 1).await.unwrap();
        let result = manager.join("#general", 1).await.unwrap();

        assert!(result.already_member);
    }

    #[tokio::test]
    async fn test_leave_channel() {
        let manager = create_test_manager().await;

        manager.join("#general", 1).await.unwrap();
        manager.join("#general", 2).await.unwrap();

        let result = manager.leave("#general", 1).await;

        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.remaining_member_session_ids, vec![2]);
        // Channel still exists (has member 2)
        assert!(manager.exists("#general").await);
    }

    #[tokio::test]
    async fn test_leave_deletes_empty_ephemeral_channel() {
        let manager = create_test_manager().await;

        manager.join("#general", 1).await.unwrap();
        let result = manager.leave("#general", 1).await;

        assert!(result.is_some());
        let result = result.unwrap();
        assert!(result.remaining_member_session_ids.is_empty());
        // Ephemeral channel deleted when empty
        assert!(!manager.exists("#general").await);
    }

    #[tokio::test]
    async fn test_leave_keeps_empty_persistent_channel() {
        let manager = create_test_manager().await;

        manager
            .initialize_persistent_channels(vec![Channel::new(DEFAULT_CHANNEL.to_string())])
            .await;

        manager.join(DEFAULT_CHANNEL, 1).await.unwrap();
        let result = manager.leave(DEFAULT_CHANNEL, 1).await;

        assert!(result.is_some());
        let result = result.unwrap();
        assert!(result.remaining_member_session_ids.is_empty());
        // Persistent channel kept even when empty
        assert!(manager.exists(DEFAULT_CHANNEL).await);
    }

    #[tokio::test]
    async fn test_leave_nonexistent_channel() {
        let manager = create_test_manager().await;

        let result = manager.leave("#nonexistent", 1).await;

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_leave_not_member() {
        let manager = create_test_manager().await;

        manager.join("#general", 1).await.unwrap();
        let result = manager.leave("#general", 2).await;

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_remove_from_all() {
        let manager = create_test_manager().await;

        manager.join("#general", 1).await.unwrap();
        manager.join("#support", 1).await.unwrap();
        manager.join("#support", 2).await.unwrap();

        let channel_names = manager.remove_from_all(1).await;

        // User was in 2 channels
        assert_eq!(channel_names.len(), 2);
        assert!(channel_names.iter().any(|n| n == "#general"));
        assert!(channel_names.iter().any(|n| n == "#support"));

        // User 1 should not be in any channel now
        assert!(!manager.is_member("#general", 1).await);
        assert!(!manager.is_member("#support", 1).await);

        // #support should still exist (has user 2)
        assert!(manager.exists("#support").await);
    }

    #[tokio::test]
    async fn test_remove_from_all_keeps_persistent() {
        let manager = create_test_manager().await;

        manager
            .initialize_persistent_channels(vec![Channel::new(DEFAULT_CHANNEL.to_string())])
            .await;

        manager.join(DEFAULT_CHANNEL, 1).await.unwrap();
        manager.join("#general", 1).await.unwrap();

        manager.remove_from_all(1).await;

        // Default channel should still exist (persistent)
        assert!(manager.exists(DEFAULT_CHANNEL).await);

        // #general should be deleted (ephemeral + empty)
        assert!(!manager.exists("#general").await);
    }

    #[tokio::test]
    async fn test_list_channels() {
        let manager = create_test_manager().await;

        manager.join("#general", 1).await.unwrap();
        manager.join("#support", 1).await.unwrap();

        let list = manager.list(1, false).await;

        assert_eq!(list.len(), 2);
        let names: Vec<&str> = list.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"#general"));
        assert!(names.contains(&"#support"));
    }

    #[tokio::test]
    async fn test_list_hides_secret_channels() {
        let manager = create_test_manager().await;

        manager.join("#public", 1).await.unwrap();
        manager.join("#secret", 2).await.unwrap();
        manager.set_secret("#secret", true).await.unwrap();

        // User 1 should see #public (non-secret) but not #secret
        let list = manager.list(1, false).await;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "#public");

        // User 2 should see both: #public (non-secret, visible to all) and #secret (member)
        let list = manager.list(2, false).await;
        assert_eq!(list.len(), 2);
        let names: Vec<_> = list.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"#public"));
        assert!(names.contains(&"#secret"));

        // Admin should see all
        let list = manager.list(99, true).await;
        assert_eq!(list.len(), 2);
    }

    #[tokio::test]
    async fn test_set_secret() {
        let manager = create_test_manager().await;

        manager.join("#general", 1).await.unwrap();

        let result = manager.set_secret("#general", true).await.unwrap();
        assert!(result);

        let channel = manager.get_channel("#general").await.unwrap();
        assert!(channel.secret);
    }

    #[tokio::test]
    async fn test_set_topic() {
        let manager = create_test_manager().await;

        manager.join("#general", 1).await.unwrap();

        manager
            .set_topic(
                "#general",
                Some("Welcome!".to_string()),
                Some("admin".to_string()),
            )
            .await
            .unwrap();

        let channel = manager.get_channel("#general").await.unwrap();
        assert_eq!(channel.topic, Some("Welcome!".to_string()));
        assert_eq!(channel.topic_set_by, Some("admin".to_string()));
    }

    #[tokio::test]
    async fn test_is_member() {
        let manager = create_test_manager().await;

        manager.join("#general", 1).await.unwrap();

        assert!(manager.is_member("#general", 1).await);
        assert!(!manager.is_member("#general", 2).await);
        assert!(!manager.is_member("#nonexistent", 1).await);
    }

    #[tokio::test]
    async fn test_get_members() {
        let manager = create_test_manager().await;

        manager.join("#general", 1).await.unwrap();
        manager.join("#general", 2).await.unwrap();

        let members = manager.get_members("#general").await.unwrap();
        assert_eq!(members.len(), 2);
        assert!(members.contains(&1));
        assert!(members.contains(&2));
    }

    #[tokio::test]
    async fn test_case_insensitive() {
        let manager = create_test_manager().await;

        manager.join("#General", 1).await.unwrap();

        assert!(manager.exists("#general").await);
        assert!(manager.exists("#GENERAL").await);
        assert!(manager.is_member("#GENERAL", 1).await);
    }

    #[tokio::test]
    async fn test_preserves_original_case() {
        let manager = create_test_manager().await;

        manager.join("#MyChannel", 1).await.unwrap();

        let channel = manager.get_channel("#mychannel").await.unwrap();
        assert_eq!(channel.name, "#MyChannel");
    }

    // ========================================================================
    // Concurrent Operation Tests
    // ========================================================================

    #[tokio::test]
    async fn test_concurrent_joins_same_channel() {
        let manager = create_test_manager().await;
        let manager = std::sync::Arc::new(manager);

        // Spawn multiple tasks joining the same channel concurrently
        let mut handles = Vec::new();
        for i in 0..10 {
            let manager = manager.clone();
            handles.push(tokio::spawn(
                async move { manager.join("#concurrent", i).await },
            ));
        }

        // Wait for all joins to complete
        for handle in handles {
            let _ = handle.await.expect("Task panicked");
        }

        // Verify all 10 users are members
        let members = manager.get_members("#concurrent").await.unwrap();
        assert_eq!(members.len(), 10);
        for i in 0..10 {
            assert!(members.contains(&i), "Missing member {}", i);
        }
    }

    #[tokio::test]
    async fn test_concurrent_join_and_leave() {
        let manager = create_test_manager().await;
        let manager = std::sync::Arc::new(manager);

        // First, have some users join
        for i in 0..5 {
            manager.join("#concurrent", i).await.unwrap();
        }

        // Concurrently: users 0-4 leave, users 5-9 join
        let mut leave_handles = Vec::new();
        let mut join_handles = Vec::new();
        for i in 0..5 {
            let manager = manager.clone();
            leave_handles.push(tokio::spawn(async move {
                manager.leave("#concurrent", i).await
            }));
        }
        for i in 5..10 {
            let manager = manager.clone();
            join_handles.push(tokio::spawn(
                async move { manager.join("#concurrent", i).await },
            ));
        }

        // Wait for all operations
        for handle in leave_handles {
            handle.await.expect("Task panicked");
        }
        for handle in join_handles {
            let _ = handle.await.expect("Task panicked");
        }

        // Verify only users 5-9 are members
        let members = manager.get_members("#concurrent").await.unwrap();
        assert_eq!(members.len(), 5);
        for i in 5..10 {
            assert!(members.contains(&i), "Missing member {}", i);
        }
        for i in 0..5 {
            assert!(!members.contains(&i), "User {} should have left", i);
        }
    }

    #[tokio::test]
    async fn test_concurrent_channel_creation() {
        let manager = create_test_manager().await;
        let manager = std::sync::Arc::new(manager);

        // Multiple users try to create different channels concurrently
        let mut handles = Vec::new();
        for i in 0..10 {
            let manager = manager.clone();
            let channel = format!("#channel{}", i);
            handles.push(tokio::spawn(async move { manager.join(&channel, i).await }));
        }

        // Wait for all
        for handle in handles {
            let _ = handle.await.expect("Task panicked");
        }

        // Verify all 10 channels exist
        for i in 0..10 {
            assert!(manager.exists(&format!("#channel{}", i)).await);
        }
    }

    #[tokio::test]
    async fn test_concurrent_remove_from_all() {
        let manager = create_test_manager().await;
        let manager = std::sync::Arc::new(manager);

        // User 1 joins many channels
        for i in 0..20 {
            manager.join(&format!("#channel{}", i), 1).await.unwrap();
        }

        // User 2 joins some of them
        for i in 0..10 {
            manager.join(&format!("#channel{}", i), 2).await.unwrap();
        }

        // Concurrently remove user 1 from all channels while user 2 continues operating
        let manager1 = manager.clone();
        let manager2 = manager.clone();

        let handle1 = tokio::spawn(async move { manager1.remove_from_all(1).await });

        let handle2 = tokio::spawn(async move {
            // User 2 leaves some channels during remove_from_all
            for i in 0..5 {
                manager2.leave(&format!("#channel{}", i), 2).await;
            }
        });

        handle1.await.expect("Task panicked");
        handle2.await.expect("Task panicked");

        // Verify user 1 is not in any channel
        for i in 0..20 {
            assert!(!manager.is_member(&format!("#channel{}", i), 1).await);
        }

        // User 2 should still be in channels 5-9
        for i in 5..10 {
            assert!(manager.is_member(&format!("#channel{}", i), 2).await);
        }
    }

    // ========================================================================
    // Edge Case Tests
    // ========================================================================

    #[tokio::test]
    async fn test_join_at_boundary_channel_name() {
        use nexus_common::validators::MAX_CHANNEL_LENGTH;

        let manager = create_test_manager().await;

        // Create channel name at exactly max length
        let max_name = format!("#{}", "a".repeat(MAX_CHANNEL_LENGTH - 1));
        let result = manager.join(&max_name, 1).await.unwrap();

        assert!(!result.already_member);
        assert!(manager.exists(&max_name).await);
    }

    #[tokio::test]
    async fn test_unicode_channel_names() {
        let manager = create_test_manager().await;

        // Test various Unicode channel names
        let unicode_channels = vec![
            "#日本語",
            "#Россия",
            "#한국어",
            "#العربية",
            "#emoji🎉",
            "#mïxéd",
        ];

        for (i, channel) in unicode_channels.iter().enumerate() {
            manager.join(channel, i as u32).await.unwrap();
        }

        // Verify all exist
        for channel in &unicode_channels {
            assert!(
                manager.exists(channel).await,
                "Channel {} should exist",
                channel
            );
        }

        // Verify case-insensitivity works with Unicode
        assert!(manager.exists("#РОССИЯ").await);
        assert!(manager.exists("#日本語").await); // Already lowercase
    }

    #[tokio::test]
    async fn test_reinitialize_with_overlapping_channels() {
        let manager = create_test_manager().await;

        // Initialize with channels A, B, C
        manager
            .initialize_persistent_channels(vec![
                Channel::new("#channelA".to_string()),
                Channel::new("#channelB".to_string()),
                Channel::new("#channelC".to_string()),
            ])
            .await;

        // Join some users
        manager.join("#channelA", 1).await.unwrap();
        manager.join("#channelB", 2).await.unwrap();
        manager.join("#channelC", 3).await.unwrap();

        // Reinitialize with B, C, D (A removed, D added)
        manager
            .reinitialize_persistent_channels(vec![
                Channel::new("#channelB".to_string()),
                Channel::new("#channelC".to_string()),
                Channel::new("#channelD".to_string()),
            ])
            .await;

        // A should still exist (has members) but no longer persistent
        assert!(manager.exists("#channelA").await);
        assert!(!manager.is_persistent("#channelA").await);

        // B, C, D should be persistent
        assert!(manager.is_persistent("#channelB").await);
        assert!(manager.is_persistent("#channelC").await);
        assert!(manager.is_persistent("#channelD").await);

        // Members should be preserved
        assert!(manager.is_member("#channelA", 1).await);
        assert!(manager.is_member("#channelB", 2).await);
        assert!(manager.is_member("#channelC", 3).await);
    }

    #[tokio::test]
    async fn test_reinitialize_deletes_empty_non_persistent() {
        let manager = create_test_manager().await;

        // Initialize with channels A, B
        manager
            .initialize_persistent_channels(vec![
                Channel::new("#channelA".to_string()),
                Channel::new("#channelB".to_string()),
            ])
            .await;

        // Don't join anyone to A (it's empty)
        manager.join("#channelB", 1).await.unwrap();

        // Reinitialize with only B (A removed)
        manager
            .reinitialize_persistent_channels(vec![Channel::new("#channelB".to_string())])
            .await;

        // A should be deleted (was empty and no longer persistent)
        assert!(!manager.exists("#channelA").await);

        // B should still exist
        assert!(manager.exists("#channelB").await);
    }

    #[tokio::test]
    async fn test_leave_nonexistent_returns_none() {
        let manager = create_test_manager().await;

        let result = manager.leave("#nonexistent", 1).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_set_secret_nonexistent_returns_false() {
        let manager = create_test_manager().await;

        let result = manager.set_secret("#nonexistent", true).await.unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn test_set_topic_nonexistent_returns_false() {
        let manager = create_test_manager().await;
        let result = manager
            .set_topic(
                "#nonexistent",
                Some("topic".to_string()),
                Some("admin".to_string()),
            )
            .await
            .unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn test_get_members_nonexistent_returns_none() {
        let manager = create_test_manager().await;

        let result = manager.get_members("#nonexistent").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_double_join_idempotent() {
        let manager = create_test_manager().await;

        let result1 = manager.join("#general", 1).await.unwrap();
        assert!(!result1.already_member);

        let result2 = manager.join("#general", 1).await.unwrap();
        assert!(result2.already_member);

        // Should still have exactly 1 member
        let members = manager.get_members("#general").await.unwrap();
        assert_eq!(members.len(), 1);
    }

    #[tokio::test]
    async fn test_double_leave_safe() {
        let manager = create_test_manager().await;

        manager.join("#general", 1).await.unwrap();

        let result1 = manager.leave("#general", 1).await;
        assert!(result1.is_some());

        // Second leave should return None (not a member anymore)
        let result2 = manager.leave("#general", 1).await;
        assert!(result2.is_none());
    }

    #[tokio::test]
    async fn test_secret_channel_visibility_for_non_member() {
        let manager = create_test_manager().await;

        // User 1 creates a secret channel
        manager.join("#secret", 1).await.unwrap();
        manager.set_secret("#secret", true).await.unwrap();

        // User 2 (non-member, non-admin) should not see it in list
        let list = manager.list(2, false).await;
        assert!(list.is_empty());

        // But the channel exists
        assert!(manager.exists("#secret").await);

        // And is_member returns false (not revealing it's secret)
        assert!(!manager.is_member("#secret", 2).await);
    }

    #[tokio::test]
    async fn test_join_enforces_channel_limit() {
        use super::super::types::JoinError;

        let manager = create_test_manager().await;

        // Join MAX_CHANNELS_PER_USER channels
        for i in 0..MAX_CHANNELS_PER_USER {
            let result = manager.join(&format!("#channel{}", i), 1).await;
            assert!(result.is_ok(), "Should be able to join channel {}", i);
        }

        // Try to join one more - should fail with TooManyChannels
        let result = manager.join("#onemore", 1).await;
        assert!(matches!(result, Err(JoinError::TooManyChannels)));

        // Verify the channel was NOT created
        assert!(!manager.exists("#onemore").await);
    }

    #[tokio::test]
    async fn test_get_channels_for_session_empty() {
        let manager = create_test_manager().await;

        // Session not in any channel
        let channels = manager.get_channels_for_session(1, false).await;
        assert!(channels.is_empty());
    }

    #[tokio::test]
    async fn test_get_channels_for_session_returns_joined_channels() {
        let manager = create_test_manager().await;

        // Join some channels
        manager.join("#alpha", 1).await.unwrap();
        manager.join("#beta", 1).await.unwrap();
        manager.join("#gamma", 1).await.unwrap();

        let channels = manager.get_channels_for_session(1, false).await;
        assert_eq!(channels.len(), 3);
        // Should be sorted alphabetically (case-insensitive)
        assert_eq!(channels, vec!["#alpha", "#beta", "#gamma"]);
    }

    #[tokio::test]
    async fn test_get_channels_for_session_excludes_secret_for_non_admin() {
        let manager = create_test_manager().await;

        // Join channels
        manager.join("#public", 1).await.unwrap();
        manager.join("#secret", 1).await.unwrap();

        // Make #secret secret
        let _ = manager.set_secret("#secret", true).await;

        // Non-admin should not see secret channel
        let channels = manager.get_channels_for_session(1, false).await;
        assert_eq!(channels, vec!["#public"]);
    }

    #[tokio::test]
    async fn test_get_channels_for_session_includes_secret_for_admin() {
        let manager = create_test_manager().await;

        // Join channels
        manager.join("#public", 1).await.unwrap();
        manager.join("#secret", 1).await.unwrap();

        // Make #secret secret
        let _ = manager.set_secret("#secret", true).await;

        // Admin should see all channels including secret
        let channels = manager.get_channels_for_session(1, true).await;
        assert_eq!(channels.len(), 2);
        assert!(channels.contains(&"#public".to_string()));
        assert!(channels.contains(&"#secret".to_string()));
    }

    #[tokio::test]
    async fn test_get_channels_for_session_sorted_case_insensitive() {
        let manager = create_test_manager().await;

        // Join channels with mixed case
        manager.join("#Zebra", 1).await.unwrap();
        manager.join("#alpha", 1).await.unwrap();
        manager.join("#BETA", 1).await.unwrap();

        let channels = manager.get_channels_for_session(1, false).await;
        // Should be sorted case-insensitively: alpha, BETA, Zebra
        assert_eq!(channels, vec!["#alpha", "#BETA", "#Zebra"]);
    }

    #[tokio::test]
    async fn test_already_member_does_not_count_against_limit() {
        let manager = create_test_manager().await;

        // Join MAX_CHANNELS_PER_USER channels
        for i in 0..MAX_CHANNELS_PER_USER {
            manager.join(&format!("#channel{}", i), 1).await.unwrap();
        }

        // Re-joining an existing channel should succeed (already_member)
        let result = manager.join("#channel0", 1).await;
        assert!(result.is_ok());
        assert!(result.unwrap().already_member);
    }
}
