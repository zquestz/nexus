//! Auto-connect functionality at startup

use iced::Task;

use crate::config::Config;
use crate::types::Message;

/// Generate connection tasks for bookmarks with auto_connect enabled
///
/// Returns one task per bookmark that has `auto_connect = true`.
/// These tasks are executed during application startup.
pub fn generate_auto_connect_tasks(config: &Config) -> Vec<Task<Message>> {
    config
        .bookmarks
        .iter()
        .filter(|bookmark| bookmark.auto_connect)
        .map(|bookmark| Task::done(Message::ConnectToBookmark(bookmark.id)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ServerBookmark;

    /// Helper to create a bookmark with just name and auto_connect
    fn bookmark(name: &str, auto_connect: bool) -> ServerBookmark {
        ServerBookmark {
            name: name.to_string(),
            auto_connect,
            ..Default::default()
        }
    }

    #[test]
    fn test_empty_config() {
        let config = Config::default();
        let tasks = generate_auto_connect_tasks(&config);
        assert_eq!(tasks.len(), 0);
    }

    #[test]
    fn test_no_auto_connect_bookmarks() {
        let mut config = Config::default();
        config.add_bookmark(bookmark("Server", false));

        let tasks = generate_auto_connect_tasks(&config);
        assert_eq!(tasks.len(), 0);
    }

    #[test]
    fn test_single_auto_connect_bookmark() {
        let mut config = Config::default();
        config.add_bookmark(bookmark("Server", true));

        let tasks = generate_auto_connect_tasks(&config);
        assert_eq!(tasks.len(), 1);
    }

    #[test]
    fn test_multiple_auto_connect_bookmarks() {
        let mut config = Config::default();
        config.add_bookmark(bookmark("Server 1", true));
        config.add_bookmark(bookmark("Server 2", false));
        config.add_bookmark(bookmark("Server 3", true));

        let tasks = generate_auto_connect_tasks(&config);
        assert_eq!(tasks.len(), 2);
    }
}
