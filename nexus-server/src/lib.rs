//! Nexus BBS Server Library
//!
//! This library exposes the server's internal modules for integration testing.

pub mod constants;
pub mod db;
pub mod users;

// Re-export commonly used items for convenience
pub use db::Database;
pub use users::UserManager;
