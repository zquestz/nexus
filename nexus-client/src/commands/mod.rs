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
//! | `/ban` | | `ban_create` | Ban a user by IP, CIDR range, or nickname |
//! | `/bans` | `/banlist` | `ban_list` | List active bans |
//! | `/broadcast` | `/bc` | `user_broadcast` | Send a broadcast to all users |
//! | `/channels` | `/ch` | `chat_list` | List available channels |
//! | `/clear` | | *none* | Clear chat history for current tab |
//! | `/focus` | `/f` | *none* | Focus server chat or a user's message tab |
//! | `/help` | `/h`, `/?` | *none* | Show available commands |
//! | `/info` | `/i`, `/userinfo`, `/whois` | `user_info` | Show information about a user |
//! | `/join` | `/j` | `chat_join` | Join or create a channel |
//! | `/kick` | `/k`, `/userkick` | `user_kick` | Kick a user from the server |
//! | `/leave` | `/part` | *none* | Leave a channel |
//! | `/list` | `/l`, `/userlist` | `user_list` | Show connected users |
//! | `/me` | | `chat_send` | Send an action message |
//! | `/message` | `/m`, `/msg` | `user_message` | Send a message to a user |
//! | `/ping` | | *none* | Measure latency to server |
//! | `/sinfo` | `/si`, `/serverinfo` | *none* | Show server information |
//! | `/status` | `/s` | *none* | Set or clear your status message |
//! | `/topic` | `/t`, `/chattopic` | `chat_topic` or `chat_topic_edit` | View or manage the chat topic |
//! | `/unban` | | `ban_delete` | Remove an IP ban |
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
mod ban;
mod bans;
mod broadcast;
mod channels;
mod clear;
mod duration;
mod focus;
mod help;
mod join;
mod leave;
mod list;
mod me;
mod message;
mod ping;
mod reindex;
mod secret;
mod server_info;
mod status;
mod topic;
mod trust;
mod trusted;
mod unban;
mod untrust;
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
    PERMISSION_BAN_CREATE, PERMISSION_BAN_DELETE, PERMISSION_BAN_LIST, PERMISSION_CHAT_JOIN,
    PERMISSION_CHAT_LIST, PERMISSION_CHAT_SECRET, PERMISSION_CHAT_SEND, PERMISSION_CHAT_TOPIC,
    PERMISSION_CHAT_TOPIC_EDIT, PERMISSION_FILE_REINDEX, PERMISSION_TRUST_CREATE,
    PERMISSION_TRUST_DELETE, PERMISSION_TRUST_LIST, PERMISSION_USER_BROADCAST,
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
            aliases: &["a", "afk"],
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
            name: "ban",
            aliases: &[],
            description_key: "cmd-ban-desc",
            usage_key: "cmd-ban-usage",
            permissions: &[PERMISSION_BAN_CREATE],
        },
        handler: ban::execute,
    },
    CommandRegistration {
        info: CommandInfo {
            name: "bans",
            aliases: &["banlist"],
            description_key: "cmd-bans-desc",
            usage_key: "cmd-bans-usage",
            permissions: &[PERMISSION_BAN_LIST],
        },
        handler: bans::execute,
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
            name: "channels",
            aliases: &["ch"],
            description_key: "cmd-channels-desc",
            usage_key: "cmd-channels-usage",
            permissions: &[PERMISSION_CHAT_LIST],
        },
        handler: channels::execute,
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
            name: "join",
            aliases: &["j"],
            description_key: "cmd-join-desc",
            usage_key: "cmd-join-usage",
            permissions: &[PERMISSION_CHAT_JOIN],
        },
        handler: join::execute,
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
            name: "leave",
            aliases: &["part"],
            description_key: "cmd-leave-desc",
            usage_key: "cmd-leave-usage",
            permissions: &[],
        },
        handler: leave::execute,
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
            permissions: &[PERMISSION_CHAT_SEND],
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
            name: "ping",
            aliases: &[],
            description_key: "cmd-ping-desc",
            usage_key: "cmd-ping-usage",
            permissions: &[],
        },
        handler: ping::execute,
    },
    CommandRegistration {
        info: CommandInfo {
            name: "reindex",
            aliases: &[],
            description_key: "cmd-reindex-desc",
            usage_key: "cmd-reindex-usage",
            permissions: &[PERMISSION_FILE_REINDEX],
        },
        handler: reindex::execute,
    },
    CommandRegistration {
        info: CommandInfo {
            name: "secret",
            aliases: &[],
            description_key: "cmd-secret-desc",
            usage_key: "cmd-secret-usage",
            permissions: &[PERMISSION_CHAT_SECRET],
        },
        handler: secret::execute,
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
            name: "trust",
            aliases: &[],
            description_key: "cmd-trust-desc",
            usage_key: "cmd-trust-usage",
            permissions: &[PERMISSION_TRUST_CREATE],
        },
        handler: trust::execute,
    },
    CommandRegistration {
        info: CommandInfo {
            name: "trusted",
            aliases: &["trustlist"],
            description_key: "cmd-trusted-desc",
            usage_key: "cmd-trusted-usage",
            permissions: &[PERMISSION_TRUST_LIST],
        },
        handler: trusted::execute,
    },
    CommandRegistration {
        info: CommandInfo {
            name: "unban",
            aliases: &[],
            description_key: "cmd-unban-desc",
            usage_key: "cmd-unban-usage",
            permissions: &[PERMISSION_BAN_DELETE],
        },
        handler: unban::execute,
    },
    CommandRegistration {
        info: CommandInfo {
            name: "untrust",
            aliases: &[],
            description_key: "cmd-untrust-desc",
            usage_key: "cmd-untrust-usage",
            permissions: &[PERMISSION_TRUST_DELETE],
        },
        handler: untrust::execute,
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

/// Complete a command name. Returns matching command names or None if no matches.
pub fn complete_command(
    prefix: &str,
    is_admin: bool,
    permissions: &[String],
) -> Option<Vec<String>> {
    let prefix_lower = prefix.to_lowercase();
    let matches: Vec<String> = command_names_for_completion(is_admin, permissions)
        .into_iter()
        .filter(|cmd| cmd.to_lowercase().starts_with(&prefix_lower))
        .collect();

    if matches.is_empty() {
        None
    } else {
        // Already sorted by command_names_for_completion
        Some(matches)
    }
}

/// Complete a channel name. Returns matching channel names or None if no matches.
pub fn complete_channel(prefix: &str, channels: &[String]) -> Option<Vec<String>> {
    let prefix_lower = prefix.to_lowercase();
    let mut matches: Vec<String> = channels
        .iter()
        .filter(|name| name.to_lowercase().starts_with(&prefix_lower))
        .cloned()
        .collect();

    if matches.is_empty() {
        None
    } else {
        matches.sort_unstable_by_key(|a| a.to_lowercase());
        Some(matches)
    }
}

/// Complete a nickname. Returns matching nicknames or None if no matches.
pub fn complete_nickname<T, F>(prefix: &str, users: &[T], get_nickname: F) -> Option<Vec<String>>
where
    F: Fn(&T) -> &str,
{
    let prefix_lower = prefix.to_lowercase();
    let mut matches: Vec<String> = users
        .iter()
        .filter(|u| get_nickname(u).to_lowercase().starts_with(&prefix_lower))
        .map(|u| get_nickname(u).to_string())
        .collect();

    if matches.is_empty() {
        None
    } else {
        matches.sort_unstable_by_key(|a| a.to_lowercase());
        Some(matches)
    }
}

/// Find the last word in a string, returning (start_position, word_slice).
///
/// This correctly handles multi-byte Unicode whitespace characters by using
/// char boundaries rather than assuming single-byte whitespace.
///
/// Returns (0, input) if no whitespace is found (entire input is one word).
/// Returns (len, "") if input ends with whitespace.
pub fn last_word(input: &str) -> (usize, &str) {
    match input.rfind(char::is_whitespace) {
        Some(i) => {
            // Skip past the whitespace character (handles multi-byte whitespace)
            let ws_char = input[i..].chars().next().unwrap();
            let start = i + ws_char.len_utf8();
            (start, &input[start..])
        }
        None => (0, input),
    }
}

/// Get list of command names (including aliases) the user has permission to use (for tab completion)
pub fn command_names_for_completion(is_admin: bool, permissions: &[String]) -> Vec<String> {
    let mut names = Vec::new();
    for reg in COMMANDS.iter() {
        let has_permission = reg.info.permissions.is_empty()
            || is_admin
            || reg
                .info
                .permissions
                .iter()
                .any(|req| permissions.iter().any(|p| p == *req));
        if has_permission {
            names.push(reg.info.name.to_string());
            for alias in reg.info.aliases {
                names.push((*alias).to_string());
            }
        }
    }
    names.sort_unstable_by_key(|a| a.to_lowercase());
    names
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
    app.add_active_tab_message(connection_id, ChatMessage::error(error_msg))
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

    // =========================================================================
    // Tab Completion Tests
    // =========================================================================

    // --- last_word tests ---

    #[test]
    fn test_last_word_empty_string() {
        let (pos, word) = last_word("");
        assert_eq!(pos, 0);
        assert_eq!(word, "");
    }

    #[test]
    fn test_last_word_single_word() {
        let (pos, word) = last_word("hello");
        assert_eq!(pos, 0);
        assert_eq!(word, "hello");
    }

    #[test]
    fn test_last_word_two_words() {
        let (pos, word) = last_word("hello world");
        assert_eq!(pos, 6);
        assert_eq!(word, "world");
    }

    #[test]
    fn test_last_word_multiple_words() {
        let (pos, word) = last_word("one two three four");
        assert_eq!(pos, 14);
        assert_eq!(word, "four");
    }

    #[test]
    fn test_last_word_trailing_space() {
        let (pos, word) = last_word("hello ");
        assert_eq!(pos, 6);
        assert_eq!(word, "");
    }

    #[test]
    fn test_last_word_multiple_trailing_spaces() {
        let (pos, word) = last_word("hello   ");
        assert_eq!(pos, 8);
        assert_eq!(word, "");
    }

    #[test]
    fn test_last_word_leading_space() {
        let (pos, word) = last_word(" hello");
        assert_eq!(pos, 1);
        assert_eq!(word, "hello");
    }

    #[test]
    fn test_last_word_multiple_spaces_between() {
        let (pos, word) = last_word("hello   world");
        assert_eq!(pos, 8);
        assert_eq!(word, "world");
    }

    #[test]
    fn test_last_word_tab_character() {
        let (pos, word) = last_word("hello\tworld");
        assert_eq!(pos, 6);
        assert_eq!(word, "world");
    }

    #[test]
    fn test_last_word_newline() {
        let (pos, word) = last_word("hello\nworld");
        assert_eq!(pos, 6);
        assert_eq!(word, "world");
    }

    #[test]
    fn test_last_word_unicode_content() {
        let (pos, word) = last_word("hello 日本語");
        assert_eq!(pos, 6);
        assert_eq!(word, "日本語");
    }

    #[test]
    fn test_last_word_unicode_whitespace() {
        // Em space (U+2003) is a multi-byte whitespace character
        let (pos, word) = last_word("hello\u{2003}world");
        assert_eq!(pos, 8); // 5 bytes for "hello" + 3 bytes for em space
        assert_eq!(word, "world");
    }

    #[test]
    fn test_last_word_channel_prefix() {
        let (pos, word) = last_word("/join #nexus");
        assert_eq!(pos, 6);
        assert_eq!(word, "#nexus");
    }

    #[test]
    fn test_last_word_partial_channel() {
        let (pos, word) = last_word("/join #nex");
        assert_eq!(pos, 6);
        assert_eq!(word, "#nex");
    }

    #[test]
    fn test_last_word_just_hash() {
        let (pos, word) = last_word("/join #");
        assert_eq!(pos, 6);
        assert_eq!(word, "#");
    }

    // --- complete_channel tests ---

    #[test]
    fn test_complete_channel_empty_prefix() {
        // Empty prefix matches all channels (everything starts with "")
        let channels = vec!["#nexus".to_string(), "#support".to_string()];
        let result = complete_channel("", &channels);
        assert!(result.is_some());
        let matches = result.unwrap();
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn test_complete_channel_hash_only() {
        let channels = vec!["#nexus".to_string(), "#support".to_string()];
        let result = complete_channel("#", &channels);
        assert!(result.is_some());
        let matches = result.unwrap();
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0], "#nexus");
        assert_eq!(matches[1], "#support");
    }

    #[test]
    fn test_complete_channel_partial_match() {
        let channels = vec![
            "#nexus".to_string(),
            "#news".to_string(),
            "#support".to_string(),
        ];
        let result = complete_channel("#ne", &channels);
        assert!(result.is_some());
        let matches = result.unwrap();
        assert_eq!(matches.len(), 2);
        assert!(matches.contains(&"#nexus".to_string()));
        assert!(matches.contains(&"#news".to_string()));
    }

    #[test]
    fn test_complete_channel_exact_match() {
        let channels = vec!["#nexus".to_string(), "#support".to_string()];
        let result = complete_channel("#nexus", &channels);
        assert!(result.is_some());
        let matches = result.unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0], "#nexus");
    }

    #[test]
    fn test_complete_channel_no_match() {
        let channels = vec!["#nexus".to_string(), "#support".to_string()];
        let result = complete_channel("#xyz", &channels);
        assert!(result.is_none());
    }

    #[test]
    fn test_complete_channel_case_insensitive() {
        let channels = vec!["#Nexus".to_string(), "#SUPPORT".to_string()];
        let result = complete_channel("#nex", &channels);
        assert!(result.is_some());
        let matches = result.unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0], "#Nexus"); // Preserves original casing
    }

    #[test]
    fn test_complete_channel_uppercase_prefix() {
        let channels = vec!["#nexus".to_string(), "#support".to_string()];
        let result = complete_channel("#NEX", &channels);
        assert!(result.is_some());
        let matches = result.unwrap();
        assert_eq!(matches[0], "#nexus");
    }

    #[test]
    fn test_complete_channel_sorted_output() {
        let channels = vec![
            "#zebra".to_string(),
            "#alpha".to_string(),
            "#middle".to_string(),
        ];
        let result = complete_channel("#", &channels);
        assert!(result.is_some());
        let matches = result.unwrap();
        assert_eq!(matches[0], "#alpha");
        assert_eq!(matches[1], "#middle");
        assert_eq!(matches[2], "#zebra");
    }

    #[test]
    fn test_complete_channel_empty_list() {
        let channels: Vec<String> = vec![];
        let result = complete_channel("#nex", &channels);
        assert!(result.is_none());
    }

    #[test]
    fn test_complete_channel_unicode_name() {
        let channels = vec!["#日本語".to_string(), "#nexus".to_string()];
        let result = complete_channel("#日", &channels);
        assert!(result.is_some());
        let matches = result.unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0], "#日本語");
    }

    // --- complete_nickname tests ---

    #[test]
    fn test_complete_nickname_empty_prefix() {
        let users = vec!["alice", "bob", "charlie"];
        let result = complete_nickname("", &users, |u| u);
        assert!(result.is_some());
        let matches = result.unwrap();
        assert_eq!(matches.len(), 3);
    }

    #[test]
    fn test_complete_nickname_partial_match() {
        let users = vec!["alice", "alex", "bob"];
        let result = complete_nickname("al", &users, |u| u);
        assert!(result.is_some());
        let matches = result.unwrap();
        assert_eq!(matches.len(), 2);
        assert!(matches.contains(&"alice".to_string()));
        assert!(matches.contains(&"alex".to_string()));
    }

    #[test]
    fn test_complete_nickname_single_match() {
        let users = vec!["alice", "bob", "charlie"];
        let result = complete_nickname("bo", &users, |u| u);
        assert!(result.is_some());
        let matches = result.unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0], "bob");
    }

    #[test]
    fn test_complete_nickname_no_match() {
        let users = vec!["alice", "bob", "charlie"];
        let result = complete_nickname("xyz", &users, |u| u);
        assert!(result.is_none());
    }

    #[test]
    fn test_complete_nickname_case_insensitive() {
        let users = vec!["Alice", "BOB", "Charlie"];
        let result = complete_nickname("ali", &users, |u| u);
        assert!(result.is_some());
        let matches = result.unwrap();
        assert_eq!(matches[0], "Alice"); // Preserves original casing
    }

    #[test]
    fn test_complete_nickname_uppercase_prefix() {
        let users = vec!["alice", "bob"];
        let result = complete_nickname("ALI", &users, |u| u);
        assert!(result.is_some());
        let matches = result.unwrap();
        assert_eq!(matches[0], "alice");
    }

    #[test]
    fn test_complete_nickname_sorted_output() {
        let users = vec!["zebra", "alice", "middle"];
        let result = complete_nickname("", &users, |u| u);
        assert!(result.is_some());
        let matches = result.unwrap();
        assert_eq!(matches[0], "alice");
        assert_eq!(matches[1], "middle");
        assert_eq!(matches[2], "zebra");
    }

    #[test]
    fn test_complete_nickname_empty_list() {
        let users: Vec<&str> = vec![];
        let result = complete_nickname("ali", &users, |u| u);
        assert!(result.is_none());
    }

    #[test]
    fn test_complete_nickname_unicode() {
        let users = vec!["日本語", "alice"];
        let result = complete_nickname("日", &users, |u| u);
        assert!(result.is_some());
        let matches = result.unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0], "日本語");
    }

    #[test]
    fn test_complete_nickname_with_struct() {
        struct User {
            nickname: String,
            #[allow(dead_code)]
            id: u32,
        }
        let users = vec![
            User {
                nickname: "alice".to_string(),
                id: 1,
            },
            User {
                nickname: "bob".to_string(),
                id: 2,
            },
        ];
        let result = complete_nickname("ali", &users, |u| &u.nickname);
        assert!(result.is_some());
        let matches = result.unwrap();
        assert_eq!(matches[0], "alice");
    }

    // --- complete_command tests ---

    #[test]
    fn test_complete_command_empty_prefix_admin() {
        let result = complete_command("", true, &[]);
        assert!(result.is_some());
        let matches = result.unwrap();
        // Admin should see all commands and aliases
        assert!(matches.len() > 10);
    }

    #[test]
    fn test_complete_command_empty_prefix_no_perms() {
        let result = complete_command("", false, &[]);
        assert!(result.is_some());
        let matches = result.unwrap();
        // Should at least see help, clear, etc.
        assert!(matches.iter().any(|c| c == "help"));
        assert!(matches.iter().any(|c| c == "clear"));
    }

    #[test]
    fn test_complete_command_partial_match() {
        let result = complete_command("he", true, &[]);
        assert!(result.is_some());
        let matches = result.unwrap();
        assert!(matches.iter().any(|c| c == "help"));
    }

    #[test]
    fn test_complete_command_no_match() {
        let result = complete_command("xyz", true, &[]);
        assert!(result.is_none());
    }

    #[test]
    fn test_complete_command_case_insensitive() {
        let result = complete_command("HE", true, &[]);
        assert!(result.is_some());
        let matches = result.unwrap();
        assert!(matches.iter().any(|c| c == "help"));
    }

    #[test]
    fn test_complete_command_includes_aliases() {
        let result = complete_command("", true, &[]);
        assert!(result.is_some());
        let matches = result.unwrap();
        // "h" and "?" are aliases for help
        assert!(matches.iter().any(|c| c == "h"));
        assert!(matches.iter().any(|c| c == "?"));
    }

    #[test]
    fn test_complete_command_alias_match() {
        let result = complete_command("h", true, &[]);
        assert!(result.is_some());
        let matches = result.unwrap();
        // Should match "h" alias and "help" command
        assert!(matches.iter().any(|c| c == "h"));
        assert!(matches.iter().any(|c| c == "help"));
    }

    #[test]
    fn test_complete_command_permission_gated() {
        // Without permissions, shouldn't see kick
        let result = complete_command("ki", false, &[]);
        assert!(result.is_none());

        // With permission, should see kick
        let perms = vec!["user_kick".to_string()];
        let result = complete_command("ki", false, &perms);
        assert!(result.is_some());
        let matches = result.unwrap();
        assert!(matches.iter().any(|c| c == "kick"));
    }

    #[test]
    fn test_complete_command_sorted() {
        let result = complete_command("", true, &[]);
        assert!(result.is_some());
        let matches = result.unwrap();
        // Verify sorted order
        for i in 1..matches.len() {
            assert!(
                matches[i - 1].to_lowercase() <= matches[i].to_lowercase(),
                "Commands not sorted: {} should come before {}",
                matches[i - 1],
                matches[i]
            );
        }
    }

    // --- command_names_for_completion tests ---

    #[test]
    fn test_command_names_admin_gets_all() {
        let names = command_names_for_completion(true, &[]);
        // Should have more names than commands (due to aliases)
        assert!(names.len() > COMMANDS.len());
    }

    #[test]
    fn test_command_names_no_perms_gets_public() {
        let names = command_names_for_completion(false, &[]);
        assert!(names.iter().any(|n| n == "help"));
        assert!(names.iter().any(|n| n == "clear"));
        // Should not have permission-gated commands
        assert!(!names.iter().any(|n| n == "kick"));
        assert!(!names.iter().any(|n| n == "broadcast"));
    }

    #[test]
    fn test_command_names_with_permission() {
        let perms = vec!["user_kick".to_string()];
        let names = command_names_for_completion(false, &perms);
        assert!(names.iter().any(|n| n == "kick"));
    }

    #[test]
    fn test_command_names_sorted() {
        let names = command_names_for_completion(true, &[]);
        for i in 1..names.len() {
            assert!(
                names[i - 1].to_lowercase() <= names[i].to_lowercase(),
                "Names not sorted: {} should come before {}",
                names[i - 1],
                names[i]
            );
        }
    }
}
