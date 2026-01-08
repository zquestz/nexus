//! Chat message handlers

use iced::Task;
use nexus_common::protocol::ChatAction;

use crate::NexusApp;
use crate::config::events::EventType;
use crate::events::{EventContext, emit_event};
use crate::i18n::{t, t_args};
use crate::types::{ChatMessage, Message};

impl NexusApp {
    /// Handle incoming chat message
    pub fn handle_chat_message(
        &mut self,
        connection_id: usize,
        nickname: String,
        message: String,
        is_admin: bool,
        is_shared: bool,
        action: ChatAction,
    ) -> Task<Message> {
        // Check if we were mentioned in this message
        if let Some(conn) = self.connections.get(&connection_id) {
            // Get our nickname (from online users list, or fall back to username)
            let our_nickname = conn
                .online_users
                .iter()
                .find(|u| u.session_ids.contains(&conn.session_id))
                .map(|u| u.nickname.as_str())
                .unwrap_or(&conn.connection_info.username);

            // Check if message mentions our nickname (case-insensitive, word boundary)
            // and it's not from ourselves
            // Guard against empty nickname which would match everything
            let our_nickname_lower = our_nickname.to_lowercase();
            let is_from_self = nickname.eq_ignore_ascii_case(our_nickname);

            // Emit ChatMessage event (with is_from_self flag for sound handling)
            emit_event(
                self,
                EventType::ChatMessage,
                EventContext::new()
                    .with_connection_id(connection_id)
                    .with_username(&nickname)
                    .with_message(&message)
                    .with_is_from_self(is_from_self),
            );

            // Also emit ChatMention if our nickname is mentioned (only for others' messages)
            if !is_from_self
                && !our_nickname_lower.is_empty()
                && contains_word(&message.to_lowercase(), &our_nickname_lower)
            {
                emit_event(
                    self,
                    EventType::ChatMention,
                    EventContext::new()
                        .with_connection_id(connection_id)
                        .with_username(&nickname)
                        .with_message(&message),
                );
            }
        }

        let chat_message = ChatMessage::with_timestamp_and_status(
            nickname,
            message,
            chrono::Local::now(),
            is_admin,
            is_shared,
            action,
        );
        self.add_chat_message(connection_id, chat_message)
    }

    /// Handle chat topic change notification
    pub fn handle_chat_topic(
        &mut self,
        connection_id: usize,
        topic: String,
        username: String,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Build message first using references (before moving values)
        let message = if topic.is_empty() {
            t_args("msg-topic-cleared", &[("username", &username)])
        } else {
            t_args(
                "msg-topic-set",
                &[("username", &username), ("topic", &topic)],
            )
        };

        // Store values by moving (no clones needed)
        conn.chat_topic = if topic.is_empty() { None } else { Some(topic) };
        conn.chat_topic_set_by = if username.is_empty() {
            None
        } else {
            Some(username)
        };

        self.add_chat_message(connection_id, ChatMessage::system(message))
    }

    /// Handle chat topic update response
    pub fn handle_chat_topic_update_response(
        &mut self,
        connection_id: usize,
        success: bool,
        error: Option<String>,
    ) -> Task<Message> {
        let message = if success {
            ChatMessage::info(t("msg-topic-updated"))
        } else {
            ChatMessage::error(t_args(
                "err-failed-update-topic",
                &[("error", &error.unwrap_or_default())],
            ))
        };
        self.add_chat_message(connection_id, message)
    }
}

/// Check if a word appears in text with word boundaries
///
/// Returns true if `word` appears in `text` surrounded by non-alphanumeric
/// characters (or at string boundaries). This prevents "bob" from matching
/// "bobcat" or "kebob".
///
/// Note: For CJK text without spaces/punctuation, this may not match due to
/// word boundary requirements. This is a known limitation - proper CJK word
/// segmentation would require a tokenizer.
fn contains_word(text: &str, word: &str) -> bool {
    // Empty word matches nothing (guard against notification spam)
    if word.is_empty() {
        return false;
    }

    // Work with character indices to handle Unicode correctly
    let text_chars: Vec<char> = text.chars().collect();
    let word_chars: Vec<char> = word.chars().collect();
    let word_len = word_chars.len();

    if word_len > text_chars.len() {
        return false;
    }

    // Slide through text looking for word matches
    for start_idx in 0..=(text_chars.len() - word_len) {
        // Check if word matches at this position
        let matches = text_chars[start_idx..start_idx + word_len]
            .iter()
            .zip(word_chars.iter())
            .all(|(a, b)| a == b);

        if !matches {
            continue;
        }

        // Check character before match (or start of string)
        let valid_start = start_idx == 0 || !text_chars[start_idx - 1].is_alphanumeric();

        // Check character after match (or end of string)
        let end_idx = start_idx + word_len;
        let valid_end = end_idx >= text_chars.len() || !text_chars[end_idx].is_alphanumeric();

        if valid_start && valid_end {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contains_word_exact_match() {
        assert!(contains_word("hello bob", "bob"));
        assert!(contains_word("bob hello", "bob"));
        assert!(contains_word("hello bob hello", "bob"));
        assert!(contains_word("bob", "bob"));
    }

    #[test]
    fn test_contains_word_with_punctuation() {
        assert!(contains_word("hello, bob!", "bob"));
        assert!(contains_word("@bob: hey", "bob"));
        assert!(contains_word("hey bob, what's up?", "bob"));
        assert!(contains_word("(bob)", "bob"));
    }

    #[test]
    fn test_contains_word_rejects_substrings() {
        assert!(!contains_word("bobcat", "bob"));
        assert!(!contains_word("kebob", "bob"));
        assert!(!contains_word("thingamabob", "bob"));
        assert!(!contains_word("bobby", "bob"));
    }

    #[test]
    fn test_contains_word_empty_inputs() {
        assert!(!contains_word("hello", ""));
        assert!(!contains_word("", "bob"));
        assert!(!contains_word("", ""));
    }

    #[test]
    fn test_contains_word_case_sensitivity() {
        // Function expects pre-lowercased input
        assert!(contains_word("hello bob", "bob"));
        assert!(!contains_word("hello Bob", "bob")); // Case mismatch
    }

    #[test]
    fn test_contains_word_multiple_occurrences() {
        // First is substring, second is word
        assert!(contains_word("bobcat and bob", "bob"));
        // Both are substrings
        assert!(!contains_word("bobcat and bobby", "bob"));
    }

    #[test]
    fn test_contains_word_unicode_names() {
        // Unicode names with space boundaries work
        assert!(contains_word("hello 日本語 world", "日本語"));
        assert!(contains_word("日本語 hello", "日本語"));
        assert!(contains_word("hello 日本語", "日本語"));

        // With punctuation
        assert!(contains_word("@日本語: hello", "日本語"));
    }

    #[test]
    fn test_contains_word_unicode_limitation() {
        // Known limitation: CJK/Unicode without spaces may not match due to word boundary check
        // This is acceptable - CJK tokenization is a complex problem
        // Users can use @mentions or punctuation as workarounds

        // Hiragana, Katakana, Kanji, and ASCII letters are all alphanumeric
        // so adjacent characters prevent word boundary detection
        assert!(!contains_word("こんにちは日本語さん", "日本語")); // No boundary
        assert!(!contains_word("hello日本語world", "日本語")); // ASCII letters are alphanumeric too

        // With punctuation, boundaries are detected
        assert!(contains_word("こんにちは、日本語、さん", "日本語"));
        assert!(contains_word("hello,日本語,world", "日本語"));
        assert!(contains_word("hello 日本語 world", "日本語"));
    }
}
