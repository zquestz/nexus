//! Voice chat for channels and user messages. State is entirely in-memory
//! (ephemeral) — no database persistence.
//!
//! Rules: one voice session per user on this server; channel voice requires
//! channel membership; user message voice requires the target user online.

mod demux;
mod registry;
mod session;
mod udp;

use nexus_common::protocol::ServerMessage;

use crate::channels::ChannelManager;
use crate::constants::FEATURE_VOICE;
use crate::db::Permission;
use crate::users::UserManager;
use crate::users::user::ConnectionWriter;

pub use registry::{VoiceLeaveInfo, VoiceRegistry};
pub use session::VoiceSession;
#[cfg(test)]
pub use udp::VoiceControlCommand;
pub use udp::{VoiceControlHandle, VoiceUdpServer, create_voice_listener};

/// Single source of truth for VoiceUserLeft notifications across every
/// cleanup path (normal disconnect, kick/delete/disable/ban, DTLS timeout).
pub async fn send_voice_leave_notifications(
    info: &VoiceLeaveInfo,
    leaving_user_tx: Option<&ConnectionWriter>,
    user_manager: &UserManager,
    channel_manager: &ChannelManager,
) {
    if let Some(tx) = leaving_user_tx {
        let self_notification = ServerMessage::VoiceUserLeft {
            nickname: info.session.nickname.clone(),
            target: info.self_target.clone(),
        };
        let _ = tx.send_message(self_notification, None);
    }

    // Only the last session of a nickname broadcasts a leave.
    if info.should_broadcast {
        if info.session.is_channel() {
            // Channels broadcast to ALL voice-capable members with voice_listen
            // (not just voice participants) so everyone sees who's in voice.
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
                    && member.has_feature(FEATURE_VOICE)
                    && member.has_permission(Permission::VoiceListen)
                {
                    let leave_notification = ServerMessage::VoiceUserLeft {
                        nickname: info.session.nickname.clone(),
                        target: channel_name.clone(),
                    };
                    let _ = member.tx.send_message(leave_notification, None);
                }
            }
        } else {
            // User messages: notify the other DM participant even if they
            // are not currently in voice, so their waiting indicator clears.
            // Self-DM voice has no "other" participant; `self_target` is empty.
            if !info.self_target.is_empty() {
                let leave_notification = ServerMessage::VoiceUserLeft {
                    nickname: info.session.nickname.clone(),
                    target: info.broadcast_target.clone(),
                };

                user_manager
                    .broadcast_to_nickname_with_feature_and_permission(
                        &info.self_target,
                        FEATURE_VOICE,
                        Permission::VoiceListen,
                        &leave_notification,
                    )
                    .await;
            }
        }
    }
}
