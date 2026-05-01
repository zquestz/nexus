//! Tracker publisher infrastructure.
//!
//! Per-tracker async tasks maintain long-lived TLS connections to
//! admin-configured trackers and refresh the registration on the
//! tracker-supplied interval. The supervisor — [`TrackerManager`] —
//! spawns / replaces / terminates tasks in response to admin
//! protocol-level operations on the `trackers` DB table.
//!
//! See `docs/TODO.md` § "Server-Side Publisher Implementation Plan"
//! for the full design rationale.

mod context;
mod manager;
mod status;
mod task;
mod tls;

pub use context::PublisherContext;
pub use manager::TrackerManager;
pub use status::{TrackerStatus, is_unrecoverable_error_kind};
