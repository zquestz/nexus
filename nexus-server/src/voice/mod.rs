//! Voice chat for channels and user messages. State is entirely in-memory
//! (ephemeral) — no database persistence.
//!
//! Rules: one voice session per user on this server; channel voice requires
//! channel membership; user message voice requires the target user online.

mod registry;
mod session;
mod udp;

use nexus_common::protocol::ServerMessage;

use crate::channels::ChannelManager;
use crate::db::Permission;
use crate::users::UserManager;
use crate::users::user::{SessionEvent, SessionTx};

pub use registry::{VoiceLeaveInfo, VoiceRegistry};
pub use session::VoiceSession;
pub use udp::{VoiceUdpServer, create_voice_listener};

/// Single source of truth for VoiceUserLeft notifications across every
/// cleanup path (normal disconnect, kick/delete/disable/ban, DTLS timeout).
pub async fn send_voice_leave_notifications(
    info: &VoiceLeaveInfo,
    leaving_user_tx: Option<&SessionTx>,
    user_manager: &UserManager,
    channel_manager: &ChannelManager,
) {
    if let Some(tx) = leaving_user_tx {
        let self_notification = ServerMessage::VoiceUserLeft {
            nickname: info.session.nickname.clone(),
            target: info.self_target.clone(),
        };
        let _ = tx.send(SessionEvent::message(self_notification, None));
    }

    // Only the last session of a nickname broadcasts a leave.
    if info.should_broadcast {
        if info.session.is_channel() {
            // Channels broadcast to ALL members with voice_listen (not just
            // voice participants) so everyone sees who's in voice.
            let channel_name = info.session.target.first().cloned().unwrap_or_default();
            let members = channel_manager
                .get_members(&channel_name)
                .await
                .unwrap_or_default();

            for member_session_id in members {
                if member_session_id == info.session.session_id {
                    continue;
                }

                if let Some(member) = user_manager.get_user_by_session_id(member_session_id).await
                    && member.has_permission(Permission::VoiceListen)
                {
                    let leave_notification = ServerMessage::VoiceUserLeft {
                        nickname: info.session.nickname.clone(),
                        target: channel_name.clone(),
                    };
                    let _ = member
                        .tx
                        .send(SessionEvent::message(leave_notification, None));
                }
            }
        } else {
            // User messages: only notify the other participant.
            for participant_nickname in &info.remaining_participants {
                let leave_notification = ServerMessage::VoiceUserLeft {
                    nickname: info.session.nickname.clone(),
                    target: info.broadcast_target.clone(),
                };

                if let Some(participant) = user_manager
                    .get_session_by_nickname(participant_nickname)
                    .await
                {
                    let _ = participant
                        .tx
                        .send(SessionEvent::message(leave_notification, None));
                }
            }
        }
    }
}
