//! Tracker registration infrastructure.
//!
//! Per-tracker async tasks hold long-lived TLS connections to configured
//! trackers and refresh on the tracker-supplied interval. The supervisor
//! [`TrackerManager`] spawns/replaces/terminates tasks as admins mutate
//! the `trackers` DB table.

mod context;
mod manager;
mod status;
mod task;
#[cfg(test)]
pub(crate) mod testing;
mod tls;

pub use context::TrackerContext;
pub use manager::TrackerManager;
pub use status::TrackerStatus;
