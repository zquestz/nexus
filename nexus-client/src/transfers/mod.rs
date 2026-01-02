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

mod executor;
mod persistence;
mod subscription;
mod types;

pub use executor::TransferEvent;
pub use persistence::TransferManager;
pub use subscription::{request_cancel, transfer_subscription};
#[allow(unused_imports)] // TransferDirection kept for API completeness (Upload support)
pub use types::{
    Transfer, TransferConnectionInfo, TransferDirection, TransferError, TransferStatus,
};
