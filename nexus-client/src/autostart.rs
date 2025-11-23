//! Auto-connect functionality at startup

use crate::config::Config;
use crate::types::Message;
use iced::Task;

/// Generate connection tasks for bookmarks with auto_connect enabled
pub fn generate_auto_connect_tasks(config: &Config) -> Vec<Task<Message>> {
    config
        .bookmarks
        .iter()
        .enumerate()
        .filter(|(_, bookmark)| bookmark.auto_connect)
        .map(|(index, _)| Task::done(Message::ConnectToBookmark(index)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{DEFAULT_PORT, ServerBookmark};

    #[test]
    fn test_no_auto_connect_bookmarks() {
        let mut config = Config::default();
        config.add_bookmark(ServerBookmark {
            name: "Test Server".to_string(),
            address: "200::1".to_string(),
            port: DEFAULT_PORT.to_string(),
            username: "user".to_string(),
            password: "pass".to_string(),
            auto_connect: false,
        });

        let tasks = generate_auto_connect_tasks(&config);
        assert_eq!(tasks.len(), 0);
    }

    #[test]
    fn test_single_auto_connect_bookmark() {
        let mut config = Config::default();
        config.add_bookmark(ServerBookmark {
            name: "Auto Server".to_string(),
            address: "200::1".to_string(),
            port: DEFAULT_PORT.to_string(),
            username: "user".to_string(),
            password: "pass".to_string(),
            auto_connect: true,
        });

        let tasks = generate_auto_connect_tasks(&config);
        assert_eq!(tasks.len(), 1);
    }

    #[test]
    fn test_multiple_auto_connect_bookmarks() {
        let mut config = Config::default();
        config.add_bookmark(ServerBookmark {
            name: "Server 1".to_string(),
            address: "200::1".to_string(),
            port: DEFAULT_PORT.to_string(),
            username: "user1".to_string(),
            password: "pass1".to_string(),
            auto_connect: true,
        });
        config.add_bookmark(ServerBookmark {
            name: "Server 2".to_string(),
            address: "200::2".to_string(),
            port: DEFAULT_PORT.to_string(),
            username: "user2".to_string(),
            password: "pass2".to_string(),
            auto_connect: false,
        });
        config.add_bookmark(ServerBookmark {
            name: "Server 3".to_string(),
            address: "200::3".to_string(),
            port: DEFAULT_PORT.to_string(),
            username: "user3".to_string(),
            password: "pass3".to_string(),
            auto_connect: true,
        });

        let tasks = generate_auto_connect_tasks(&config);
        assert_eq!(tasks.len(), 2);
    }
}
