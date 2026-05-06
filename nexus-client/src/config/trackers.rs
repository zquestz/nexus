//! Client tracker management methods for [`Config`].

use uuid::Uuid;

use crate::types::ClientTracker;

use super::Config;

impl Config {
    /// Add a new tracker to the config.
    pub fn add_tracker(&mut self, tracker: ClientTracker) {
        self.client_trackers.push(tracker);
    }

    /// Delete a tracker by id. No-op if no tracker with the given id exists.
    pub fn delete_tracker(&mut self, id: Uuid) {
        self.client_trackers.retain(|t| t.id != id);
    }

    /// Update an existing tracker by id. No-op if no tracker with the given
    /// id exists. The id field of `tracker` is irrelevant — the lookup is
    /// done by the `id` argument and the stored row's id is preserved.
    pub fn update_tracker(&mut self, id: Uuid, tracker: ClientTracker) {
        if let Some(existing) = self.client_trackers.iter_mut().find(|t| t.id == id) {
            *existing = ClientTracker { id, ..tracker };
        }
    }

    /// Borrow a tracker by id.
    #[allow(dead_code)] // Reserved for handler call sites added in later steps.
    pub fn get_tracker(&self, id: Uuid) -> Option<&ClientTracker> {
        self.client_trackers.iter().find(|t| t.id == id)
    }

    /// Mutably borrow a tracker by id. Used by the TOFU-pin commit path.
    #[allow(dead_code)] // Reserved for handler call sites added in later steps.
    pub fn get_tracker_mut(&mut self, id: Uuid) -> Option<&mut ClientTracker> {
        self.client_trackers.iter_mut().find(|t| t.id == id)
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn tracker(name: &str) -> ClientTracker {
        ClientTracker {
            name: name.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn test_add_tracker() {
        let mut config = Config::default();
        config.add_tracker(tracker("Tracker One"));
        assert_eq!(config.client_trackers.len(), 1);
        assert_eq!(config.client_trackers[0].name, "Tracker One");
    }

    #[test]
    fn test_add_multiple_trackers_preserves_order() {
        let mut config = Config::default();
        config.add_tracker(tracker("A"));
        config.add_tracker(tracker("B"));
        assert_eq!(config.client_trackers.len(), 2);
        assert_eq!(config.client_trackers[0].name, "A");
        assert_eq!(config.client_trackers[1].name, "B");
    }

    #[test]
    fn test_delete_tracker() {
        let mut config = Config::default();
        let t1 = tracker("A");
        let t2 = tracker("B");
        let id1 = t1.id;
        config.add_tracker(t1);
        config.add_tracker(t2);

        config.delete_tracker(id1);
        assert_eq!(config.client_trackers.len(), 1);
        assert_eq!(config.client_trackers[0].name, "B");
    }

    #[test]
    fn test_delete_tracker_nonexistent_is_noop() {
        let mut config = Config::default();
        config.add_tracker(tracker("A"));
        config.delete_tracker(Uuid::new_v4());
        assert_eq!(config.client_trackers.len(), 1);
    }

    #[test]
    fn test_update_tracker_preserves_id() {
        // The stored row's id is preserved even if the caller passes a
        // ClientTracker whose id field is something else. This is the
        // contract that lets the panel build a fresh ClientTracker from
        // form fields and pass it through `update_tracker` without
        // having to thread the original id through the form state.
        let mut config = Config::default();
        let t = tracker("Original");
        let id = t.id;
        config.add_tracker(t);

        let replacement = ClientTracker {
            id: Uuid::new_v4(), // deliberately different
            name: "Updated".to_string(),
            address: "tracker.example".to_string(),
            port: 7511,
            password: Some("invite".to_string()),
            certificate_fingerprint: None,
        };
        config.update_tracker(id, replacement);

        assert_eq!(config.client_trackers.len(), 1);
        let stored = &config.client_trackers[0];
        assert_eq!(stored.id, id, "stored id must match the lookup id");
        assert_eq!(stored.name, "Updated");
        assert_eq!(stored.address, "tracker.example");
        assert_eq!(stored.port, 7511);
    }

    #[test]
    fn test_update_tracker_nonexistent_is_noop() {
        let mut config = Config::default();
        config.add_tracker(tracker("A"));
        config.update_tracker(Uuid::new_v4(), tracker("Should Not Appear"));
        assert_eq!(config.client_trackers.len(), 1);
        assert_eq!(config.client_trackers[0].name, "A");
    }

    #[test]
    fn test_get_tracker() {
        let mut config = Config::default();
        let t = tracker("A");
        let id = t.id;
        config.add_tracker(t);
        assert_eq!(config.get_tracker(id).map(|t| t.name.as_str()), Some("A"));
    }

    #[test]
    fn test_get_tracker_nonexistent() {
        let config = Config::default();
        assert!(config.get_tracker(Uuid::new_v4()).is_none());
    }

    #[test]
    fn test_get_tracker_mut_allows_pin_commit() {
        // The TOFU-pin commit path goes through `get_tracker_mut` to write
        // the observed fingerprint into the row in place. Pin the contract.
        let mut config = Config::default();
        let t = tracker("A");
        let id = t.id;
        config.add_tracker(t);

        let row = config
            .get_tracker_mut(id)
            .expect("tracker should exist after add");
        row.certificate_fingerprint = Some("AA:BB".to_string());

        assert_eq!(
            config
                .get_tracker(id)
                .and_then(|t| t.certificate_fingerprint.as_deref()),
            Some("AA:BB")
        );
    }
}
