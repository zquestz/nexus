//! Helper methods for UserManager

use super::UserManager;

impl UserManager {
    /// Remove disconnected users from the manager
    ///
    /// Takes a list of session IDs whose channels have closed and removes them
    /// from the UserManager. This is called by broadcast methods when they detect
    /// that a user's channel has been closed.
    pub(super) async fn remove_disconnected(&self, session_ids: Vec<u32>) {
        if !session_ids.is_empty() {
            let mut users = self.users.write().await;
            for session_id in session_ids {
                users.remove(&session_id);
            }
        }
    }
}
