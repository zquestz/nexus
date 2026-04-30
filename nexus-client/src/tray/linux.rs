//! Linux system tray implementation using ksni (StatusNotifierItem)
//!
//! This implementation uses the D-Bus StatusNotifierItem protocol directly,
//! which supports left-click activation (unlike libappindicator).

use std::pin::Pin;
use std::sync::OnceLock;

use iced::Subscription;
use iced::futures::Stream;
use iced::stream;
use tokio::sync::Mutex as AsyncMutex;
use tokio::sync::mpsc;

use ksni::menu::{MenuItem as KsniMenuItem, StandardItem};
use ksni::{Icon as KsniIcon, Tray, TrayMethods};

use super::{BYTES_PER_PIXEL, TRAY_ID, TRAY_SERVICE_CLOSED_DELAY, TRAY_TITLE, TrayState};
use crate::constants::ERR_TRAY_RX_UNINITIALIZED;
use crate::i18n::t;
use crate::types::Message;

/// Global receiver for tray events, set once by `TrayManager::new()`.
/// The subscription stream takes ownership on first access.
static TRAY_RX: OnceLock<AsyncMutex<Option<mpsc::UnboundedReceiver<Message>>>> = OnceLock::new();

// =============================================================================
// Tray Implementation
// =============================================================================

/// Internal tray state for ksni
struct NexusTray {
    /// Channel to send events back to the main app
    tx: mpsc::UnboundedSender<Message>,
    /// Current tray state (for icon selection)
    state: TrayState,
    /// Whether window is currently visible
    window_visible: bool,
    /// Whether mute option is enabled (only when in voice)
    mute_enabled: bool,
    /// Whether user is currently deafened
    is_deafened: bool,
}

impl Tray for NexusTray {
    // Left-click should call activate(), not show menu
    const MENU_ON_ACTIVATE: bool = false;

    fn id(&self) -> String {
        TRAY_ID.into()
    }

    fn title(&self) -> String {
        TRAY_TITLE.into()
    }

    fn icon_pixmap(&self) -> Vec<KsniIcon> {
        let icon_data = self.state.icon_data();
        load_icon_pixmap(icon_data).unwrap_or_default()
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        // Left-click: toggle window visibility
        let _ = self.tx.send(Message::TrayIconClicked);
    }

    fn menu(&self) -> Vec<KsniMenuItem<Self>> {
        let mut items = Vec::new();

        // Show/Hide Window
        let show_hide_label = if self.window_visible {
            t("tray-hide-window")
        } else {
            t("tray-show-window")
        };
        items.push(
            StandardItem {
                label: show_hide_label,
                activate: Box::new(|this: &mut Self| {
                    let _ = this.tx.send(Message::TrayMenuShowHide);
                }),
                ..Default::default()
            }
            .into(),
        );

        items.push(KsniMenuItem::Separator);

        // Mute/Unmute (only enabled when in voice)
        let mute_label = if self.is_deafened {
            t("tray-unmute")
        } else {
            t("tray-mute")
        };
        items.push(
            StandardItem {
                label: mute_label,
                enabled: self.mute_enabled,
                activate: Box::new(|this: &mut Self| {
                    let _ = this.tx.send(Message::TrayMenuMute);
                }),
                ..Default::default()
            }
            .into(),
        );

        items.push(KsniMenuItem::Separator);

        // Quit
        items.push(
            StandardItem {
                label: t("tray-quit"),
                activate: Box::new(|this: &mut Self| {
                    let _ = this.tx.send(Message::TrayMenuQuit);
                }),
                ..Default::default()
            }
            .into(),
        );

        items
    }
}

// =============================================================================
// Icon Loading
// =============================================================================

/// Load a PNG icon and convert to ksni's ARGB format
fn load_icon_pixmap(data: &[u8]) -> Option<Vec<KsniIcon>> {
    let image = image::load_from_memory(data).ok()?;
    let rgba = image.to_rgba8();

    let width = rgba.width() as i32;
    let height = rgba.height() as i32;

    // ksni expects ARGB in network byte order (big-endian)
    // image crate gives us RGBA, so we need to convert
    let mut argb_data = Vec::with_capacity((width * height) as usize * BYTES_PER_PIXEL);
    for pixel in rgba.pixels() {
        let [r, g, b, a] = pixel.0;
        // ARGB in big-endian byte order
        argb_data.push(a);
        argb_data.push(r);
        argb_data.push(g);
        argb_data.push(b);
    }

    Some(vec![KsniIcon {
        width,
        height,
        data: argb_data,
    }])
}

// =============================================================================
// Tray Manager
// =============================================================================

/// Manages the system tray icon and menu on Linux
pub struct TrayManager {
    /// Handle to update the tray
    handle: ksni::Handle<NexusTray>,
    /// Current state (cached for comparison)
    current_state: TrayState,
}

impl TrayManager {
    /// Create a new tray manager
    ///
    /// Returns `None` if tray creation fails.
    pub fn new() -> Option<Self> {
        // Create channel for events
        let (tx, rx) = mpsc::unbounded_channel();

        // Store the receiver in the global static for the subscription to consume.
        // On first call, OnceLock initializes with the receiver.
        // On subsequent calls (tray recreated after D-Bus death), we replace the
        // inner receiver with the new one.
        // We use block_in_place + block_on here (same as the tray spawn below)
        // to guarantee the receiver is stored. try_lock() would silently drop
        // the receiver if contended, leaving the tray icon visible but
        // unresponsive to clicks.
        let lock = TRAY_RX.get_or_init(|| AsyncMutex::new(None));
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let mut guard = lock.lock().await;
                *guard = Some(rx);
            });
        });

        // Create the tray
        let tray = NexusTray {
            tx,
            state: TrayState::Disconnected,
            window_visible: true,
            mute_enabled: false,
            is_deafened: false,
        };

        // Spawn the tray service
        // We need to spawn this in a way that works with the existing tokio runtime
        let handle = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async { tray.spawn().await.ok() })
        })?;

        Some(Self {
            handle,
            current_state: TrayState::Disconnected,
        })
    }

    /// Update the tray icon state
    pub fn update_state(&mut self, state: TrayState) {
        if self.current_state == state {
            return;
        }

        self.current_state = state;

        let handle = self.handle.clone();
        tokio::spawn(async move {
            handle.update(|tray| tray.state = state).await;
        });
    }

    /// Update the tooltip text
    ///
    /// Note: ksni doesn't support dynamic tooltips in the same way,
    /// the title is used instead which updates on hover.
    pub fn update_tooltip(&mut self, _tooltip: &str) {
        // ksni uses title() for the tooltip, which is static.
        // We could store tooltip and return it from title(), but that
        // would require more refactoring. For now, we use a static title.
    }

    /// Update the show/hide menu item based on window visibility
    pub fn set_window_visible(&mut self, visible: bool) {
        let handle = self.handle.clone();
        tokio::spawn(async move {
            handle
                .update(|tray| {
                    if tray.window_visible != visible {
                        tray.window_visible = visible;
                    }
                })
                .await;
        });
    }

    /// Enable or disable the mute menu option
    pub fn set_mute_enabled(&mut self, enabled: bool) {
        let handle = self.handle.clone();
        tokio::spawn(async move {
            handle
                .update(|tray| {
                    if tray.mute_enabled != enabled {
                        tray.mute_enabled = enabled;
                    }
                })
                .await;
        });
    }

    /// Update the mute/unmute text based on deafen state
    pub fn set_deafened(&mut self, deafened: bool) {
        let handle = self.handle.clone();
        tokio::spawn(async move {
            handle
                .update(|tray| {
                    if tray.is_deafened != deafened {
                        tray.is_deafened = deafened;
                    }
                })
                .await;
        });
    }
}

impl Drop for TrayManager {
    fn drop(&mut self) {
        // Shutdown the ksni tray service to remove the icon.
        // Use try_current() because the tokio runtime may already be
        // shut down when Drop runs during app exit. If no runtime is
        // available, the tray icon is cleaned up when the process exits.
        let handle = self.handle.clone();
        if let Ok(rt) = tokio::runtime::Handle::try_current() {
            rt.spawn(async move {
                handle.shutdown().await;
            });
        }
    }
}

// =============================================================================
// Subscription
// =============================================================================

/// Create an event-driven subscription for tray icon events.
///
/// Instead of polling every 50ms through iced's update/view cycle, this
/// awaits the tray event channel directly. The iced event loop only wakes
/// when an actual tray event arrives (menu click, left-click on icon, etc.).
pub fn tray_subscription() -> Subscription<Message> {
    Subscription::run(tray_event_stream)
}

/// Async stream that awaits tray events from the ksni channel.
fn tray_event_stream() -> Pin<Box<dyn Stream<Item = Message> + Send>> {
    Box::pin(stream::channel(
        16,
        |mut output: iced::futures::channel::mpsc::Sender<Message>| async move {
            use iced::futures::SinkExt;

            // Take the receiver from the global static
            let rx = TRAY_RX.get().expect(ERR_TRAY_RX_UNINITIALIZED);

            let mut rx = {
                let mut guard = rx.lock().await;
                match guard.take() {
                    Some(rx) => rx,
                    None => {
                        // Receiver already taken — shouldn't happen since iced
                        // deduplicates subscriptions, but sleep forever as fallback
                        std::future::pending::<()>().await;
                        return;
                    }
                }
            };

            loop {
                match rx.recv().await {
                    Some(msg) => {
                        let _ = output.send(msg).await;
                    }
                    None => {
                        // Channel closed — tray was dropped or D-Bus died.
                        // Wait briefly then signal service closed so the app
                        // can attempt to recreate the tray.
                        tokio::time::sleep(TRAY_SERVICE_CLOSED_DELAY).await;
                        let _ = output.send(Message::TrayServiceClosed).await;
                        break;
                    }
                }
            }
        },
    ))
}
