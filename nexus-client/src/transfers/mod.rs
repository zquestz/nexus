//! File transfer management
//!
//! This module handles file transfers (downloads and uploads) which operate on a separate
//! port (7501) from the main BBS protocol. Transfers are persisted to disk to support
//! resume across application restarts.
//!
//! Key types:
//! - `Transfer` - A single file or directory transfer
//! - `TransferConnectionInfo` - Connection details needed to reconnect for resume
//! - `TransferManager` - Manages all transfers and persistence
//! - `TransferEvent` - Progress events from the executor

// Allow dead code and unused imports for items not yet integrated.
// TODO: Remove this once file transfers are fully implemented.
#![allow(dead_code, unused_imports)]

mod executor;
mod persistence;
mod subscription;
mod types;

pub use executor::{TransferEvent, execute_transfer};
pub use persistence::TransferManager;
pub use subscription::transfer_subscription;
pub use types::{
    Transfer, TransferConnectionInfo, TransferDirection, TransferError, TransferStatus,
};
