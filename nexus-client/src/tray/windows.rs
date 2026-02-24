//! Windows system tray implementation using tray-icon
//!
//! This implementation uses the native Windows system tray API via tray-icon.
//! Left-click toggles window visibility, right-click shows the menu.

use std::pin::Pin;
use std::time::Duration;

use crossbeam_channel::TryRecvError;
use iced::Subscription;
use iced::futures::Stream;
use iced::stream;

use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use super::{TRAY_SERVICE_CLOSED_DELAY, TrayState};
use crate::i18n::t;
use crate::types::Message;

/// Icon width/height in pixels
const ICON_SIZE: u32 = 32;

/// Menu item ID for show/hide window toggle
const MENU_ID_SHOW_HIDE: &str = "show_hide";

/// Menu item ID for mute/unmute toggle
const MENU_ID_MUTE: &str = "mute";

/// Menu item ID for quit
const MENU_ID_QUIT: &str = "quit";

/// Poll interval for checking crossbeam receivers in the async stream.
/// crossbeam-channel doesn't have an async API, so we use try_recv with sleep.
/// 100ms balances responsiveness (perceptible click latency) with CPU efficiency.
const RECV_POLL_INTERVAL_MS: u64 = 100;

// =============================================================================
// Tray Manager
// =============================================================================

/// Manages the system tray icon and menu on Windows
pub struct TrayManager {
    /// The tray icon instance
    tray_icon: TrayIcon,
    /// Show/Hide menu item (text changes based on window visibility)
    show_hide_item: MenuItem,
    /// Mute/Unmute menu item (text changes based on deafen state)
    mute_item: MenuItem,
    /// Current tray state
    current_state: TrayState,
    /// Whether window is currently visible
    window_visible: bool,
    /// Whether mute option is enabled (only when in voice)
    mute_enabled: bool,
    /// Whether user is currently deafened
    is_deafened: bool,
}

impl TrayManager {
    /// Create a new tray manager
    ///
    /// Returns `None` if tray creation fails (e.g., no system tray available).
    pub fn new() -> Option<Self> {
        // Load initial icon
        let icon = load_icon(TrayState::Disconnected.icon_data())?;

        // Create menu items
        let show_hide_item =
            MenuItem::with_id(MENU_ID_SHOW_HIDE, t("tray-hide-window"), true, None);
        let mute_item = MenuItem::with_id(MENU_ID_MUTE, t("tray-mute"), false, None); // Initially disabled
        let quit_item = MenuItem::with_id(MENU_ID_QUIT, t("tray-quit"), true, None);

        // Build menu
        let menu = Menu::new();
        let _ = menu.append(&show_hide_item);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&mute_item);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&quit_item);

        // Create tray icon
        let tray_icon = TrayIconBuilder::new()
            .with_icon(icon)
            .with_tooltip(t("tray-tooltip-disconnected"))
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false) // Left-click sends event, right-click shows menu
            .build()
            .ok()?;

        Some(Self {
            tray_icon,
            show_hide_item,
            mute_item,
            current_state: TrayState::Disconnected,
            window_visible: true,
            mute_enabled: false,
            is_deafened: false,
        })
    }

    /// Update the tray icon state
    pub fn update_state(&mut self, state: TrayState) {
        if self.current_state == state {
            return;
        }

        self.current_state = state;

        if let Some(icon) = load_icon(state.icon_data()) {
            let _ = self.tray_icon.set_icon(Some(icon));
        }
    }

    /// Update the tooltip text
    pub fn update_tooltip(&mut self, tooltip: &str) {
        let _ = self.tray_icon.set_tooltip(Some(tooltip));
    }

    /// Update the show/hide menu item based on window visibility
    pub fn set_window_visible(&mut self, visible: bool) {
        if self.window_visible == visible {
            return;
        }

        self.window_visible = visible;

        let text = if visible {
            t("tray-hide-window")
        } else {
            t("tray-show-window")
        };
        self.show_hide_item.set_text(text);
    }

    /// Enable or disable the mute menu option
    pub fn set_mute_enabled(&mut self, enabled: bool) {
        if self.mute_enabled == enabled {
            return;
        }

        self.mute_enabled = enabled;
        self.mute_item.set_enabled(enabled);

        // Reset text when disabled
        if !enabled {
            self.mute_item.set_text(t("tray-mute"));
        }
    }

    /// Update the mute/unmute text based on deafen state
    pub fn set_deafened(&mut self, deafened: bool) {
        if self.is_deafened == deafened {
            return;
        }

        self.is_deafened = deafened;

        let text = if deafened {
            t("tray-unmute")
        } else {
            t("tray-mute")
        };
        self.mute_item.set_text(text);
    }
}

// =============================================================================
// Icon Loading
// =============================================================================

/// Load an icon from PNG bytes
fn load_icon(data: &[u8]) -> Option<Icon> {
    // Decode PNG to RGBA
    let image = image::load_from_memory(data).ok()?;
    let rgba = image.to_rgba8();

    // Verify dimensions
    if rgba.width() != ICON_SIZE || rgba.height() != ICON_SIZE {
        // Try to resize if needed
        let resized = image::imageops::resize(
            &rgba,
            ICON_SIZE,
            ICON_SIZE,
            image::imageops::FilterType::Lanczos3,
        );
        Icon::from_rgba(resized.into_raw(), ICON_SIZE, ICON_SIZE).ok()
    } else {
        Icon::from_rgba(rgba.into_raw(), ICON_SIZE, ICON_SIZE).ok()
    }
}

// =============================================================================
// Subscription
// =============================================================================

/// Create an event-driven subscription for tray icon events.
///
/// Instead of polling every 50ms through iced's update/view cycle, this uses
/// an async stream that checks the crossbeam receivers with a sleep interval.
/// The iced event loop only wakes when an actual tray event arrives.
pub fn tray_subscription() -> Subscription<Message> {
    Subscription::run(tray_event_stream)
}

/// Async stream that polls tray-icon's crossbeam receivers for events.
///
/// crossbeam-channel doesn't have an async API, so we use try_recv with
/// a small sleep. This is similar to how PTT hotkey events are handled.
fn tray_event_stream() -> Pin<Box<dyn Stream<Item = Message> + Send>> {
    Box::pin(stream::channel(
        16,
        |mut output: iced::futures::channel::mpsc::Sender<Message>| async move {
            use iced::futures::SinkExt;

            'poll: loop {
                // Check for tray icon events (clicks)
                let tray_receiver = TrayIconEvent::receiver();
                match tray_receiver.try_recv() {
                    Ok(TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    }) => {
                        let _ = output.send(Message::TrayIconClicked).await;
                        continue;
                    }
                    Ok(_) => {
                        // Other tray events (right-click, double-click, mouse down, etc.)
                    }
                    Err(TryRecvError::Empty) => {}
                    Err(TryRecvError::Disconnected) => break 'poll,
                }

                // Check for menu events
                let menu_receiver = MenuEvent::receiver();
                match menu_receiver.try_recv() {
                    Ok(event) => {
                        let message = match event.id.0.as_str() {
                            MENU_ID_SHOW_HIDE => Some(Message::TrayMenuShowHide),
                            MENU_ID_MUTE => Some(Message::TrayMenuMute),
                            MENU_ID_QUIT => Some(Message::TrayMenuQuit),
                            _ => None,
                        };
                        if let Some(msg) = message {
                            let _ = output.send(msg).await;
                            continue;
                        }
                    }
                    Err(TryRecvError::Empty) => {}
                    Err(TryRecvError::Disconnected) => break 'poll,
                }

                // No events, sleep briefly before checking again
                tokio::time::sleep(Duration::from_millis(RECV_POLL_INTERVAL_MS)).await;
            }

            // Receiver disconnected — signal service closed so the app
            // can attempt to recreate the tray.
            tokio::time::sleep(TRAY_SERVICE_CLOSED_DELAY).await;
            let _ = output.send(Message::TrayServiceClosed).await;
        },
    ))
}
