//! Message handlers for client commands

mod ban_create;
mod ban_delete;
mod ban_list;
mod broadcast;
mod chat;
mod chat_join;
mod chat_leave;
mod chat_list;
mod chat_secret;
mod chat_topic_update;
mod connection_monitor;
pub(crate) mod duration;
pub mod errors;
mod file_copy;
mod file_create_dir;
mod file_delete;
mod file_info;
mod file_list;
mod file_move;
mod file_reindex;
mod file_rename;
mod file_search;
mod group_create;
mod group_delete;
mod group_edit;
mod group_list;
mod group_update;
mod handshake;
mod login;
mod news_create;
mod news_delete;
mod news_edit;
mod news_list;
mod news_show;
mod news_update;
mod server_info;
mod server_info_update;
mod tracker_accept_fingerprint;
mod tracker_add;
mod tracker_edit;
mod tracker_list;
mod tracker_remove;
mod tracker_update;
mod tracker_utils;
mod trust_create;
mod trust_delete;
mod trust_list;
mod user_away;
mod user_back;
mod user_create;
mod user_delete;
mod user_edit;
mod user_info;
mod user_kick;
mod user_list;
mod user_message;
mod user_status;
mod user_update;
mod voice_join;
mod voice_leave;

#[cfg(test)]
pub mod testing;

pub use ban_create::handle_ban_create;
pub use ban_delete::handle_ban_delete;
pub use ban_list::handle_ban_list;
pub use broadcast::handle_user_broadcast;
pub use chat::handle_chat_send;
pub use chat_join::handle_chat_join;
pub use chat_leave::handle_chat_leave;
pub use chat_list::handle_chat_list;
pub use chat_secret::handle_chat_secret;
pub use chat_topic_update::handle_chat_topic_update;
pub use connection_monitor::handle_connection_monitor;
pub use errors::*;
pub use file_copy::handle_file_copy;
pub use file_create_dir::handle_file_create_dir;
pub use file_delete::handle_file_delete;
pub use file_info::handle_file_info;
pub use file_list::handle_file_list;
pub use file_move::handle_file_move;
pub use file_reindex::handle_file_reindex;
pub use file_rename::handle_file_rename;
pub use file_search::handle_file_search;
pub use group_create::handle_group_create;
pub use group_delete::handle_group_delete;
pub use group_edit::handle_group_edit;
pub use group_list::handle_group_list;
pub use group_update::handle_group_update;
pub use handshake::handle_handshake;
pub use login::{LoginRequest, handle_login};
pub use news_create::handle_news_create;
pub use news_delete::handle_news_delete;
pub use news_edit::handle_news_edit;
pub use news_list::handle_news_list;
pub use news_show::handle_news_show;
pub use news_update::handle_news_update;
pub use server_info::{ServerInfoOptions, ServerInfoValues, build_server_info};
pub use server_info_update::{ServerInfoUpdateRequest, handle_server_info_update};
pub use tracker_accept_fingerprint::handle_tracker_accept_fingerprint;
pub use tracker_add::{TrackerAddRequest, handle_tracker_add};
pub use tracker_edit::handle_tracker_edit;
pub use tracker_list::handle_tracker_list;
pub use tracker_remove::handle_tracker_remove;
pub use tracker_update::{TrackerUpdateRequest, handle_tracker_update};
pub use tracker_utils::{compose_tracker_info, validate_tracker_inputs};
pub use trust_create::handle_trust_create;
pub use trust_delete::handle_trust_delete;
pub use trust_list::handle_trust_list;
pub use user_away::handle_user_away;
pub use user_back::handle_user_back;
pub use user_create::{UserCreateRequest, handle_user_create};
pub use user_delete::handle_user_delete;
pub use user_edit::handle_user_edit;
pub use user_info::handle_user_info;
pub use user_kick::handle_user_kick;
pub use user_list::handle_user_list;
pub use user_message::handle_user_message;
pub use user_status::handle_user_status;
pub use user_update::{UserUpdateRequest, handle_user_update};
pub use voice_join::handle_voice_join;
pub use voice_leave::handle_voice_leave;

use std::collections::{HashMap, HashSet};
use std::io;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use crate::constants::ERR_CHANNEL_CLOSED;

use tokio::io::AsyncWrite;
use tokio::sync::mpsc;

use nexus_common::framing::{FrameWriter, MessageId};
use nexus_common::io::send_server_message_with_id;
use nexus_common::names::fold_name;
use nexus_common::protocol::ServerMessage;

use crate::channels::ChannelManager;
use crate::connection_tracker::ConnectionTracker;
use crate::db::{Database, Permission};
use crate::files::{FileIndex, PathLockMap};
use crate::flood::FloodConfig;
use crate::ip_rule_cache::IpRuleState;
use crate::tracker::TrackerManager;
use crate::transfers::TransferRegistry;
use crate::users::UserManager;
use crate::users::user::UserSession;
use crate::voice::{VoiceRegistry, send_voice_leave_notifications};

/// Shared resources passed to every handler.
pub struct HandlerContext<'a, W> {
    pub writer: &'a mut FrameWriter<W>,
    pub peer_addr: SocketAddr,
    pub user_manager: &'a UserManager,
    pub db: &'a Database,
    pub tx: &'a mpsc::UnboundedSender<(ServerMessage, Option<MessageId>)>,
    pub locale: &'a str,
    /// Echoed back on responses for request correlation.
    pub message_id: MessageId,
    /// File area root, or None if the file area isn't configured.
    pub file_root: Option<&'static Path>,
    pub transfer_port: u16,
    /// WebSocket transfer port; None when WebSocket is disabled.
    pub transfer_websocket_port: Option<u16>,
    pub connection_tracker: Arc<ConnectionTracker>,
    /// In-memory ban/trust lookup cache plus mutation gate.
    pub ip_rule_cache: Arc<IpRuleState>,
    pub file_index: Arc<FileIndex>,
    /// See `files::path_lock`.
    pub file_mutation_locks: Arc<PathLockMap>,
    pub channel_manager: &'a ChannelManager,
    /// Used to disconnect active transfers on ban.
    pub transfer_registry: Arc<TransferRegistry>,
    pub voice_registry: &'a VoiceRegistry,
    /// Per-tracker registration tasks.
    pub tracker_manager: &'a TrackerManager,
    /// Server certificate fingerprint (SHA-256, colon-separated).
    pub fingerprint: &'static str,
    pub flood_config: Arc<FloodConfig>,
}

impl<'a, W: AsyncWrite + Unpin> HandlerContext<'a, W> {
    /// Send to the client, echoing the request's message ID.
    pub async fn send_message(&mut self, message: &ServerMessage) -> io::Result<()> {
        send_server_message_with_id(self.writer, message, self.message_id).await
    }

    /// Send via the channel rather than the socket, so the message queues after
    /// any broadcasts on the same channel. Use when a response must arrive after
    /// broadcasts in the client's receive order.
    pub fn send_message_via_channel(&self, message: &ServerMessage) -> io::Result<()> {
        self.tx
            .send((message.clone(), Some(self.message_id)))
            .map_err(|_| io::Error::other(ERR_CHANNEL_CLOSED))
    }

    /// Send an error message without disconnecting
    pub async fn send_error(&mut self, message: &str, command: Option<&str>) -> io::Result<()> {
        let error_msg = ServerMessage::Error {
            message: message.to_string(),
            command: command.map(|s| s.to_string()),
        };
        self.send_message(&error_msg).await
    }

    /// Send an error message and disconnect
    pub async fn send_error_and_disconnect(
        &mut self,
        message: &str,
        command: Option<&str>,
    ) -> io::Result<()> {
        self.send_error(message, command).await?;
        Err(io::Error::other(message))
    }
}

/// Terminal action chosen under a state lock; dispatched by `dispatch_outcome`
/// after the lock guard is dropped. DB errors flow through `Send`; only a
/// missing session uses `Disconnect`.
pub(super) enum Outcome {
    Send(Box<ServerMessage>),
    Disconnect,
}

/// Tail dispatch for admin handlers using the labelled-block + `Outcome` pattern.
pub(super) async fn dispatch_outcome<W>(
    outcome: Outcome,
    ctx: &mut HandlerContext<'_, W>,
    handler_name: &'static str,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    match outcome {
        Outcome::Send(response) => ctx.send_message(&response).await,
        Outcome::Disconnect => {
            ctx.send_error_and_disconnect(&err_not_logged_in(ctx.locale), Some(handler_name))
                .await
        }
    }
}

/// Remove a batch of sessions and run the full disconnect teardown for each:
/// remove them from the map (the serialization point — only sessions this call
/// actually removes are torn down), release their channel + voice resources, then
/// announce the disconnects. Used by kick, account deletion, disable, and ban;
/// each caller sends its own per-session reason while the sessions are still live,
/// so `cleanup_resources` also notifies each leaver (`notify_leaving_user = true`).
/// The forced/batch counterpart to the graceful single-session teardown in
/// `connection.rs`; both share `cleanup_resources` so they clean up identically.
pub async fn remove_users_with_cleanup(
    user_manager: &UserManager,
    voice_registry: &VoiceRegistry,
    channel_manager: &ChannelManager,
    sessions: &[UserSession],
) {
    let session_ids: Vec<u32> = sessions.iter().map(|u| u.session_id).collect();
    let removed = user_manager.remove_users(&session_ids).await;
    cleanup_resources(
        user_manager,
        voice_registry,
        channel_manager,
        &removed,
        true,
    )
    .await;
    user_manager.broadcast_disconnections(&removed).await;
}

/// Release the per-session resources a disconnect must free, identically for
/// either teardown path: remove each session from every channel (emitting
/// `ChatUserLeft` to remaining members on the last leave of a nickname) and from
/// any voice session (notifying other participants). Does NOT remove sessions from
/// the user manager or announce `UserDisconnected` — the caller owns that
/// (`remove_users` before, `broadcast_disconnections` after, so cleanup messages
/// precede the disconnect). `notify_leaving_user` forwards each leaver their own
/// `VoiceUserLeft`: true while they're still live (kick/ban/etc.), false for a
/// session that's already gone (graceful teardown).
pub async fn cleanup_resources(
    user_manager: &UserManager,
    voice_registry: &VoiceRegistry,
    channel_manager: &ChannelManager,
    sessions: &[UserSession],
    notify_leaving_user: bool,
) {
    // Pass 1: remove all sessions from all channels, recording which nicknames left
    // each channel. Processing in bulk before the leave-check prevents duplicate
    // ChatUserLeft for the same nickname when a regular account's multiple sessions
    // are removed in one batch — all sessions are already out of UserManager by the
    // time cleanup runs, so interleaving removal and the "still present?" check would
    // incorrectly treat batch-sibling sessions as gone from UserManager even though
    // they are still in the channel.
    let mut channel_to_nicknames: HashMap<String, HashSet<String>> = HashMap::new();
    for session in sessions {
        for channel_name in channel_manager.remove_from_all(session.session_id).await {
            channel_to_nicknames
                .entry(channel_name)
                .or_default()
                .insert(session.nickname.clone());
        }
    }

    // Pass 2: for each affected channel, emit one ChatUserLeft per nickname that
    // fully departed. All batch sessions are now removed from both UserManager and
    // the channel, so remaining_members is clean and sessions_contain_nickname
    // correctly reflects only surviving sessions.
    for (channel_name, nicknames) in &channel_to_nicknames {
        if let Some(remaining_members) = channel_manager.get_members(channel_name).await {
            for nickname in nicknames {
                let still_present = user_manager
                    .sessions_contain_nickname(&remaining_members, nickname, None)
                    .await;
                if !still_present {
                    let leave_msg = ServerMessage::ChatUserLeft {
                        channel: channel_name.clone(),
                        nickname: nickname.clone(),
                    };
                    for &member_session_id in &remaining_members {
                        user_manager
                            .send_to_session(member_session_id, leave_msg.clone())
                            .await;
                    }
                }
            }
        }
    }

    // Pass 3: voice cleanup — voice sessions are 1:1 with user sessions, so no
    // nickname dedup is needed here.
    for session in sessions {
        if let Some(info) = voice_registry
            .remove_by_session_id(session.session_id)
            .await
        {
            send_voice_leave_notifications(
                &info,
                notify_leaving_user.then_some(&session.tx),
                user_manager,
                channel_manager,
            )
            .await;
        }
    }
}

/// Fan out one `UserUpdated` per affected member (filtered by `should_emit`) to
/// all `user_list` holders. Shared accounts broadcast **per session**
/// (`build_user_info_from_session`), since each shared session is its own
/// user-list entry; regular accounts dedup by username and send one aggregated
/// `UserInfo`. `previous_username_override` carries the pre-edit username for
/// single-account callers that may rename (`user_update`); pass `None` when each
/// member's current username is already the previous one (group cascades, which
/// never rename and may span multiple accounts).
pub(crate) async fn broadcast_user_updated_for_members<W, F>(
    ctx: &HandlerContext<'_, W>,
    member_sessions: &[UserSession],
    previous_username_override: Option<&str>,
    should_emit: F,
) where
    W: AsyncWrite + Unpin,
    F: Fn(&UserSession) -> bool,
{
    let mut seen_usernames: HashSet<String> = HashSet::new();
    for session in member_sessions {
        if !should_emit(session) {
            continue;
        }

        if session.is_shared {
            let user_updated = ServerMessage::UserUpdated {
                previous_username: previous_username_override
                    .unwrap_or(&session.username)
                    .to_string(),
                user: UserManager::build_user_info_from_session(session),
            };
            ctx.user_manager
                .broadcast_to_permission(user_updated, Permission::UserList)
                .await;
        } else if seen_usernames.insert(fold_name(&session.username)) {
            // Regular account: collapse all of its sessions into one aggregated entry.
            let all_sessions = ctx
                .user_manager
                .get_sessions_by_username(&session.username)
                .await;
            if let Some(user_info) = UserManager::build_aggregated_user_info(&all_sessions) {
                let user_updated = ServerMessage::UserUpdated {
                    previous_username: previous_username_override
                        .unwrap_or(&session.username)
                        .to_string(),
                    user: user_info,
                };
                ctx.user_manager
                    .broadcast_to_permission(user_updated, Permission::UserList)
                    .await;
            }
        }
    }
}

/// Tell every channel the user's sessions belong to that a nickname changed,
/// via the channel stream (not the UserList-gated `UserUpdated`), so member
/// lists and voiced sets stay correct for members without `user_list` and for
/// the renamed user themselves. ChannelManager supplies membership; the send
/// happens here, mirroring `cleanup_resources`.
pub(crate) async fn broadcast_chat_user_renamed(
    user_manager: &UserManager,
    channel_manager: &ChannelManager,
    sessions: &[UserSession],
    old_nickname: &str,
    new_nickname: &str,
    is_admin: bool,
) {
    // Union of channels across the user's sessions (small N).
    let mut channels: Vec<String> = Vec::new();
    for session in sessions {
        for channel in channel_manager
            .channels_with_member(session.session_id)
            .await
        {
            if !channels.contains(&channel) {
                channels.push(channel);
            }
        }
    }
    for channel in channels {
        if let Some(members) = channel_manager.get_members(&channel).await {
            let msg = ServerMessage::ChatUserRenamed {
                channel,
                old_nickname: old_nickname.to_string(),
                new_nickname: new_nickname.to_string(),
                is_admin,
            };
            for member_session_id in members {
                user_manager
                    .send_to_session(member_session_id, msg.clone())
                    .await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channels::JoinPolicy;
    use crate::db::ChannelDb;
    use crate::db::testing::create_test_db;
    use crate::users::user::NewSessionParams;

    type TestTx = mpsc::UnboundedSender<(ServerMessage, Option<MessageId>)>;

    fn regular_session(user_id: i64, nickname: &str, tx: TestTx) -> NewSessionParams {
        NewSessionParams {
            session_id: 0,
            user_id,
            username: nickname.to_string(),
            is_admin: false,
            is_shared: false,
            permissions: HashSet::new(),
            address: "127.0.0.1:12345".parse::<SocketAddr>().unwrap(),
            created_at: 0,
            tx,
            features: vec![],
            locale: "en".to_string(),
            avatar: None,
            nickname: nickname.to_string(),
            is_away: false,
            status: None,
            group_id: None,
            group_name: None,
            bandwidth_weight_override: None,
            last_activity: std::time::Instant::now(),
            bandwidth_weight: 1,
        }
    }

    /// Regression for the forced-removal channel-cleanup gap: tearing a session
    /// down must release its channel state exactly like graceful disconnect — a
    /// co-channel member receives `ChatUserLeft` for the departed nickname, and a
    /// channel the session was alone in (ephemeral) is reaped. Mirrors the forced
    /// path: `remove_users` first, then `cleanup_resources` on the owned copies.
    #[tokio::test]
    async fn test_cleanup_resources_emits_chat_user_left_and_reaps_empty_channel() {
        let user_manager = UserManager::new();
        let voice_registry = VoiceRegistry::new();
        let channel_manager =
            ChannelManager::new(ChannelDb::new(create_test_db().await), UserManager::new());

        // observer survives; victim is the one being torn down.
        let (obs_tx, mut obs_rx) = mpsc::unbounded_channel();
        let observer = user_manager
            .add_user(regular_session(1, "observer", obs_tx))
            .await
            .unwrap();
        let (victim_tx, _victim_rx) = mpsc::unbounded_channel();
        let victim = user_manager
            .add_user(regular_session(2, "victim", victim_tx))
            .await
            .unwrap();

        // #general has both; #solo has only the victim.
        channel_manager
            .join("#general", observer, JoinPolicy::CreateIfMissing)
            .await
            .unwrap();
        channel_manager
            .join("#general", victim, JoinPolicy::CreateIfMissing)
            .await
            .unwrap();
        channel_manager
            .join("#solo", victim, JoinPolicy::CreateIfMissing)
            .await
            .unwrap();

        // Forced-path order: remove from the map, then clean up the owned copy.
        let removed = user_manager.remove_users(&[victim]).await;
        assert_eq!(removed.len(), 1);
        cleanup_resources(
            &user_manager,
            &voice_registry,
            &channel_manager,
            &removed,
            true,
        )
        .await;

        // The surviving member is told the victim left #general (last-leave rule);
        // #solo emptied, so it's reaped before it could emit a leave.
        let mut left_channels: Vec<String> = Vec::new();
        while let Ok((msg, _)) = obs_rx.try_recv() {
            if let ServerMessage::ChatUserLeft { channel, nickname } = msg {
                assert_eq!(nickname, "victim");
                left_channels.push(channel);
            }
        }
        assert_eq!(
            left_channels,
            vec!["#general".to_string()],
            "co-channel member must get exactly one ChatUserLeft for the removed session"
        );

        // Victim dropped from #general (observer remains); #solo emptied → reaped.
        let general = channel_manager
            .get_members("#general")
            .await
            .expect("#general persists while observer remains");
        assert!(general.contains(&observer) && !general.contains(&victim));
        assert!(
            channel_manager.get_members("#solo").await.is_none(),
            "an emptied ephemeral channel must be removed"
        );
    }
}
