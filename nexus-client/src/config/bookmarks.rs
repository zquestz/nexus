//! Bookmark management methods for Config

use uuid::Uuid;

use crate::types::ServerBookmark;

use super::Config;

impl Config {
    /// Add a new bookmark to the configuration
    pub fn add_bookmark(&mut self, bookmark: ServerBookmark) {
        self.bookmarks.push(bookmark);
    }

    /// Delete a bookmark by ID
    ///
    /// Does nothing if no bookmark with the given ID exists.
    pub fn delete_bookmark(&mut self, id: Uuid) {
        self.bookmarks.retain(|b| b.id != id);
    }

    /// Get a bookmark by ID
    ///
    /// Returns None if no bookmark with the given ID exists.
    pub fn get_bookmark(&self, id: Uuid) -> Option<&ServerBookmark> {
        self.bookmarks.iter().find(|b| b.id == id)
    }

    /// Get a mutable bookmark by ID
    ///
    /// Returns None if no bookmark with the given ID exists.
    pub fn get_bookmark_mut(&mut self, id: Uuid) -> Option<&mut ServerBookmark> {
        self.bookmarks.iter_mut().find(|b| b.id == id)
    }

    /// Update an existing bookmark by ID
    ///
    /// Does nothing if no bookmark with the given ID exists.
    pub fn update_bookmark(&mut self, id: Uuid, bookmark: ServerBookmark) {
        if let Some(existing) = self.bookmarks.iter_mut().find(|b| b.id == id) {
            *existing = bookmark;
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create a bookmark with just a name
    fn bookmark(name: &str) -> ServerBookmark {
        ServerBookmark {
            id: Uuid::new_v4(),
            name: name.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn test_add_bookmark() {
        let mut config = Config::default();
        config.add_bookmark(bookmark("Test Server"));

        assert_eq!(config.bookmarks.len(), 1);
        assert_eq!(config.bookmarks[0].name, "Test Server");
    }

    #[test]
    fn test_add_multiple_bookmarks() {
        let mut config = Config::default();
        config.add_bookmark(bookmark("Server 1"));
        config.add_bookmark(bookmark("Server 2"));

        assert_eq!(config.bookmarks.len(), 2);
        assert_eq!(config.bookmarks[0].name, "Server 1");
        assert_eq!(config.bookmarks[1].name, "Server 2");
    }

    #[test]
    fn test_delete_bookmark() {
        let mut config = Config::default();
        let bookmark1 = bookmark("Server 1");
        let bookmark2 = bookmark("Server 2");
        let id1 = bookmark1.id;
        config.add_bookmark(bookmark1);
        config.add_bookmark(bookmark2);

        config.delete_bookmark(id1);

        assert_eq!(config.bookmarks.len(), 1);
        assert_eq!(config.bookmarks[0].name, "Server 2");
    }

    #[test]
    fn test_delete_bookmark_nonexistent() {
        let mut config = Config::default();
        config.add_bookmark(bookmark("Server 1"));

        config.delete_bookmark(Uuid::new_v4());

        assert_eq!(config.bookmarks.len(), 1);
        assert_eq!(config.bookmarks[0].name, "Server 1");
    }

    #[test]
    fn test_get_bookmark() {
        let mut config = Config::default();
        let bm = bookmark("Test Server");
        let id = bm.id;
        config.add_bookmark(bm);

        let result = config.get_bookmark(id);
        assert_eq!(result.map(|b| b.name.as_str()), Some("Test Server"));
    }

    #[test]
    fn test_get_bookmark_nonexistent() {
        let mut config = Config::default();
        config.add_bookmark(bookmark("Test Server"));

        assert!(config.get_bookmark(Uuid::new_v4()).is_none());
    }

    #[test]
    fn test_update_bookmark() {
        let mut config = Config::default();
        let bm = bookmark("Original");
        let id = bm.id;
        config.add_bookmark(bm);

        config.update_bookmark(
            id,
            ServerBookmark {
                id,
                name: "Updated".to_string(),
                address: "200::2".to_string(),
                port: 8000,
                auto_connect: true,
                ..Default::default()
            },
        );

        assert_eq!(config.bookmarks.len(), 1);
        assert_eq!(config.bookmarks[0].name, "Updated");
        assert_eq!(config.bookmarks[0].address, "200::2");
        assert_eq!(config.bookmarks[0].port, 8000);
        assert!(config.bookmarks[0].auto_connect);
    }

    #[test]
    fn test_update_bookmark_nonexistent() {
        let mut config = Config::default();
        config.add_bookmark(bookmark("Server 1"));

        config.update_bookmark(Uuid::new_v4(), bookmark("Should Not Appear"));

        assert_eq!(config.bookmarks.len(), 1);
        assert_eq!(config.bookmarks[0].name, "Server 1");
    }
}
