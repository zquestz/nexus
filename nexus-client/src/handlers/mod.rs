//! Message handlers organized by category

mod bookmarks;
mod broadcast;
mod connection;
mod connection_monitor;
mod files;
mod fingerprint;
mod focus;
mod group_management;
mod keyboard;
pub(crate) mod network;
mod news;
mod server_info;
mod settings;
mod tracker_browser;
mod tracker_management;
mod transfers;
#[cfg(not(target_os = "macos"))]
mod tray;
mod ui;
mod uri;
mod user_management;
mod voice;
