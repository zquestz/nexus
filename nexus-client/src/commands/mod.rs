//! Chat input command system
//!
//! This module provides IRC-style `/command` parsing and execution for the chat input.
//!
//! ## Usage
//!
//! Commands are parsed from chat input when the message starts with `/`:
//! - `/help` - Show available commands
//! - `//text` - Escape, sends `/text` as a regular chat message
//!
//! Unknown commands display an error in chat and are never sent to the server.

mod broadcast;
mod clear;
mod help;
mod list;
mod message;
mod topic;
mod userinfo;
mod userkick;

use std::collections::HashMap;
use std::sync::LazyLock;

use crate::NexusApp;
use crate::i18n::t_args;
use crate::types::{ChatMessage, Message};
use iced::Task;

/// Command handler function type
/// Parameters: app, connection_id, invoked_name, args
type CommandHandler = fn(&mut NexusApp, usize, &str, &[String]) -> Task<Message>;

/// Registry of all available commands with their descriptions (for /help display)
pub struct CommandInfo {
    /// Primary command name
    pub name: &'static str,
    /// Aliases for the command
    pub aliases: &'static [&'static str],
    /// Translation key for the description
    pub description_key: &'static str,
}

/// Command registration entry - links metadata to handler
struct CommandRegistration {
    info: CommandInfo,
    handler: CommandHandler,
}

/// All registered commands (alphabetical order)
static COMMANDS: &[CommandRegistration] = &[
    CommandRegistration {
        info: CommandInfo {
            name: "broadcast",
            aliases: &["b"],
            description_key: "cmd-broadcast-desc",
        },
        handler: broadcast::execute,
    },
    CommandRegistration {
        info: CommandInfo {
            name: "clear",
            aliases: &[],
            description_key: "cmd-clear-desc",
        },
        handler: clear::execute,
    },
    CommandRegistration {
        info: CommandInfo {
            name: "help",
            aliases: &["h", "?"],
            description_key: "cmd-help-desc",
        },
        handler: help::execute,
    },
    CommandRegistration {
        info: CommandInfo {
            name: "info",
            aliases: &["i", "userinfo", "whois"],
            description_key: "cmd-userinfo-desc",
        },
        handler: userinfo::execute,
    },
    CommandRegistration {
        info: CommandInfo {
            name: "kick",
            aliases: &["k", "userkick"],
            description_key: "cmd-kick-desc",
        },
        handler: userkick::execute,
    },
    CommandRegistration {
        info: CommandInfo {
            name: "list",
            aliases: &["l", "userlist"],
            description_key: "cmd-list-desc",
        },
        handler: list::execute,
    },
    CommandRegistration {
        info: CommandInfo {
            name: "message",
            aliases: &["m", "msg"],
            description_key: "cmd-message-desc",
        },
        handler: message::execute,
    },
    CommandRegistration {
        info: CommandInfo {
            name: "topic",
            aliases: &["t", "chattopic"],
            description_key: "cmd-topic-desc",
        },
        handler: topic::execute,
    },
];

/// Command dispatch map - maps command names and aliases to their handlers
static COMMAND_MAP: LazyLock<HashMap<&'static str, CommandHandler>> = LazyLock::new(|| {
    let mut map = HashMap::new();

    for reg in COMMANDS {
        map.insert(reg.info.name, reg.handler);
        for alias in reg.info.aliases {
            map.insert(alias, reg.handler);
        }
    }

    map
});

/// Get list of all commands for /help display
pub fn command_list() -> impl Iterator<Item = &'static CommandInfo> {
    COMMANDS.iter().map(|reg| &reg.info)
}

/// Result of parsing chat input
pub enum ParseResult {
    /// Input is a command that should be executed
    Command(CommandInvocation),
    /// Input is a regular message that should be sent to the server
    Message(String),
    /// Input is empty (should be ignored)
    Empty,
}

/// A parsed command invocation
pub struct CommandInvocation {
    /// Command name (lowercase, without the leading slash)
    pub name: String,
    /// Arguments after the command name
    pub args: Vec<String>,
}

/// Parse chat input into a command or regular message
///
/// # Rules
/// - `/command args` → Command { name: "command", args: ["args"] }
/// - `//text` → Message("/text") (escape sequence, preserves rest of input)
/// - ` /command` → Message(" /command") (leading space prevents command parsing)
/// - `regular text` → Message("regular text")
/// - `` or whitespace only → Empty
pub fn parse_input(input: &str) -> ParseResult {
    // Check if input is empty or whitespace-only
    if input.trim().is_empty() {
        return ParseResult::Empty;
    }

    // Commands must start with `/` at position 0 (no leading whitespace)
    if let Some(rest) = input.strip_prefix('/') {
        // Check for escape sequence: `//` sends original input without first `/`
        if rest.starts_with('/') {
            return ParseResult::Message(rest.to_string());
        }

        // Parse as command
        let parts: Vec<&str> = rest.split_whitespace().collect();

        // "/" by itself is a shortcut for "/help"
        let (name, args) = if parts.is_empty() {
            ("help".to_string(), Vec::new())
        } else {
            let name = parts[0].to_lowercase();
            let args = parts[1..].iter().map(|s| (*s).to_string()).collect();
            (name, args)
        };

        ParseResult::Command(CommandInvocation { name, args })
    } else {
        // Not a command - send as-is (preserving original input)
        ParseResult::Message(input.to_string())
    }
}

/// Execute a command and return the resulting task
///
/// Commands are executed client-side and may:
/// - Add messages to the chat (info, error, etc.)
/// - Trigger server requests
/// - Modify client state
pub fn execute_command(
    app: &mut NexusApp,
    connection_id: usize,
    command: CommandInvocation,
) -> Task<Message> {
    if let Some(handler) = COMMAND_MAP.get(command.name.as_str()) {
        handler(app, connection_id, &command.name, &command.args)
    } else {
        // Unknown command - show error
        let error_msg = t_args("cmd-unknown", &[("command", &command.name)]);
        app.add_chat_message(connection_id, ChatMessage::error(error_msg))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty_input() {
        assert!(matches!(parse_input(""), ParseResult::Empty));
        assert!(matches!(parse_input("   "), ParseResult::Empty));
    }

    #[test]
    fn test_parse_slash_alone_is_help() {
        match parse_input("/") {
            ParseResult::Command(cmd) => {
                assert_eq!(cmd.name, "help");
                assert!(cmd.args.is_empty());
            }
            _ => panic!("Expected Command"),
        }
    }

    #[test]
    fn test_parse_regular_message() {
        match parse_input("hello world") {
            ParseResult::Message(msg) => assert_eq!(msg, "hello world"),
            _ => panic!("Expected Message"),
        }
    }

    #[test]
    fn test_parse_message_with_leading_space() {
        // Leading space should prevent command parsing
        match parse_input(" /help") {
            ParseResult::Message(msg) => assert_eq!(msg, " /help"),
            _ => panic!("Expected Message"),
        }
    }

    #[test]
    fn test_parse_message_preserves_whitespace() {
        match parse_input("  hello  world  ") {
            ParseResult::Message(msg) => assert_eq!(msg, "  hello  world  "),
            _ => panic!("Expected Message"),
        }
    }

    #[test]
    fn test_parse_command() {
        match parse_input("/help") {
            ParseResult::Command(cmd) => {
                assert_eq!(cmd.name, "help");
                assert!(cmd.args.is_empty());
            }
            _ => panic!("Expected Command"),
        }
    }

    #[test]
    fn test_parse_command_with_args() {
        match parse_input("/test arg1 arg2") {
            ParseResult::Command(cmd) => {
                assert_eq!(cmd.name, "test");
                assert_eq!(cmd.args, vec!["arg1", "arg2"]);
            }
            _ => panic!("Expected Command"),
        }
    }

    #[test]
    fn test_parse_command_case_insensitive() {
        match parse_input("/HELP") {
            ParseResult::Command(cmd) => {
                assert_eq!(cmd.name, "help");
            }
            _ => panic!("Expected Command"),
        }
    }

    #[test]
    fn test_parse_escape_sequence() {
        match parse_input("//shrug") {
            ParseResult::Message(msg) => assert_eq!(msg, "/shrug"),
            _ => panic!("Expected Message"),
        }
    }

    #[test]
    fn test_parse_escape_with_space() {
        match parse_input("//me does something") {
            ParseResult::Message(msg) => assert_eq!(msg, "/me does something"),
            _ => panic!("Expected Message"),
        }
    }

    #[test]
    fn test_parse_escape_preserves_formatting() {
        // Escape should preserve everything after the first /
        match parse_input("//  spaced  out  ") {
            ParseResult::Message(msg) => assert_eq!(msg, "/  spaced  out  "),
            _ => panic!("Expected Message"),
        }
    }

    #[test]
    fn test_command_map_contains_all_aliases() {
        // Verify all commands and aliases are in COMMAND_MAP
        for info in command_list() {
            assert!(
                COMMAND_MAP.contains_key(info.name),
                "Missing command: {}",
                info.name
            );
            for alias in info.aliases {
                assert!(
                    COMMAND_MAP.contains_key(alias),
                    "Missing alias: {} for command {}",
                    alias,
                    info.name
                );
            }
        }
    }
}
