//! /window command implementation - manage chat tabs

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{ChatMessage, ChatTab, Message};
use iced::Task;

/// Get translated subcommand keywords
fn get_keywords() -> (String, String, String) {
    (
        t("cmd-window-arg-next").to_lowercase(),
        t("cmd-window-arg-prev").to_lowercase(),
        t("cmd-window-arg-close").to_lowercase(),
    )
}

/// Execute the /window command
///
/// Manages chat tabs.
/// Usage:
/// - `/window` or `/w` - List open tabs
/// - `/window close` or `/w close` - Close current PM tab
/// - `/window close <username>` or `/w close <username>` - Close specific user's PM tab
/// - `/window next` or `/w next` - Switch to next tab
/// - `/window prev` or `/w prev` - Switch to previous tab
pub fn execute(
    app: &mut NexusApp,
    connection_id: usize,
    invoked_name: &str,
    args: &[String],
) -> Task<Message> {
    let Some(conn) = app.connections.get(&connection_id) else {
        return Task::none();
    };

    // No args = list open tabs
    if args.is_empty() {
        return list_tabs(app, connection_id);
    }

    let (next_keyword, prev_keyword, close_keyword) = get_keywords();
    let arg = args[0].to_lowercase();

    if arg == next_keyword {
        if args.len() > 1 {
            let error_msg = t_args("cmd-window-usage", &[("command", invoked_name)]);
            return app.add_chat_message(connection_id, ChatMessage::error(error_msg));
        }
        Task::done(Message::NextChatTab)
    } else if arg == prev_keyword {
        if args.len() > 1 {
            let error_msg = t_args("cmd-window-usage", &[("command", invoked_name)]);
            return app.add_chat_message(connection_id, ChatMessage::error(error_msg));
        }
        Task::done(Message::PrevChatTab)
    } else if arg == close_keyword {
        if args.len() == 1 {
            // /window close - close current tab
            close_current_tab(app, connection_id)
        } else if args.len() == 2 {
            // /window close <username> - close specific tab
            let target = &args[1];
            let target_lower = target.to_lowercase();

            // Find matching tab (case-insensitive)
            let matching_user = conn
                .user_messages
                .keys()
                .find(|username| username.to_lowercase() == target_lower)
                .cloned();

            if let Some(username) = matching_user {
                Task::done(Message::CloseUserMessageTab(username))
            } else {
                let error_msg = t_args("cmd-window-not-found", &[("name", target.as_str())]);
                app.add_chat_message(connection_id, ChatMessage::error(error_msg))
            }
        } else {
            let error_msg = t_args("cmd-window-usage", &[("command", invoked_name)]);
            app.add_chat_message(connection_id, ChatMessage::error(error_msg))
        }
    } else {
        let error_msg = t_args("cmd-window-usage", &[("command", invoked_name)]);
        app.add_chat_message(connection_id, ChatMessage::error(error_msg))
    }
}

/// List all open tabs
fn list_tabs(app: &mut NexusApp, connection_id: usize) -> Task<Message> {
    let Some(conn) = app.connections.get(&connection_id) else {
        return Task::none();
    };

    let mut tabs = vec![t("chat-tab-server")];

    let mut pm_usernames: Vec<String> = conn.user_messages.keys().cloned().collect();
    pm_usernames.sort();

    for username in pm_usernames {
        tabs.push(username);
    }

    let tab_list = tabs.join(", ");
    let message = t_args(
        "cmd-window-list",
        &[("tabs", &tab_list), ("count", &tabs.len().to_string())],
    );

    app.add_chat_message(connection_id, ChatMessage::info(message))
}

/// Close the current PM tab
fn close_current_tab(app: &mut NexusApp, connection_id: usize) -> Task<Message> {
    let Some(conn) = app.connections.get(&connection_id) else {
        return Task::none();
    };

    match &conn.active_chat_tab {
        ChatTab::Server => app.add_chat_message(
            connection_id,
            ChatMessage::error(t("cmd-window-close-server")),
        ),
        ChatTab::UserMessage(username) => {
            Task::done(Message::CloseUserMessageTab(username.clone()))
        }
    }
}
