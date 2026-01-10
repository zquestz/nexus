//! Network message handlers
//!
//! This module contains handlers for all server messages, organized by category.

use files::FileListResponseData;

mod ban_create;
mod ban_delete;
mod ban_list;
mod broadcast;
mod chat;
mod error;
mod files;
mod news;
mod permissions;
mod server_info;
mod user_admin;
mod user_connection;
mod user_info;
mod user_kick;
mod user_message;
mod user_status;

pub use user_admin::UserEditResponseData;

use iced::Task;
use nexus_common::framing::MessageId;
use nexus_common::protocol::ServerMessage;

use crate::NexusApp;
use crate::types::Message;

impl NexusApp {
    /// Handle message received from server
    ///
    /// This is the entry point for all server messages, routing them to the appropriate handler.
    pub fn handle_server_message_received(
        &mut self,
        connection_id: usize,
        message_id: MessageId,
        msg: ServerMessage,
    ) -> Task<Message> {
        if self.connections.contains_key(&connection_id) {
            self.handle_server_message(connection_id, message_id, msg)
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
        message_id: MessageId,
        msg: ServerMessage,
    ) -> Task<Message> {
        match msg {
            ServerMessage::ChatMessage {
                session_id: _,
                nickname,
                is_admin,
                is_shared,
                message,
                action,
            } => self.handle_chat_message(
                connection_id,
                nickname,
                message,
                is_admin,
                is_shared,
                action,
            ),

            ServerMessage::ChatTopicUpdated { topic, username } => {
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
                server_info,
                chat_info,
            } => self.handle_permissions_updated(
                connection_id,
                is_admin,
                permissions,
                server_info,
                chat_info,
            ),

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

            ServerMessage::UserCreateResponse {
                success,
                error,
                username,
            } => self.handle_user_create_response(
                connection_id,
                message_id,
                success,
                error,
                username,
            ),

            ServerMessage::UserDeleteResponse {
                success,
                error,
                username,
            } => self.handle_user_delete_response(
                connection_id,
                message_id,
                success,
                error,
                username,
            ),

            ServerMessage::UserDisconnected {
                session_id,
                nickname,
            } => self.handle_user_disconnected(connection_id, session_id, nickname),

            ServerMessage::UserEditResponse {
                success,
                error,
                username,
                is_admin,
                is_shared,
                enabled,
                permissions,
            } => self.handle_user_edit_response(
                connection_id,
                message_id,
                UserEditResponseData {
                    success,
                    error,
                    username,
                    is_admin,
                    is_shared,
                    enabled,
                    permissions,
                },
            ),

            ServerMessage::UserInfoResponse {
                success,
                error,
                user,
            } => self.handle_user_info_response(connection_id, message_id, success, error, user),

            ServerMessage::UserKickResponse {
                success,
                error,
                nickname,
            } => self.handle_user_kick_response(connection_id, success, error, nickname),

            ServerMessage::UserListResponse {
                success,
                error: _,
                users,
            } => self.handle_user_list_response(connection_id, message_id, success, users),

            ServerMessage::UserMessage {
                from_nickname,
                from_admin,
                to_nickname,
                message,
                action,
            } => self.handle_user_message(
                connection_id,
                from_nickname,
                from_admin,
                to_nickname,
                message,
                action,
            ),

            ServerMessage::UserMessageResponse {
                success,
                error,
                is_away,
                status,
            } => self.handle_user_message_response(
                connection_id,
                message_id,
                success,
                error,
                is_away,
                status,
            ),

            ServerMessage::UserUpdated {
                previous_username,
                user,
            } => self.handle_user_updated(connection_id, previous_username, user),

            ServerMessage::UserUpdateResponse {
                success,
                error,
                username,
            } => self.handle_user_update_response(
                connection_id,
                message_id,
                success,
                error,
                username,
            ),

            ServerMessage::ServerInfoUpdated { server_info } => {
                self.handle_server_info_updated(connection_id, server_info)
            }

            ServerMessage::ServerInfoUpdateResponse { success, error } => {
                self.handle_server_info_update_response(connection_id, success, error)
            }

            ServerMessage::NewsListResponse {
                success,
                error,
                items,
            } => self.handle_news_list_response(connection_id, message_id, success, error, items),

            ServerMessage::NewsShowResponse {
                success,
                error,
                news,
            } => self.handle_news_show_response(connection_id, message_id, success, error, news),

            ServerMessage::NewsCreateResponse {
                success,
                error,
                news,
            } => self.handle_news_create_response(connection_id, message_id, success, error, news),

            ServerMessage::NewsEditResponse {
                success,
                error,
                news,
            } => self.handle_news_edit_response(connection_id, message_id, success, error, news),

            ServerMessage::NewsUpdateResponse {
                success,
                error,
                news,
            } => self.handle_news_update_response(connection_id, message_id, success, error, news),

            ServerMessage::NewsDeleteResponse { success, error, id } => {
                self.handle_news_delete_response(connection_id, message_id, success, error, id)
            }

            ServerMessage::NewsUpdated { action, id } => {
                self.handle_news_updated(connection_id, action, id)
            }

            ServerMessage::FileListResponse {
                success,
                error,
                path,
                entries,
                can_upload,
            } => self.handle_file_list_response(
                connection_id,
                message_id,
                FileListResponseData {
                    success,
                    error,
                    path,
                    entries,
                    can_upload,
                },
            ),

            ServerMessage::FileCreateDirResponse {
                success,
                error,
                path,
            } => self.handle_file_create_dir_response(
                connection_id,
                message_id,
                success,
                error,
                path,
            ),

            ServerMessage::FileDeleteResponse { success, error } => {
                self.handle_file_delete_response(connection_id, message_id, success, error)
            }

            ServerMessage::FileInfoResponse {
                success,
                error,
                info,
            } => self.handle_file_info_response(connection_id, message_id, success, error, info),

            ServerMessage::FileRenameResponse { success, error } => {
                self.handle_file_rename_response(connection_id, message_id, success, error)
            }

            ServerMessage::FileMoveResponse {
                success,
                error,
                error_kind,
            } => self.handle_file_move_response(
                connection_id,
                message_id,
                success,
                error,
                error_kind,
            ),

            ServerMessage::FileCopyResponse {
                success,
                error,
                error_kind,
            } => self.handle_file_copy_response(
                connection_id,
                message_id,
                success,
                error,
                error_kind,
            ),

            ServerMessage::UserAwayResponse { success, error } => {
                self.handle_user_away_response(connection_id, message_id, success, error)
            }

            ServerMessage::UserBackResponse { success, error } => {
                self.handle_user_back_response(connection_id, message_id, success, error)
            }

            ServerMessage::UserStatusResponse { success, error } => {
                self.handle_user_status_response(connection_id, message_id, success, error)
            }

            ServerMessage::BanCreateResponse {
                success,
                error,
                ips,
                nickname,
            } => self.handle_ban_create_response(connection_id, success, error, ips, nickname),

            ServerMessage::BanDeleteResponse {
                success,
                error,
                ips,
                nickname,
            } => self.handle_ban_delete_response(connection_id, success, error, ips, nickname),

            ServerMessage::BanListResponse {
                success,
                error,
                bans,
            } => self.handle_ban_list_response(connection_id, success, error, bans),

            ServerMessage::FileReindexResponse { success, error } => {
                self.handle_file_reindex_response(connection_id, success, error)
            }

            ServerMessage::FileSearchResponse {
                success,
                error,
                results,
            } => {
                self.handle_file_search_response(connection_id, message_id, success, error, results)
            }

            // Catch-all for any unhandled message types
            _ => Task::none(),
        }
    }
}
