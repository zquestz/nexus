//! Chat input command system
//!
//! This module provides IRC-style `/command` parsing and execution for the chat input.
//!
//! ## Available Commands
//!
//! | Command | Aliases | Permission | Description |
//! |---------|---------|------------|-------------|
//! | `/broadcast` | `/b` | `user_broadcast` | Send a broadcast to all users |
//! | `/clear` | | *none* | Clear chat history for current tab |
//! | `/focus` | `/f` | *none* | Focus server chat or a user's PM tab |
//! | `/help` | `/h`, `/?` | *none* | Show available commands |
//! | `/info` | `/i`, `/userinfo`, `/whois` | `user_info` | Show information about a user |
//! | `/kick` | `/k`, `/userkick` | `user_kick` | Kick a user from the server |
//! | `/list` | `/l`, `/userlist` | `user_list` | Show connected users |
//! | `/message` | `/m`, `/msg` | `user_message` | Send a message to a user |
//! | `/topic` | `/t`, `/chattopic` | `chat_topic` or `chat_topic_edit` | View or manage the chat topic |
//! | `/window` | `/w` | *none* | Manage chat tabs (list, close) |
//!
//! ## Special Syntax
//!
//! - `/` alone is a shortcut for `/help`
//! - `//text` - Escape sequence, sends `/text` as a regular message
//! - ` /command` - Leading space prevents command parsing
//!
//! ## Permissions
//!
//! Commands may require permissions to execute. If a user doesn't have the required
//! permission, the command is treated as unknown (same error as non-existent command).
//!
//! Unknown commands display an error in chat and are never sent to the server.

mod broadcast;
mod clear;
mod focus;
mod help;
mod list;
mod message;
mod topic;
mod userinfo;
mod userkick;
mod window;

use std::collections::HashMap;
use std::sync::LazyLock;

use crate::NexusApp;
use crate::i18n::t_args;
use crate::types::{ChatMessage, Message};
use crate::views::constants::{
    PERMISSION_CHAT_TOPIC, PERMISSION_CHAT_TOPIC_EDIT, PERMISSION_USER_BROADCAST,
    PERMISSION_USER_INFO, PERMISSION_USER_KICK, PERMISSION_USER_LIST, PERMISSION_USER_MESSAGE,
};
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
    /// Translation key for the usage
    pub usage_key: &'static str,
    /// Required permissions (any of these grants access, empty = always available)
    pub permissions: &'static [&'static str],
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
            usage_key: "cmd-broadcast-usage",
            permissions: &[PERMISSION_USER_BROADCAST],
        },
        handler: broadcast::execute,
    },
    CommandRegistration {
        info: CommandInfo {
            name: "clear",
            aliases: &[],
            description_key: "cmd-clear-desc",
            usage_key: "cmd-clear-usage",
            permissions: &[],
        },
        handler: clear::execute,
    },
    CommandRegistration {
        info: CommandInfo {
            name: "focus",
            aliases: &["f"],
            description_key: "cmd-focus-desc",
            usage_key: "cmd-focus-usage",
            permissions: &[],
        },
        handler: focus::execute,
    },
    CommandRegistration {
        info: CommandInfo {
            name: "help",
            aliases: &["h", "?"],
            description_key: "cmd-help-desc",
            usage_key: "cmd-help-usage",
            permissions: &[],
        },
        handler: help::execute,
    },
    CommandRegistration {
        info: CommandInfo {
            name: "info",
            aliases: &["i", "userinfo", "whois"],
            description_key: "cmd-userinfo-desc",
            usage_key: "cmd-userinfo-usage",
            permissions: &[PERMISSION_USER_INFO],
        },
        handler: userinfo::execute,
    },
    CommandRegistration {
        info: CommandInfo {
            name: "kick",
            aliases: &["k", "userkick"],
            description_key: "cmd-kick-desc",
            usage_key: "cmd-kick-usage",
            permissions: &[PERMISSION_USER_KICK],
        },
        handler: userkick::execute,
    },
    CommandRegistration {
        info: CommandInfo {
            name: "list",
            aliases: &["l", "userlist"],
            description_key: "cmd-list-desc",
            usage_key: "cmd-list-usage",
            permissions: &[PERMISSION_USER_LIST],
        },
        handler: list::execute,
    },
    CommandRegistration {
        info: CommandInfo {
            name: "message",
            aliases: &["m", "msg"],
            description_key: "cmd-message-desc",
            usage_key: "cmd-message-usage",
            permissions: &[PERMISSION_USER_MESSAGE],
        },
        handler: message::execute,
    },
    CommandRegistration {
        info: CommandInfo {
            name: "topic",
            aliases: &["t", "chattopic"],
            description_key: "cmd-topic-desc",
            usage_key: "cmd-topic-usage",
            permissions: &[PERMISSION_CHAT_TOPIC, PERMISSION_CHAT_TOPIC_EDIT],
        },
        handler: topic::execute,
    },
    CommandRegistration {
        info: CommandInfo {
            name: "window",
            aliases: &["w"],
            description_key: "cmd-window-desc",
            usage_key: "cmd-window-usage",
            permissions: &[],
        },
        handler: window::execute,
    },
];

/// Command dispatch map - maps command names and aliases to registration index
static COMMAND_MAP: LazyLock<HashMap<&'static str, usize>> = LazyLock::new(|| {
    let mut map = HashMap::new();

    for (index, reg) in COMMANDS.iter().enumerate() {
        map.insert(reg.info.name, index);
        for alias in reg.info.aliases {
            map.insert(alias, index);
        }
    }

    map
});

/// Check if user has any of the required permissions for a command
fn has_permission(is_admin: bool, user_permissions: &[String], required: &[&str]) -> bool {
    // Empty permissions = always allowed
    if required.is_empty() {
        return true;
    }

    // Admins have all permissions
    if is_admin {
        return true;
    }

    // Check if user has any of the required permissions
    required
        .iter()
        .any(|req| user_permissions.iter().any(|p| p == *req))
}

/// Get command info by name or alias (for /help <command>)
pub fn get_command_info(name: &str) -> Option<&'static CommandInfo> {
    COMMAND_MAP
        .get(name.to_lowercase().as_str())
        .map(|&index| &COMMANDS[index].info)
}

/// Get list of commands the user has permission to use (for /help display)
pub fn command_list_for_user(
    is_admin: bool,
    permissions: &[String],
) -> impl Iterator<Item = &'static CommandInfo> {
    COMMANDS.iter().filter_map(move |reg| {
        if has_permission(is_admin, permissions, reg.info.permissions) {
            Some(&reg.info)
        } else {
            None
        }
    })
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
///
/// If the user doesn't have permission for the command, it's treated as unknown.
pub fn execute_command(
    app: &mut NexusApp,
    connection_id: usize,
    command: CommandInvocation,
) -> Task<Message> {
    // Look up command registration
    if let Some(&index) = COMMAND_MAP.get(command.name.as_str()) {
        let reg = &COMMANDS[index];

        // Check permissions
        let (is_admin, permissions) = app
            .connections
            .get(&connection_id)
            .map(|conn| (conn.is_admin, conn.permissions.clone()))
            .unwrap_or((false, Vec::new()));

        if has_permission(is_admin, &permissions, reg.info.permissions) {
            return (reg.handler)(app, connection_id, &command.name, &command.args);
        }
    }

    // Unknown command or no permission - show error
    let error_msg = t_args("cmd-unknown", &[("command", &command.name)]);
    app.add_chat_message(connection_id, ChatMessage::error(error_msg))
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
        for (index, reg) in COMMANDS.iter().enumerate() {
            assert_eq!(
                COMMAND_MAP.get(reg.info.name),
                Some(&index),
                "Missing command: {}",
                reg.info.name
            );
            for alias in reg.info.aliases {
                assert_eq!(
                    COMMAND_MAP.get(alias),
                    Some(&index),
                    "Missing alias: {} for command {}",
                    alias,
                    reg.info.name
                );
            }
        }
    }

    #[test]
    fn test_has_permission_empty_always_allowed() {
        assert!(has_permission(false, &[], &[]));
        assert!(has_permission(true, &[], &[]));
    }

    #[test]
    fn test_has_permission_admin_always_allowed() {
        assert!(has_permission(true, &[], &["user_list"]));
        assert!(has_permission(true, &[], &["user_list", "user_info"]));
    }

    #[test]
    fn test_has_permission_user_with_permission() {
        let perms = vec!["user_list".to_string(), "chat_send".to_string()];
        assert!(has_permission(false, &perms, &["user_list"]));
        assert!(has_permission(false, &perms, &["chat_send"]));
        assert!(has_permission(false, &perms, &["user_list", "user_info"])); // any match
    }

    #[test]
    fn test_has_permission_user_without_permission() {
        let perms = vec!["chat_send".to_string()];
        assert!(!has_permission(false, &perms, &["user_list"]));
        assert!(!has_permission(false, &perms, &["user_list", "user_info"]));
    }

    #[test]
    fn test_get_command_info_by_name() {
        let info = get_command_info("help").expect("help command should exist");
        assert_eq!(info.name, "help");
    }

    #[test]
    fn test_get_command_info_by_alias() {
        let info = get_command_info("h").expect("h alias should exist");
        assert_eq!(info.name, "help");

        let info = get_command_info("?").expect("? alias should exist");
        assert_eq!(info.name, "help");
    }

    #[test]
    fn test_get_command_info_case_insensitive() {
        let info = get_command_info("HELP").expect("HELP should match help");
        assert_eq!(info.name, "help");

        let info = get_command_info("Help").expect("Help should match help");
        assert_eq!(info.name, "help");
    }

    #[test]
    fn test_get_command_info_unknown() {
        assert!(get_command_info("nonexistent").is_none());
    }

    #[test]
    fn test_command_list_for_user_admin_sees_all() {
        let commands: Vec<_> = command_list_for_user(true, &[]).collect();
        assert_eq!(commands.len(), COMMANDS.len());
    }

    #[test]
    fn test_command_list_for_user_no_perms_sees_public() {
        let commands: Vec<_> = command_list_for_user(false, &[]).collect();
        // Should see help and clear (no permissions required)
        assert!(commands.iter().any(|c| c.name == "help"));
        assert!(commands.iter().any(|c| c.name == "clear"));
        // Should not see permission-gated commands
        assert!(!commands.iter().any(|c| c.name == "kick"));
        assert!(!commands.iter().any(|c| c.name == "broadcast"));
    }

    #[test]
    fn test_command_list_for_user_with_permission() {
        let perms = vec!["user_list".to_string()];
        let commands: Vec<_> = command_list_for_user(false, &perms).collect();
        // Should see list command now
        assert!(commands.iter().any(|c| c.name == "list"));
        // Still shouldn't see kick
        assert!(!commands.iter().any(|c| c.name == "kick"));
    }
}
