//! UI view rendering components

mod about;
mod bookmark;
mod broadcast;
mod chat;
mod connection;
pub(crate) mod constants;
pub(crate) mod files;
mod fingerprint;
mod layout;
mod news;
mod server_info;
mod server_list;
mod settings;
mod user_info;
mod user_list;
mod users;

// Re-export the main layout function and fingerprint dialog (public API)
pub use fingerprint::fingerprint_mismatch_dialog;
pub use layout::main_layout;
