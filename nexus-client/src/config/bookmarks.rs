//! Bookmark management methods for Config

use crate::types::ServerBookmark;

use super::Config;

impl Config {
    /// Add a new bookmark to the configuration
    pub fn add_bookmark(&mut self, bookmark: ServerBookmark) {
        self.bookmarks.push(bookmark);
    }

    /// Delete a bookmark at the given index
    ///
    /// Does nothing if the index is out of bounds.
    pub fn delete_bookmark(&mut self, index: usize) {
        if index < self.bookmarks.len() {
            self.bookmarks.remove(index);
        }
    }

    /// Get a bookmark by index
    ///
    /// Returns None if the index is out of bounds.
    pub fn get_bookmark(&self, index: usize) -> Option<&ServerBookmark> {
        self.bookmarks.get(index)
    }

    /// Update an existing bookmark at the given index
    ///
    /// Does nothing if the index is out of bounds.
    pub fn update_bookmark(&mut self, index: usize, bookmark: ServerBookmark) {
        if index < self.bookmarks.len() {
            self.bookmarks[index] = bookmark;
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
        config.add_bookmark(bookmark("Server 1"));
        config.add_bookmark(bookmark("Server 2"));

        config.delete_bookmark(0);

        assert_eq!(config.bookmarks.len(), 1);
        assert_eq!(config.bookmarks[0].name, "Server 2");
    }

    #[test]
    fn test_delete_bookmark_out_of_bounds() {
        let mut config = Config::default();
        config.add_bookmark(bookmark("Server 1"));

        config.delete_bookmark(10);

        assert_eq!(config.bookmarks.len(), 1);
        assert_eq!(config.bookmarks[0].name, "Server 1");
    }

    #[test]
    fn test_get_bookmark() {
        let mut config = Config::default();
        config.add_bookmark(bookmark("Test Server"));

        let result = config.get_bookmark(0);
        assert_eq!(result.map(|b| b.name.as_str()), Some("Test Server"));
    }

    #[test]
    fn test_get_bookmark_out_of_bounds() {
        let mut config = Config::default();
        config.add_bookmark(bookmark("Test Server"));

        assert!(config.get_bookmark(5).is_none());
    }

    #[test]
    fn test_update_bookmark() {
        let mut config = Config::default();
        config.add_bookmark(bookmark("Original"));

        config.update_bookmark(
            0,
            ServerBookmark {
                name: "Updated".to_string(),
                address: "200::2".to_string(),
                port: "8000".to_string(),
                auto_connect: true,
                ..Default::default()
            },
        );

        assert_eq!(config.bookmarks.len(), 1);
        assert_eq!(config.bookmarks[0].name, "Updated");
        assert_eq!(config.bookmarks[0].address, "200::2");
        assert_eq!(config.bookmarks[0].port, "8000");
        assert!(config.bookmarks[0].auto_connect);
    }

    #[test]
    fn test_update_bookmark_out_of_bounds() {
        let mut config = Config::default();
        config.add_bookmark(bookmark("Server 1"));

        config.update_bookmark(5, bookmark("Should Not Appear"));

        assert_eq!(config.bookmarks.len(), 1);
        assert_eq!(config.bookmarks[0].name, "Server 1");
    }
}
