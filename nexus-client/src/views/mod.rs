//! UI view rendering components

// Module declarations
mod admin;
mod bookmark;
mod chat;
mod connection;
mod layout;
mod server_list;
mod user_list;

// Re-export the main layout function (public API)
pub use layout::main_layout;