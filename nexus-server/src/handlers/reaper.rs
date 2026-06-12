//! Dead-session reaper.
//!
//! Broadcasts that find a session's `ConnectionWriter` closed enqueue its id
//! here instead of cleaning up inline — they hold no channel/voice context and
//! may run under `read_user_state`. This task drains those ids and runs the
//! canonical disconnect teardown for them, acquiring `read_user_state` fresh in
//! its own task. Cleanup therefore never recurses into a broadcast helper or
//! nests a lock acquisition, making the write-preferring `RwLock` deadlock
//! impossible.

use tokio::sync::mpsc;

use crate::channels::ChannelManager;
use crate::users::UserManager;
use crate::voice::VoiceRegistry;

use super::remove_user_sessions_with_cleanup;

/// Drain dead session ids and tear them down via the canonical path. Runs until
/// every `UserManager` clone (and thus every sender) is dropped at shutdown.
pub async fn run_dead_session_reaper(
    mut dead_session_rx: mpsc::UnboundedReceiver<u32>,
    user_manager: UserManager,
    voice_registry: VoiceRegistry,
    channel_manager: ChannelManager,
) {
    while let Some(first) = dead_session_rx.recv().await {
        reap_pending(
            first,
            &mut dead_session_rx,
            &user_manager,
            &voice_registry,
            &channel_manager,
        )
        .await;
    }
}

/// Tear down `first` plus everything else currently queued, in one canonical
/// pass. Batches + dedupes so a fan-out burst that hit the same dead receiver
/// many times collapses to a single teardown. Returns the deduped ids reaped.
///
/// `notify_leaving_user = false`: the receiver is closed, so its own
/// `VoiceUserLeft` can't be delivered. `remove_users` is the serialization
/// point, so an id already swept by a normal disconnect is a no-op here.
pub(super) async fn reap_pending(
    first: u32,
    dead_session_rx: &mut mpsc::UnboundedReceiver<u32>,
    user_manager: &UserManager,
    voice_registry: &VoiceRegistry,
    channel_manager: &ChannelManager,
) -> Vec<u32> {
    let mut session_ids = vec![first];
    while let Ok(session_id) = dead_session_rx.try_recv() {
        session_ids.push(session_id);
    }
    session_ids.sort_unstable();
    session_ids.dedup();

    remove_user_sessions_with_cleanup(
        user_manager,
        voice_registry,
        channel_manager,
        &session_ids,
        false,
    )
    .await;

    session_ids
}
