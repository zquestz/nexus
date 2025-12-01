//! UI view rendering components

mod bookmark;
mod broadcast;
mod chat;
mod connection;
pub(crate) mod constants;
mod fingerprint;
mod layout;
mod server_list;
mod settings;
mod user_list;
mod users;

// Re-export the main layout function and fingerprint dialog (public API)
pub use fingerprint::fingerprint_mismatch_dialog;
pub use layout::main_layout;
