//! Chat input command system
//!
//! This module provides IRC-style `/command` parsing and execution for the chat input.
//!
//! ## Available Commands
//!
//! | Command | Aliases | Permission | Description |
//! |---------|---------|------------|-------------|
//! | `/away` | `/a` | *none* | Set yourself as away |
//! | `/back` | `/b` | *none* | Clear away status |
//! | `/broadcast` | `/bc` | `user_broadcast` | Send a broadcast to all users |
//! | `/clear` | | *none* | Clear chat history for current tab |
//! | `/focus` | `/f` | *none* | Focus server chat or a user's PM tab |
//! | `/help` | `/h`, `/?` | *none* | Show available commands |
//! | `/info` | `/i`, `/userinfo`, `/whois` | `user_info` | Show information about a user |
//! | `/kick` | `/k`, `/userkick` | `user_kick` | Kick a user from the server |
//! | `/list` | `/l`, `/userlist` | `user_list` | Show connected users |
//! | `/me` | | *none* | Send an action message |
//! | `/message` | `/m`, `/msg` | `user_message` | Send a message to a user |
//! | `/sinfo` | `/si`, `/serverinfo` | *none* | Show server information |
//! | `/status` | `/s` | *none* | Set or clear your status message |
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

mod away;
mod back;
mod broadcast;
mod clear;
mod focus;
mod help;
mod list;
mod me;
mod message;
mod server_info;
mod status;
mod topic;
mod user_info;
mod user_kick;
mod window;

use std::collections::HashMap;
use std::sync::LazyLock;

use iced::Task;
use nexus_common::protocol::ChatAction;

use crate::NexusApp;
use crate::i18n::t_args;
use crate::types::{ChatMessage, Message};
use crate::views::constants::{
    PERMISSION_CHAT_TOPIC, PERMISSION_CHAT_TOPIC_EDIT, PERMISSION_USER_BROADCAST,
    PERMISSION_USER_INFO, PERMISSION_USER_KICK, PERMISSION_USER_LIST, PERMISSION_USER_MESSAGE,
};

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
            name: "away",
            aliases: &["a"],
            description_key: "cmd-away-desc",
            usage_key: "cmd-away-usage",
            permissions: &[],
        },
        handler: away::execute,
    },
    CommandRegistration {
        info: CommandInfo {
            name: "back",
            aliases: &["b"],
            description_key: "cmd-back-desc",
            usage_key: "cmd-back-usage",
            permissions: &[],
        },
        handler: back::execute,
    },
    CommandRegistration {
        info: CommandInfo {
            name: "broadcast",
            aliases: &["bc"],
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
        handler: user_info::execute,
    },
    CommandRegistration {
        info: CommandInfo {
            name: "kick",
            aliases: &["k", "userkick"],
            description_key: "cmd-kick-desc",
            usage_key: "cmd-kick-usage",
            permissions: &[PERMISSION_USER_KICK],
        },
        handler: user_kick::execute,
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
            name: "me",
            aliases: &[],
            description_key: "cmd-me-desc",
            usage_key: "cmd-me-usage",
            permissions: &[],
        },
        handler: me::execute,
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
            name: "sinfo",
            aliases: &["si", "serverinfo"],
            description_key: "cmd-serverinfo-desc",
            usage_key: "cmd-serverinfo-usage",
            permissions: &[],
        },
        handler: server_info::execute,
    },
    CommandRegistration {
        info: CommandInfo {
            name: "status",
            aliases: &["s"],
            description_key: "cmd-status-desc",
            usage_key: "cmd-status-usage",
            permissions: &[],
        },
        handler: status::execute,
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

/// Get command info by name or alias (for /help <command>)
pub fn get_command_info(name: &str) -> Option<&'static CommandInfo> {
    COMMAND_MAP
        .get(name.to_lowercase().as_str())
        .map(|&index| &COMMANDS[index].info)
}

/// Get list of commands the user has permission to use (for /help display)
pub(crate) fn command_list_for_permissions(
    is_admin: bool,
    permissions: &[String],
) -> impl Iterator<Item = &'static CommandInfo> {
    COMMANDS.iter().filter_map(move |reg| {
        let has_permission = reg.info.permissions.is_empty()
            || is_admin
            || reg
                .info
                .permissions
                .iter()
                .any(|req| permissions.iter().any(|p| p == *req));
        if has_permission {
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
    /// The ChatAction indicates normal or /me action format
    Message(String, ChatAction),
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
            return ParseResult::Message(rest.to_string(), ChatAction::Normal);
        }

        // Try to match a command
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
        ParseResult::Message(input.to_string(), ChatAction::Normal)
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
        let has_permission = app
            .connections
            .get(&connection_id)
            .is_some_and(|conn| conn.has_any_permission(reg.info.permissions));

        if has_permission {
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
    fn test_parse_me_command() {
        match parse_input("/me waves hello") {
            ParseResult::Command(cmd) => {
                assert_eq!(cmd.name, "me");
                assert_eq!(cmd.args, vec!["waves", "hello"]);
            }
            _ => panic!("Expected Command"),
        }
    }

    #[test]
    fn test_parse_me_case_insensitive() {
        match parse_input("/ME waves") {
            ParseResult::Command(cmd) => {
                assert_eq!(cmd.name, "me");
                assert_eq!(cmd.args, vec!["waves"]);
            }
            _ => panic!("Expected Command"),
        }
    }

    #[test]
    fn test_parse_me_no_args() {
        match parse_input("/me") {
            ParseResult::Command(cmd) => {
                assert_eq!(cmd.name, "me");
                assert!(cmd.args.is_empty());
            }
            _ => panic!("Expected Command"),
        }
    }

    #[test]
    fn test_parse_regular_message() {
        match parse_input("hello world") {
            ParseResult::Message(msg, action) => {
                assert_eq!(msg, "hello world");
                assert_eq!(action, ChatAction::Normal);
            }
            _ => panic!("Expected Message"),
        }
    }

    #[test]
    fn test_parse_message_with_leading_space() {
        // Leading space should prevent command parsing
        match parse_input(" /help") {
            ParseResult::Message(msg, action) => {
                assert_eq!(msg, " /help");
                assert_eq!(action, ChatAction::Normal);
            }
            _ => panic!("Expected Message"),
        }
    }

    #[test]
    fn test_parse_message_preserves_whitespace() {
        match parse_input("  hello  world  ") {
            ParseResult::Message(msg, action) => {
                assert_eq!(msg, "  hello  world  ");
                assert_eq!(action, ChatAction::Normal);
            }
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
            ParseResult::Message(msg, action) => {
                assert_eq!(msg, "/shrug");
                assert_eq!(action, ChatAction::Normal);
            }
            _ => panic!("Expected Message"),
        }
    }

    #[test]
    fn test_parse_escape_with_space() {
        match parse_input("//me does something") {
            ParseResult::Message(msg, action) => {
                assert_eq!(msg, "/me does something");
                assert_eq!(action, ChatAction::Normal);
            }
            _ => panic!("Expected Message"),
        }
    }

    #[test]
    fn test_parse_escape_preserves_formatting() {
        // Escape should preserve everything after the first /
        match parse_input("//  spaced  out  ") {
            ParseResult::Message(msg, action) => {
                assert_eq!(msg, "/  spaced  out  ");
                assert_eq!(action, ChatAction::Normal);
            }
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
                    "Missing alias: {} for command: {}",
                    alias,
                    reg.info.name
                );
            }
        }
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
        let commands: Vec<_> = command_list_for_permissions(true, &[]).collect();
        assert_eq!(commands.len(), COMMANDS.len());
    }

    #[test]
    fn test_command_list_for_user_no_perms_sees_public() {
        let commands: Vec<_> = command_list_for_permissions(false, &[]).collect();
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
        let commands: Vec<_> = command_list_for_permissions(false, &perms).collect();
        // Should see list command now
        assert!(commands.iter().any(|c| c.name == "list"));
        // Still shouldn't see kick
        assert!(!commands.iter().any(|c| c.name == "kick"));
    }
}
