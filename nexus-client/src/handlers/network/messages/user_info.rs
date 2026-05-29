//! User info response handlers

use chrono::Local;
use iced::Task;
use nexus_common::framing::MessageId;
use nexus_common::names::fold_name;
use nexus_common::protocol::{ClientMessage, UserInfo as ProtocolUserInfo, UserInfoDetailed};

use crate::NexusApp;
use crate::avatar::{apply_avatar_update, compute_avatar_hash, get_or_create_avatar};
use crate::constants::{ERR_CONNECTION_EXISTS, ERR_SYSTEM_TIME_AFTER_EPOCH};
use crate::handlers::network::constants::DATETIME_FORMAT;
use crate::handlers::network::helpers::{format_duration, sort_user_list};
use crate::i18n::{t, t_args};
use crate::types::{
    ActivePanel, ChatMessage, Message, PendingRequests, ResponseRouting, UserInfo as ClientUserInfo,
};

/// Indentation for user info display lines
const INFO_INDENT: &str = "  ";

fn detailed_user_matches_update(detail: &UserInfoDetailed, user: &ProtocolUserInfo) -> bool {
    if user.is_shared {
        detail.is_shared
            && detail.id == user.id
            && fold_name(&detail.nickname) == fold_name(&user.nickname)
    } else {
        !detail.is_shared && detail.id == user.id
    }
}

impl NexusApp {
    /// Handle user info response
    ///
    /// Routes response based on message_id tracking:
    /// - If tracked as UserInfoPanel(username) → populate panel if username matches
    /// - If tracked as UserInfoChat → display in chat (from `/info` command)
    /// - Otherwise → discard (stale/untracked response)
    pub fn handle_user_info_response(
        &mut self,
        connection_id: usize,
        message_id: MessageId,
        success: bool,
        error: Option<String>,
        user: Option<UserInfoDetailed>,
    ) -> Task<Message> {
        // Check if this response corresponds to a tracked request
        let routing = self
            .connections
            .get_mut(&connection_id)
            .and_then(|conn| conn.pending_requests.remove(&message_id));

        // Determine how to route this response
        let is_chat_request = matches!(routing, Some(ResponseRouting::DisplayUserInfoInChat));

        // If this was a panel request, only populate if panel is still open, waiting,
        // AND the username matches (to handle rapid clicks on different users)
        if let Some(ResponseRouting::PopulateUserInfoPanel(requested_nickname)) = routing {
            let panel_waiting = self.connections.get(&connection_id).is_some_and(|conn| {
                conn.active_panel == ActivePanel::UserInfo && conn.user_info_data.is_none()
            });

            // Check if requested nickname matches the response (case-insensitive)
            // nickname is always the display name (== username for regular accounts)
            let nickname_matches = match &user {
                Some(u) => fold_name(&requested_nickname) == fold_name(&u.nickname),
                None => true, // Error responses don't have user data, accept them
            };

            if panel_waiting && nickname_matches {
                // Populate the panel with the response
                if let Some(conn) = self.connections.get_mut(&connection_id) {
                    if success {
                        if let Some(user_data) = user {
                            conn.user_info_data = Some(Ok(user_data));
                        }
                    } else {
                        conn.user_info_data = Some(Err(error.unwrap_or_default()));
                    }
                }
            }
            // Panel request but panel closed, has data, or username mismatch - discard silently
            return Task::none();
        }

        // If not a chat request, discard (unknown/stale response)
        if !is_chat_request {
            return Task::none();
        }

        // Show in chat (from /info command)
        if !success {
            return self.add_active_tab_message(
                connection_id,
                ChatMessage::error(error.unwrap_or_default()),
            );
        }

        let Some(user) = user else {
            return Task::none();
        };

        // Calculate session duration
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect(ERR_SYSTEM_TIME_AFTER_EPOCH)
            .as_secs() as i64;
        let session_duration_secs = now.saturating_sub(user.login_time) as u64;
        let duration_str = format_duration(session_duration_secs);

        // Format account creation time (ISO 8601)
        let created = chrono::DateTime::from_timestamp(user.created_at, 0)
            .map(|dt| dt.format(DATETIME_FORMAT).to_string())
            .unwrap_or_else(|| t("user-info-unknown"));

        // Build multi-line IRC WHOIS-style output
        let mut lines = Vec::new();

        // Header: display name with 💤 if away (nickname is always populated - equals username for regular accounts)
        if user.is_away {
            lines.push(format!("[{} 💤]", user.nickname));
        } else {
            lines.push(format!("[{}]", user.nickname));
        }

        // Username (only shown for shared accounts where nickname differs from account name)
        if user.is_shared {
            lines.push(format!(
                "{INFO_INDENT}{} {}",
                t("user-info-username").to_lowercase(),
                user.username
            ));
        }

        // Role (only shown if is_admin field is present)
        if let Some(is_admin) = user.is_admin {
            let role_value = if is_admin {
                t("user-info-role-admin")
            } else if user.is_shared {
                t("user-info-role-shared")
            } else {
                t("user-info-role-user")
            };
            lines.push(format!(
                "{INFO_INDENT}{} {}",
                t("user-info-role").to_lowercase(),
                role_value
            ));
        }

        // Group (if user belongs to a group)
        if let Some(group_name) = &user.group_name {
            lines.push(format!(
                "{INFO_INDENT}{} {}",
                t("user-info-group").to_lowercase(),
                group_name
            ));
        }

        // Status (if set) - away is shown via 💤 in header
        if let Some(status) = &user.status {
            lines.push(format!(
                "{INFO_INDENT}{} {}",
                t("user-info-status").to_lowercase(),
                status
            ));
        }

        // Channels (if present and not empty)
        if let Some(channels) = &user.channels
            && !channels.is_empty()
        {
            lines.push(format!(
                "{INFO_INDENT}{} {}",
                t("user-info-channels").to_lowercase(),
                channels.join(", ")
            ));
        }

        // Sessions
        let session_count = user.session_ids.len();
        let connected_value = if session_count == 1 {
            t_args("user-info-connected-value", &[("duration", &duration_str)])
        } else {
            t_args(
                "user-info-connected-value-sessions",
                &[
                    ("duration", &duration_str),
                    ("count", &session_count.to_string()),
                ],
            )
        };
        lines.push(format!(
            "{INFO_INDENT}{} {}",
            t("user-info-connected").to_lowercase(),
            connected_value
        ));

        // Features
        let features_value = if user.features.is_empty() {
            t("user-info-features-none").to_lowercase()
        } else {
            t_args(
                "user-info-features-value",
                &[("features", &user.features.join(", "))],
            )
        };
        lines.push(format!(
            "{INFO_INDENT}{} {}",
            t("user-info-features").to_lowercase(),
            features_value
        ));

        // Locale
        lines.push(format!(
            "{INFO_INDENT}{} {}",
            t("user-info-locale").to_lowercase(),
            user.locale
        ));

        // IP Addresses (only visible to admins)
        if let Some(addresses) = user.addresses
            && !addresses.is_empty()
        {
            if addresses.len() == 1 {
                lines.push(format!(
                    "{INFO_INDENT}{} {}",
                    t("user-info-address").to_lowercase(),
                    addresses[0]
                ));
            } else {
                lines.push(format!(
                    "{INFO_INDENT}{}",
                    t("user-info-addresses").to_lowercase()
                ));
                for addr in &addresses {
                    lines.push(format!("{INFO_INDENT}  - {}", addr));
                }
            }
        }

        // Account created (last field)
        lines.push(format!(
            "{INFO_INDENT}{} {}",
            t("user-info-created").to_lowercase(),
            created
        ));

        lines.push(format!("{INFO_INDENT}{}", t("user-info-end")));

        // Add each line as a separate chat message with shared timestamp
        let timestamp = Local::now();
        let mut task = Task::none();
        for line in lines {
            task = self.add_active_tab_message(
                connection_id,
                ChatMessage::info_with_timestamp(line, timestamp),
            );
        }
        // Last add_active_tab_message will handle auto-scroll
        task
    }

    /// Handle user list response
    pub fn handle_user_list_response(
        &mut self,
        connection_id: usize,
        message_id: MessageId,
        success: bool,
        users: Option<Vec<ProtocolUserInfo>>,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Check if this response corresponds to a tracked request
        let routing = conn.pending_requests.remove(&message_id);

        let users_vec = users.unwrap_or_default();

        // If this was a user management list request, populate the panel
        if matches!(routing, Some(ResponseRouting::PopulateUserManagementList)) {
            if !success {
                if let Some(conn) = self.connections.get_mut(&connection_id) {
                    conn.user_management.all_users = Some(Err(t("err-userlist-failed")));
                }
                return Task::none();
            }

            if let Some(conn) = self.connections.get_mut(&connection_id) {
                conn.user_management.all_users = Some(Ok(users_vec));
            }
            return Task::none();
        }

        if !success {
            return Task::none();
        }

        // If this was a /list all request, display in chat instead of caching
        if matches!(routing, Some(ResponseRouting::DisplayListInChat)) {
            return self.display_all_users_list(connection_id, users_vec);
        }

        // Normal case: update the online_users cache
        let conn = self
            .connections
            .get_mut(&connection_id)
            .expect(ERR_CONNECTION_EXISTS);
        let user_list: Vec<ClientUserInfo> = users_vec
            .into_iter()
            .map(|u| {
                let avatar_hash = compute_avatar_hash(u.avatar.as_deref());
                // Pre-populate avatar cache using display name (nickname is always populated)
                get_or_create_avatar(&mut conn.avatar_cache, &u.nickname, u.avatar.as_deref());
                ClientUserInfo {
                    id: u.id,
                    username: u.username.clone(),
                    nickname: u.nickname,
                    is_admin: u.is_admin,
                    is_shared: u.is_shared,
                    session_ids: u.session_ids,
                    avatar_hash,
                    is_away: u.is_away,
                    status: u.status,
                }
            })
            .collect();

        conn.online_users = user_list;
        sort_user_list(&mut conn.online_users);

        // Clear expanded_user if the user is no longer in the list (folded compare so a
        // case-only rename snapshot — "Alice" in the list vs expanded "alice" — doesn't
        // drop the panel for the same user).
        if let Some(expanded) = &conn.expanded_user
            && !conn
                .online_users
                .iter()
                .any(|u| fold_name(&u.nickname) == fold_name(expanded))
        {
            conn.expanded_user = None;
        }
        Task::none()
    }

    /// Display all users list in chat (for /list all command)
    fn display_all_users_list(
        &mut self,
        connection_id: usize,
        users: Vec<ProtocolUserInfo>,
    ) -> Task<Message> {
        if users.is_empty() {
            return self
                .add_active_tab_message(connection_id, ChatMessage::info(t("cmd-list-empty")));
        }

        // Build IRC-style user list: @admin user1 user2
        // Use account username (not nickname) since /list all shows accounts, not sessions
        let user_count = users.len();
        let user_list: String = users
            .iter()
            .map(|user| {
                if user.is_admin {
                    format!("@{}", user.username)
                } else {
                    user.username.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(" ");

        // Format: "All users: @alice bob charlie (3 users)"
        let message = t_args(
            "cmd-list-all-output",
            &[("users", &user_list), ("count", &user_count.to_string())],
        );

        self.add_active_tab_message(connection_id, ChatMessage::info(message))
    }

    /// Handle user updated notification
    ///
    /// This is broadcast when an admin modifies a user's account (username, admin status, etc.)
    /// For shared accounts, multiple sessions may need updating (all sessions share the same account).
    pub fn handle_user_updated(
        &mut self,
        connection_id: usize,
        previous_username: String,
        user: ProtocolUserInfo,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        let new_username = user.username.clone();
        let username_changed = previous_username != new_username;

        conn.user_management.mark_edit_stale_if_user_id(user.id);

        // Match the affected entry/entries: a shared account's UserUpdated is sent
        // per session, so target the single entry by nickname; a regular account is
        // matched by its (previous) username (nickname == username). Case-insensitive
        // throughout, since online_users may carry client-provided casing.
        let target_nickname_lower = fold_name(&user.nickname);
        for existing_user in conn.online_users.iter_mut().filter(|u| {
            if user.is_shared {
                // Shared: one entry per nickname; this message targets exactly one.
                u.is_shared && fold_name(&u.nickname) == target_nickname_lower
            } else {
                // Regular: match by immutable id, not the (mutable) username — a
                // concurrent rename must not make this miss.
                !u.is_shared && u.id == user.id
            }
        }) {
            let old_nickname = existing_user.nickname.clone();
            // Captured before the hash is reassigned: whether a decoded (non-identicon)
            // avatar exists to preserve across a rename in the unchanged case.
            let had_real_avatar = existing_user.avatar_hash.is_some();

            existing_user.username = new_username.clone();
            // For shared accounts, nickname is per-session and doesn't come from
            // UserUpdated; only regular accounts take the new nickname.
            if !existing_user.is_shared {
                existing_user.nickname = user.nickname.clone();
            }
            existing_user.is_admin = user.is_admin;
            // is_shared is immutable, but update anyway for consistency
            existing_user.is_shared = user.is_shared;
            existing_user.session_ids = user.session_ids.clone();
            existing_user.is_away = user.is_away;
            existing_user.status = user.status.clone();

            // Avatar tri-state delta: None = unchanged (keep cache), "" = cleared,
            // data = new. A None must never be recomputed into a blank hash.
            let new_avatar_hash = match user.avatar.as_deref() {
                None => existing_user.avatar_hash,
                Some("") => None,
                Some(data) => compute_avatar_hash(Some(data)),
            };
            existing_user.avatar_hash = new_avatar_hash;

            // Apply the same delta to the decoded-avatar cache (re-keying on rename).
            let new_nickname = existing_user.nickname.clone();
            apply_avatar_update(
                &mut conn.avatar_cache,
                &old_nickname,
                &new_nickname,
                user.avatar.as_deref(),
                had_real_avatar,
            );
        }

        let refetch_user_info_nickname = if conn.active_panel == ActivePanel::UserInfo {
            conn.user_info_data.as_ref().and_then(|data| match data {
                Ok(detail) if detailed_user_matches_update(detail, &user) => {
                    Some(if user.is_shared {
                        detail.nickname.clone()
                    } else {
                        user.nickname.clone()
                    })
                }
                _ => None,
            })
        } else {
            None
        };

        if let Some(nickname) = refetch_user_info_nickname {
            conn.user_info_data = None;
            match conn.send(ClientMessage::UserInfo {
                nickname: nickname.clone(),
            }) {
                Ok(message_id) => conn
                    .pending_requests
                    .track(message_id, ResponseRouting::PopulateUserInfoPanel(nickname)),
                Err(e) => {
                    let error_msg = format!("{}: {}", t("err-send-failed"), e);
                    conn.user_info_data = Some(Err(error_msg));
                }
            }
        }

        // Re-sort the list since username may have changed
        sort_user_list(&mut conn.online_users);

        // If the username changed, re-key this connection's rename-sensitive state.
        // DM tabs display the nickname but are resolved by folded identity, so the
        // re-key merges any fold-equal conversation onto the new name. Regular accounts
        // (nickname == username) move the tab; a shared account's per-session nickname is
        // unaffected by a username change, so it's a no-op — both correct.
        let mut renamed_self_nickname = false;
        if username_changed {
            // DM tab, our own identity, and active voice session (shared with
            // ChatUserRenamed; idempotent). Channel member lists and voiced sets are
            // re-keyed by ChatUserRenamed instead, so they reach channel members who
            // lack `user_list` and never receive this UserUpdated.
            renamed_self_nickname = conn.apply_user_rename(&previous_username, &new_username);

            // expanded_user is a user-list detail, so it stays here (UserUpdated is
            // itself UserList-gated). Regular accounts have nickname == username so
            // this matches; a shared account's per-session nickname is unaffected by a
            // username change, so this is a no-op for it.
            if conn
                .expanded_user
                .as_ref()
                .is_some_and(|n| fold_name(n) == fold_name(&previous_username))
            {
                conn.expanded_user = Some(new_username.clone());
            }
        }

        // Rename effects that live outside this connection's own state (voice audio
        // thread, chat history, news cache, saved bookmarks, persisted transfers).
        // The `conn` borrow has ended, so this takes `&mut self`.
        if username_changed {
            return self.apply_rename_side_effects(
                connection_id,
                &previous_username,
                &new_username,
                user.is_shared,
                user.is_admin,
                renamed_self_nickname,
            );
        }

        Task::none()
    }

    /// Apply the parts of a rename that live outside the renamed user's
    /// `ServerConnection`, and so can't ride along in
    /// `ServerConnection::apply_user_rename`. Called from both rename paths
    /// (`handle_user_updated` and `handle_chat_user_renamed`) right after that
    /// method, once the `conn` borrow has been released.
    pub(crate) fn apply_rename_side_effects(
        &mut self,
        connection_id: usize,
        old: &str,
        new: &str,
        is_shared: bool,
        is_admin: bool,
        renamed_self_nickname: bool,
    ) -> Task<Message> {
        let mut tasks = Vec::new();
        let connection_info = self
            .connections
            .get(&connection_id)
            .map(|conn| conn.connection_info.clone());

        // The audio thread keeps its own participant, speaking, buffer, and mute
        // state. When this connection owns the active voice session, re-key that
        // state too: already-accepted audio remains valid, stale old-name packets
        // are rejected after the command lands, and speaking is rebuilt by fresh
        // packets/events.
        if self.active_voice_connection == Some(connection_id)
            && let Some(handle) = &self.voice_session_handle
            && let Some(muted) = self
                .connections
                .get(&connection_id)
                .and_then(|c| c.voice_session.as_ref())
                .map(|vs| vs.muted_users.contains(&fold_name(new)))
        {
            handle.user_renamed(old, new, muted);
        }

        if renamed_self_nickname
            && self.is_local_speaking
            && self.active_voice_connection == Some(connection_id)
            && let Some(conn) = self.connections.get_mut(&connection_id)
            && let Some(voice_session) = conn.voice_session.as_mut()
        {
            voice_session.set_speaking(new);
        }

        // Chat history: re-key the renamed party's DM conversation. A self-rename is
        // a no-op here (no conversation is keyed by your own nickname, and the history
        // directory is keyed by the immutable user id, so it never moves on a rename).
        if let Some(base_dir) = self.connection_history_keys.get(&connection_id).cloned()
            && let Some(manager) = self.history_managers.get_mut(&base_dir)
            && let Err(e) = manager.rename_conversation(old, new)
        {
            tasks.push(self.add_background_error_message(
                connection_id,
                t_args(
                    "err-chat-history-rename",
                    &[("old", old), ("new", new), ("error", &e.to_string())],
                ),
            ));
        }

        if let Some(conn) = self.connections.get_mut(&connection_id) {
            conn.news_management
                .rename_cached_author(old, new, is_admin);
        }

        if let Some(connection_info) = connection_info {
            // Saved bookmarks can point at any account on this server, not just the
            // active connection's own account. Keep matching bookmark usernames in
            // step so quick-connect/auto-connect survive a rename.
            let updated_bookmark_ids = self.config.update_bookmark_usernames_for_server_rename(
                &connection_info.address,
                connection_info.port,
                &connection_info.certificate_fingerprint,
                old,
                new,
                is_shared,
            );
            for id in &updated_bookmark_ids {
                self.bookmark_edit.mark_edit_stale_if_bookmark_id(*id);
            }
            if !updated_bookmark_ids.is_empty()
                && let Err(e) = self.config.save()
            {
                tasks.push(self.add_background_error_message(
                    connection_id,
                    t_args("err-failed-save-config", &[("error", &e)]),
                ));
            }

            // Transfer resume opens a new transfer-port login from the persisted
            // transfer row. When this connection's own account was renamed, refresh
            // those saved credentials too; otherwise pause/resume would retry the old
            // username. Shared-account nicknames are preserved by the manager.
            if fold_name(&connection_info.username) == fold_name(new)
                && self
                    .transfer_manager
                    .update_username_for_connection(&connection_info, old, new)
                && let Err(e) = self.transfer_manager.save()
            {
                tasks.push(self.add_background_error_message(connection_id, e));
            }
        }

        Task::batch(tasks)
    }
}
