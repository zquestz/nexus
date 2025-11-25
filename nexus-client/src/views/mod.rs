//! UI view rendering components

mod bookmark;
mod broadcast;
mod chat;
mod colors;
mod connection;
mod layout;
mod server_list;
mod style;
mod user_list;
mod users;

// Re-export the main layout function (public API)
pub use layout::main_layout;
