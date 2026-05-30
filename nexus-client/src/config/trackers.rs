//! Client tracker management methods for [`Config`].

use uuid::Uuid;

use crate::types::ClientTracker;

use super::Config;

impl Config {
    /// Add a new tracker to the config. Appends to the end of
    /// `client_trackers`; stored order matches insertion order. Use
    /// [`Self::insert_tracker`] when callers need to preserve a
    /// specific position (e.g. rollback after a save failure).
    pub fn add_tracker(&mut self, tracker: ClientTracker) {
        self.client_trackers.push(tracker);
    }

    /// Insert a tracker at a specific position. Used by the Remove
    /// rollback path so a save-failure restores the row to its original
    /// index rather than appending to the end. `index` is clamped to
    /// `client_trackers.len()` so an out-of-range value falls back to
    /// `push` rather than panicking.
    pub fn insert_tracker(&mut self, index: usize, tracker: ClientTracker) {
        let safe_index = index.min(self.client_trackers.len());
        self.client_trackers.insert(safe_index, tracker);
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
    pub fn get_tracker(&self, id: Uuid) -> Option<&ClientTracker> {
        self.client_trackers.iter().find(|t| t.id == id)
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

    // ========================================================================
    // Rollback invariants
    //
    // The Add / Edit / Remove handlers in `handlers/tracker_browser.rs`
    // mutate the in-memory config first and then persist. On a save
    // failure they roll the in-memory mutation back so disk and memory
    // stay consistent. These tests pin the round-trip contract on the
    // primitives the handlers compose; they do NOT cover handler-level
    // state (is_submitting reset, error banner population) — those need
    // full NexusApp scaffolding.
    // ========================================================================

    #[test]
    fn add_rollback_restores_initial_state() {
        // Start state: two trackers.
        let mut config = Config::default();
        config.add_tracker(tracker("A"));
        config.add_tracker(tracker("B"));
        let snapshot_ids: Vec<Uuid> = config.client_trackers.iter().map(|t| t.id).collect();

        // Apply add, then roll back.
        let new = tracker("Pending");
        let new_id = new.id;
        config.add_tracker(new);
        config.delete_tracker(new_id);

        // Roll-back returns the Vec to its original length and id order.
        let after_ids: Vec<Uuid> = config.client_trackers.iter().map(|t| t.id).collect();
        assert_eq!(after_ids, snapshot_ids);
    }

    #[test]
    fn edit_rollback_restores_original_fields() {
        // Start with one row holding all-original field values.
        let mut config = Config::default();
        let original = ClientTracker {
            id: Uuid::new_v4(),
            name: "Original".to_string(),
            address: "orig.example.com".to_string(),
            port: 7510,
            password: Some("invite-orig".to_string()),
            certificate_fingerprint: Some("AA:BB".to_string()),
        };
        let id = original.id;
        config.add_tracker(original.clone());

        // Apply edit (replacement with totally different fields).
        let updated = ClientTracker {
            id,
            name: "Updated".to_string(),
            address: "new.example.com".to_string(),
            port: 9999,
            password: None,
            certificate_fingerprint: None,
        };
        config.update_tracker(id, updated);

        // Roll back by re-applying the original row.
        config.update_tracker(id, original.clone());

        let restored = config
            .get_tracker(id)
            .expect("row must still exist after rollback");
        assert_eq!(restored.id, original.id);
        assert_eq!(restored.name, original.name);
        assert_eq!(restored.address, original.address);
        assert_eq!(restored.port, original.port);
        assert_eq!(restored.password, original.password);
        assert_eq!(
            restored.certificate_fingerprint,
            original.certificate_fingerprint
        );
    }

    #[test]
    fn remove_rollback_restores_row_at_original_index() {
        // Start with three rows; remove the middle one and roll back.
        // The handler captures the original Vec index alongside the
        // value, so `insert_tracker` puts the row back where it was
        // — preserving on-disk `config.json` order across save-failed
        // removals.
        let mut config = Config::default();
        let a = tracker("A");
        let b = tracker("B");
        let c = tracker("C");
        let b_id = b.id;
        config.add_tracker(a.clone());
        config.add_tracker(b.clone());
        config.add_tracker(c.clone());

        // Mirror handler shape: capture index + value, delete, then
        // restore via insert_tracker on rollback.
        let original_index = config
            .client_trackers
            .iter()
            .position(|t| t.id == b_id)
            .expect("b must exist before rollback test");
        let original = config
            .get_tracker(b_id)
            .cloned()
            .expect("b must exist before rollback test");
        config.delete_tracker(b_id);
        config.insert_tracker(original_index, original);

        // Row is back at its original index; on-disk order preserved.
        let order: Vec<&str> = config
            .client_trackers
            .iter()
            .map(|t| t.name.as_str())
            .collect();
        assert_eq!(order, vec!["A", "B", "C"]);
    }

    #[test]
    fn insert_tracker_at_index_zero_prepends() {
        let mut config = Config::default();
        config.add_tracker(tracker("B"));
        config.add_tracker(tracker("C"));
        config.insert_tracker(0, tracker("A"));
        let order: Vec<&str> = config
            .client_trackers
            .iter()
            .map(|t| t.name.as_str())
            .collect();
        assert_eq!(order, vec!["A", "B", "C"]);
    }

    #[test]
    fn insert_tracker_at_end_index_appends() {
        let mut config = Config::default();
        config.add_tracker(tracker("A"));
        config.add_tracker(tracker("B"));
        // index == len is valid for Vec::insert; equivalent to push.
        config.insert_tracker(2, tracker("C"));
        let order: Vec<&str> = config
            .client_trackers
            .iter()
            .map(|t| t.name.as_str())
            .collect();
        assert_eq!(order, vec!["A", "B", "C"]);
    }

    #[test]
    fn insert_tracker_out_of_range_clamps_to_end() {
        // Out-of-range index falls back to push rather than panicking.
        let mut config = Config::default();
        config.add_tracker(tracker("A"));
        config.insert_tracker(99, tracker("B"));
        let order: Vec<&str> = config
            .client_trackers
            .iter()
            .map(|t| t.name.as_str())
            .collect();
        assert_eq!(order, vec!["A", "B"]);
    }
}
