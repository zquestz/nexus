//! Server message handlers
//!
//! This module contains handlers for all server messages, organized by category.

mod broadcast;
mod chat;
mod error;
mod permissions;
mod user_admin;
mod user_connection;
mod user_info;
mod user_kick;
mod user_message;

pub use user_admin::UserEditResponseData;

use crate::types::Message;
use crate::NexusApp;
use iced::Task;
use nexus_common::protocol::ServerMessage;

impl NexusApp {
    /// Handle message received from server
    ///
    /// This is the entry point for all server messages, routing them to the appropriate handler.
    pub fn handle_server_message_received(
        &mut self,
        connection_id: usize,
        msg: ServerMessage,
    ) -> Task<Message> {
        if self.connections.contains_key(&connection_id) {
            self.handle_server_message(connection_id, msg)
        } else {
            Task::none()
        }
    }

    /// Process a specific server message and update state
    ///
    /// This is the main dispatcher that routes server messages to their specific handlers.
    pub fn handle_server_message(
        &mut self,
        connection_id: usize,
        msg: ServerMessage,
    ) -> Task<Message> {
        match msg {
            ServerMessage::ChatMessage {
                session_id: _,
                username,
                message,
            } => self.handle_chat_message(connection_id, username, message),

            ServerMessage::ChatTopic { topic, username } => {
                self.handle_chat_topic(connection_id, topic, username)
            }

            ServerMessage::ChatTopicUpdateResponse { success, error } => {
                self.handle_chat_topic_update_response(connection_id, success, error)
            }

            ServerMessage::Error { message, command } => {
                self.handle_error(connection_id, message, command)
            }

            ServerMessage::PermissionsUpdated {
                is_admin,
                permissions,
            } => self.handle_permissions_updated(connection_id, is_admin, permissions),

            ServerMessage::ServerBroadcast {
                session_id: _,
                username,
                message,
            } => self.handle_server_broadcast(connection_id, username, message),

            ServerMessage::UserBroadcastResponse { success, error } => {
                self.handle_user_broadcast_response(connection_id, success, error)
            }

            ServerMessage::UserConnected { user } => {
                self.handle_user_connected(connection_id, user)
            }

            ServerMessage::UserCreateResponse { success, error } => {
                self.handle_user_create_response(connection_id, success, error)
            }

            ServerMessage::UserDeleteResponse { success, error } => {
                self.handle_user_delete_response(connection_id, success, error)
            }

            ServerMessage::UserDisconnected {
                session_id,
                username,
            } => self.handle_user_disconnected(connection_id, session_id, username),

            ServerMessage::UserEditResponse {
                success,
                error,
                username,
                is_admin,
                enabled,
                permissions,
            } => self.handle_user_edit_response(
                connection_id,
                UserEditResponseData {
                    success,
                    error,
                    username,
                    is_admin,
                    enabled,
                    permissions,
                },
            ),

            ServerMessage::UserInfoResponse {
                success,
                error,
                user,
            } => self.handle_user_info_response(connection_id, success, error, user),

            ServerMessage::UserKickResponse { success, error } => {
                self.handle_user_kick_response(connection_id, success, error)
            }

            ServerMessage::UserListResponse {
                success,
                error: _,
                users,
            } => self.handle_user_list_response(connection_id, success, users),

            ServerMessage::UserMessage {
                from_username,
                to_username,
                message,
            } => self.handle_user_message(connection_id, from_username, to_username, message),

            ServerMessage::UserMessageResponse { success, error } => {
                self.handle_user_message_response(connection_id, success, error)
            }

            ServerMessage::UserUpdated {
                previous_username,
                user,
            } => self.handle_user_updated(connection_id, previous_username, user),

            ServerMessage::UserUpdateResponse { success, error } => {
                self.handle_user_update_response(connection_id, success, error)
            }

            // Catch-all for any unhandled message types
            _ => Task::none(),
        }
    }
}