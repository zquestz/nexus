//! UI view rendering components

mod bookmark;
mod broadcast;
mod chat;
mod colors;
pub(crate) mod constants;
mod connection;
mod fingerprint;
mod layout;
mod server_list;
mod style;
mod user_list;
mod users;

// Re-export the main layout function and fingerprint dialog (public API)
pub use fingerprint::fingerprint_mismatch_dialog;
pub use layout::main_layout;
