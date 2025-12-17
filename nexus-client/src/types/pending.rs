//! Pending request tracking for response routing
//!
//! Some requests need special response handling based on how they were initiated:
//! - `/list all` - display results in chat instead of updating the user cache
//! - `/msg` - open a PM tab on successful delivery
//! - `/info` - display user info in chat
//! - Info icon click - populate the UserInfo panel
//!
//! This module provides types to track these requests by message ID so the
//! response handler knows how to route them.

use nexus_common::framing::MessageId;

/// How to route a response when it arrives
#[derive(Debug, Clone)]
pub enum ResponseRouting {
    /// Display user list in chat (from `/list all` command)
    DisplayListInChat,
    /// Open PM tab on success (from `/msg` command)
    OpenMessageTab(String),
    /// Display user info in chat (from `/info` command)
    DisplayUserInfoInChat,
    /// Populate user info panel if username matches (from info icon click)
    PopulateUserInfoPanel(String),
    /// Populate user management list (from panel open)
    PopulateUserManagementList,
    /// Populate user management edit form (from edit button click)
    PopulateUserManagementEdit,
    /// User management create result (return to list on success)
    UserManagementCreateResult,
    /// User management update result (return to list on success)
    UserManagementUpdateResult,
    /// User management delete result (return to list on success)
    UserManagementDeleteResult,
    /// Password change result (close panel on success, show error on failure)
    PasswordChangeResult,
    /// Populate news list (from panel open)
    PopulateNewsList,
    /// Populate news edit form (from edit button click)
    PopulateNewsEdit,
    /// News create result (return to list on success)
    NewsCreateResult,
    /// News update result (return to list on success)
    NewsUpdateResult,
    /// News delete result (return to list on success)
    NewsDeleteResult,
    /// News show result for refresh (after NewsUpdated broadcast)
    NewsShowForRefresh(i64),
}

/// Extension trait for tracking pending requests
///
/// This is implemented on `HashMap<MessageId, ResponseRouting>` to provide
/// a convenient method for tracking requests.
pub trait PendingRequests {
    /// Track a pending request for response routing
    fn track(&mut self, message_id: MessageId, routing: ResponseRouting);
}

impl PendingRequests for std::collections::HashMap<MessageId, ResponseRouting> {
    fn track(&mut self, message_id: MessageId, routing: ResponseRouting) {
        self.insert(message_id, routing);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_track_display_list_in_chat() {
        let mut pending: HashMap<MessageId, ResponseRouting> = HashMap::new();
        let id = MessageId::new();
        pending.track(id, ResponseRouting::DisplayListInChat);
        assert!(matches!(
            pending.get(&id),
            Some(ResponseRouting::DisplayListInChat)
        ));
    }

    #[test]
    fn test_track_open_message_tab() {
        let mut pending: HashMap<MessageId, ResponseRouting> = HashMap::new();
        let id = MessageId::new();
        pending.track(id, ResponseRouting::OpenMessageTab("alice".to_string()));
        assert!(matches!(
            pending.get(&id),
            Some(ResponseRouting::OpenMessageTab(name)) if name == "alice"
        ));
    }

    #[test]
    fn test_track_display_user_info_in_chat() {
        let mut pending: HashMap<MessageId, ResponseRouting> = HashMap::new();
        let id = MessageId::new();
        pending.track(id, ResponseRouting::DisplayUserInfoInChat);
        assert!(matches!(
            pending.get(&id),
            Some(ResponseRouting::DisplayUserInfoInChat)
        ));
    }

    #[test]
    fn test_track_populate_user_info_panel() {
        let mut pending: HashMap<MessageId, ResponseRouting> = HashMap::new();
        let id = MessageId::new();
        pending.track(
            id,
            ResponseRouting::PopulateUserInfoPanel("bob".to_string()),
        );
        assert!(matches!(
            pending.get(&id),
            Some(ResponseRouting::PopulateUserInfoPanel(name)) if name == "bob"
        ));
    }

    #[test]
    fn test_remove_returns_tracked_routing() {
        let mut pending: HashMap<MessageId, ResponseRouting> = HashMap::new();
        let id = MessageId::new();
        pending.track(id, ResponseRouting::DisplayListInChat);
        let removed = pending.remove(&id);
        assert!(matches!(removed, Some(ResponseRouting::DisplayListInChat)));
        assert!(pending.is_empty());
    }

    #[test]
    fn test_track_multiple_requests() {
        let mut pending: HashMap<MessageId, ResponseRouting> = HashMap::new();
        let id1 = MessageId::new();
        let id2 = MessageId::new();
        pending.track(id1, ResponseRouting::DisplayListInChat);
        pending.track(id2, ResponseRouting::OpenMessageTab("bob".to_string()));
        assert_eq!(pending.len(), 2);
        assert!(matches!(
            pending.get(&id1),
            Some(ResponseRouting::DisplayListInChat)
        ));
        assert!(matches!(
            pending.get(&id2),
            Some(ResponseRouting::OpenMessageTab(name)) if name == "bob"
        ));
    }

    #[test]
    fn test_track_overwrites_existing() {
        let mut pending: HashMap<MessageId, ResponseRouting> = HashMap::new();
        let id = MessageId::new();
        pending.track(id, ResponseRouting::DisplayListInChat);
        pending.track(id, ResponseRouting::OpenMessageTab("alice".to_string()));
        assert_eq!(pending.len(), 1);
        assert!(matches!(
            pending.get(&id),
            Some(ResponseRouting::OpenMessageTab(name)) if name == "alice"
        ));
    }
}
