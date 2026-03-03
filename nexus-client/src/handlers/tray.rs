//! System tray message handlers (Windows/Linux only)

#![cfg(not(target_os = "macos"))]

use std::time::Duration;

use iced::Task;
use iced::widget::{Id, operation};

use crate::NexusApp;
use crate::i18n::t;
use crate::types::{ActivePanel, ChatMessage, ChatTab, InputId, Message};

/// Delay before restoring focus after window show/restore.
///
/// Winit processes `set_visible(true)` and `set_minimized(false)` via
/// `execute_in_thread`, queuing them on the window thread. A short timer
/// ensures those queued `ShowWindow` calls have been processed before we
/// attempt to claim keyboard focus — otherwise the window may not yet be
/// in a visible, non-minimized state when `SetFocus` (Windows) or
/// `gain_focus` (Linux) runs.
const DEFERRED_FOCUS_DELAY: Duration = Duration::from_millis(100);

/// Deferred focus restore after window show/restore.
fn deferred_focus_task(id: iced::window::Id) -> Task<Message> {
    Task::future(async move {
        tokio::time::sleep(DEFERRED_FOCUS_DELAY).await;
        Message::TrayRestoreFocus(id)
    })
}

impl NexusApp {
    /// Handle tray icon click - toggle window visibility
    pub fn handle_tray_icon_clicked(&mut self) -> Task<Message> {
        self.toggle_window_visibility()
    }

    /// Handle show/hide menu item
    pub fn handle_tray_show_hide(&mut self) -> Task<Message> {
        self.toggle_window_visibility()
    }

    /// Handle mute/unmute menu item - toggle deafen state
    pub fn handle_tray_mute(&mut self) -> Task<Message> {
        // Only works if we're in a voice session
        if self.active_voice_connection.is_some() {
            self.handle_voice_deafen_toggle()
        } else {
            Task::none()
        }
    }

    /// Handle quit menu item - proper application shutdown
    pub fn handle_tray_quit(&mut self) -> Task<Message> {
        // When the window is hidden to tray, querying position returns the
        // Windows minimized parking coordinates (-32000, -32000). Use the
        // last-known good values from config instead.
        if !self.window_visible {
            let width = self.config.settings.window_width;
            let height = self.config.settings.window_height;
            let x = self.config.settings.window_x;
            let y = self.config.settings.window_y;
            return iced::window::oldest().then(move |opt_id| {
                if let Some(id) = opt_id {
                    Task::done(Message::WindowSaveAndClose {
                        id,
                        width,
                        height,
                        x,
                        y,
                    })
                } else {
                    Task::none()
                }
            });
        }

        // Get the oldest window and trigger a close that bypasses minimize-to-tray
        iced::window::oldest().then(|opt_id| {
            if let Some(id) = opt_id {
                // Query window size and position, then save and close
                iced::window::size(id).then(move |size| {
                    iced::window::position(id).map(move |point| Message::WindowSaveAndClose {
                        id,
                        width: size.width,
                        height: size.height,
                        x: point.map(|p| p.x as i32),
                        y: point.map(|p| p.y as i32),
                    })
                })
            } else {
                Task::none()
            }
        })
    }

    /// Toggle window visibility (show/hide)
    fn toggle_window_visibility(&mut self) -> Task<Message> {
        if self.window_visible {
            // Window is "visible" but might be minimized - check before hiding
            // If minimized, restore it instead of hiding
            iced::window::oldest().then(|opt_id| {
                if let Some(id) = opt_id {
                    iced::window::is_minimized(id).then(move |is_minimized| {
                        if is_minimized.unwrap_or(false) {
                            // Window is minimized - restore it
                            // Query actual maximized state to restore correctly
                            iced::window::is_maximized(id).map(move |maximized| {
                                Message::TrayRestoreMinimized { id, maximized }
                            })
                        } else {
                            // Window is visible and not minimized - hide it
                            // Query size, position, and maximized state before hiding
                            iced::window::size(id).then(move |size| {
                                iced::window::position(id).then(move |point| {
                                    iced::window::is_maximized(id).map(move |maximized| {
                                        Message::TrayHideWindow {
                                            id,
                                            was_maximized: maximized,
                                            width: size.width,
                                            height: size.height,
                                            x: point.map(|p| p.x as i32),
                                            y: point.map(|p| p.y as i32),
                                        }
                                    })
                                })
                            })
                        }
                    })
                } else {
                    Task::none()
                }
            })
        } else {
            // Need to show from tray-hidden state
            iced::window::oldest().then(|opt_id| {
                if let Some(id) = opt_id {
                    Task::done(Message::TrayShowWindow(id))
                } else {
                    Task::none()
                }
            })
        }
    }

    /// Hide the window to tray (called after querying size, position, and maximized state)
    pub fn handle_tray_hide_window(
        &mut self,
        id: iced::window::Id,
        was_maximized: bool,
        width: f32,
        height: f32,
        x: Option<i32>,
        y: Option<i32>,
    ) -> Task<Message> {
        self.window_visible = false;
        self.window_was_maximized = was_maximized;

        // Save window geometry so tray quit uses the last-known good position
        // instead of the Windows minimized parking coordinates (-32000, -32000).
        self.config.settings.window_width = width;
        self.config.settings.window_height = height;
        self.config.settings.window_x = x;
        self.config.settings.window_y = y;

        if let Some(ref mut tray) = self.tray_manager {
            tray.set_window_visible(false);
        }

        // On Windows, minimize first to remove from taskbar, then hide.
        // Just using Hidden mode leaves a generic icon in the taskbar.
        #[cfg(target_os = "windows")]
        {
            Task::batch([
                iced::window::minimize(id, true),
                iced::window::set_mode(id, iced::window::Mode::Hidden),
            ])
        }

        #[cfg(not(target_os = "windows"))]
        {
            iced::window::set_mode(id, iced::window::Mode::Hidden)
        }
    }

    /// Restore a minimized window (not tray-hidden, just OS-minimized)
    ///
    /// `gain_focus` is deferred to the next update cycle via `TrayRestoreFocus`
    /// so that winit's cached window flags reflect the actual window state.
    pub fn handle_tray_restore_minimized(
        &mut self,
        id: iced::window::Id,
        was_maximized: bool,
    ) -> Task<Message> {
        if was_maximized {
            Task::batch([
                iced::window::minimize(id, false),
                iced::window::maximize(id, true),
                deferred_focus_task(id),
            ])
        } else {
            Task::batch([iced::window::minimize(id, false), deferred_focus_task(id)])
        }
    }

    /// Show the window from tray (restores maximized state if needed)
    ///
    /// `gain_focus` is deferred to the next update cycle via `TrayRestoreFocus`
    /// so that winit's cached window flags reflect the actual window state.
    pub fn handle_tray_show_window(&mut self, id: iced::window::Id) -> Task<Message> {
        self.window_visible = true;
        let was_maximized = self.window_was_maximized;

        if let Some(ref mut tray) = self.tray_manager {
            tray.set_window_visible(true);
        }

        // On Windows, we minimized before hiding, so we need to unminimize.
        // On other platforms, just set mode to Windowed.
        #[cfg(target_os = "windows")]
        {
            if was_maximized {
                Task::batch([
                    iced::window::minimize(id, false),
                    iced::window::set_mode(id, iced::window::Mode::Windowed),
                    iced::window::maximize(id, true),
                    deferred_focus_task(id),
                ])
            } else {
                Task::batch([
                    iced::window::minimize(id, false),
                    iced::window::set_mode(id, iced::window::Mode::Windowed),
                    deferred_focus_task(id),
                ])
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            if was_maximized {
                Task::batch([
                    iced::window::set_mode(id, iced::window::Mode::Windowed),
                    iced::window::maximize(id, true),
                    deferred_focus_task(id),
                ])
            } else {
                Task::batch([
                    iced::window::set_mode(id, iced::window::Mode::Windowed),
                    deferred_focus_task(id),
                ])
            }
        }
    }

    /// Update tray icon state based on current application state
    ///
    /// Called after state changes that might affect the tray icon.
    pub fn update_tray_state(&mut self) {
        use crate::tray::{TrayState, build_tooltip};

        // Early return if no tray manager
        if self.tray_manager.is_none() {
            return;
        }

        // Compute all the state we need BEFORE borrowing tray_manager
        let is_disconnected = self.connections.is_empty();
        let in_voice = self.active_voice_connection.is_some();
        let is_deafened = self.is_deafened;
        let is_speaking = self.is_local_speaking;
        let voice_target = self.get_voice_target();
        let unread_count = self.count_unread_user_messages();

        // Determine state (priority order)
        let state = if is_disconnected {
            TrayState::Disconnected
        } else if in_voice && is_deafened {
            TrayState::VoiceMuted
        } else if in_voice && is_speaking {
            TrayState::VoiceSpeaking
        } else if in_voice {
            TrayState::VoiceActive
        } else if unread_count > 0 {
            TrayState::Unread
        } else {
            TrayState::Normal
        };

        // Build tooltip
        let tooltip = build_tooltip(state, voice_target.as_deref(), unread_count);

        // Now borrow tray_manager mutably and update it
        if let Some(ref mut tray) = self.tray_manager {
            tray.update_state(state);
            tray.update_tooltip(&tooltip);
            tray.set_mute_enabled(in_voice);
            if in_voice {
                tray.set_deafened(is_deafened);
            }
        }
    }

    /// Handle deferred focus restore after window show/restore.
    ///
    /// This runs on the next update cycle after window mode changes, ensuring
    /// winit's cached flags are up to date. First applies OS-level window focus,
    /// then focuses the appropriate widget (ChatInput for chat view, last focused
    /// field for panels).
    ///
    /// On Windows, we call `SetFocus(hwnd)` via raw FFI instead of using iced's
    /// `gain_focus` (which calls `SetForegroundWindow`). This is necessary because
    /// winit's `apply_diff()` calls `SetWindowPos` with `SWP_NOACTIVATE` after
    /// `ShowWindow(SW_SHOW)`, which can break the keyboard input chain. When this
    /// happens, `WM_CHAR` messages fall through to `DefWindowProcW` which produces
    /// the system error beep on every keypress.
    ///
    /// `SetFocus` directly sends `WM_SETFOCUS` to the window, which resets winit's
    /// keyboard state machine via `gain_active_focus()`. The callback from
    /// `iced::window::run` executes on the window-owning thread, satisfying
    /// `SetFocus`'s thread affinity requirement.
    pub fn handle_tray_restore_focus(&mut self, id: iced::window::Id) -> Task<Message> {
        let widget_focus = if self.active_panel() == ActivePanel::None {
            operation::focus(Id::from(InputId::ChatInput))
        } else {
            operation::focus(Id::from(self.focused_field))
        };

        // On Windows, gain_focus brings the window to the foreground via
        // SetForegroundWindow, then SetFocus resets the keyboard input chain
        // by directly sending WM_SETFOCUS (fixing the beep-on-keypress bug).
        // gain_focus is fire-and-forget so batch ordering is sufficient —
        // iced processes actions from a batch in order.
        #[cfg(target_os = "windows")]
        let os_focus = Task::batch([
            iced::window::gain_focus(id),
            iced::window::run(id, |window| {
                if let Ok(handle) = window.window_handle() {
                    let raw = handle.as_raw();
                    if let iced::window::raw_window_handle::RawWindowHandle::Win32(w) = raw {
                        let hwnd = w.hwnd.get();
                        unsafe extern "system" {
                            safe fn SetFocus(hWnd: isize) -> isize;
                        }
                        SetFocus(hwnd);
                    }
                }
            })
            .discard(),
        ]);

        #[cfg(not(target_os = "windows"))]
        let os_focus = iced::window::gain_focus(id);

        Task::batch([os_focus, widget_focus])
    }

    /// Get the current voice target (channel name or user nickname)
    fn get_voice_target(&self) -> Option<String> {
        let conn_id = self.active_voice_connection?;
        let conn = self.connections.get(&conn_id)?;
        let voice_state = conn.voice_session.as_ref()?;
        Some(voice_state.target.clone())
    }

    /// Count total unread user message tabs across all connections
    fn count_unread_user_messages(&self) -> usize {
        let mut count = 0;
        for conn in self.connections.values() {
            for tab in &conn.unread_tabs {
                if matches!(tab, ChatTab::UserMessage(_)) {
                    count += 1;
                }
            }
        }
        count
    }

    /// Create or destroy the tray manager based on settings
    ///
    /// Returns a Task that may show an error message if tray creation fails.
    pub fn update_tray_from_settings(&mut self) -> Task<Message> {
        use crate::tray::TrayManager;

        if self.config.settings.show_tray_icon {
            // Create tray if not exists
            if self.tray_manager.is_none() {
                match TrayManager::new() {
                    Some(tray) => {
                        self.tray_manager = Some(tray);
                        self.update_tray_state();
                    }
                    None => {
                        // Tray creation failed - show window if hidden and show error
                        let show_task = self.show_window_if_hidden_to_tray();
                        let error_task = self.show_tray_error(t("err-tray-creation-failed"));
                        return Task::batch([show_task, error_task]);
                    }
                }
            }
        } else {
            // Destroy tray if exists
            if self.tray_manager.is_some() {
                self.tray_manager = None;
                return self.show_window_if_hidden_to_tray();
            }
        }
        Task::none()
    }

    /// Show the window if it was hidden to tray
    ///
    /// Used when tray is destroyed or recreation fails to ensure user isn't stuck
    /// with no way to access the application.
    fn show_window_if_hidden_to_tray(&mut self) -> Task<Message> {
        if !self.window_visible {
            self.window_visible = true;
            let was_maximized = self.window_was_maximized;
            return iced::window::oldest().then(move |opt_id| {
                if let Some(id) = opt_id {
                    #[cfg(target_os = "windows")]
                    {
                        if was_maximized {
                            Task::batch([
                                iced::window::minimize(id, false),
                                iced::window::set_mode(id, iced::window::Mode::Windowed),
                                iced::window::maximize(id, true),
                                deferred_focus_task(id),
                            ])
                        } else {
                            Task::batch([
                                iced::window::minimize(id, false),
                                iced::window::set_mode(id, iced::window::Mode::Windowed),
                                deferred_focus_task(id),
                            ])
                        }
                    }

                    #[cfg(not(target_os = "windows"))]
                    {
                        if was_maximized {
                            Task::batch([
                                iced::window::set_mode(id, iced::window::Mode::Windowed),
                                iced::window::maximize(id, true),
                                deferred_focus_task(id),
                            ])
                        } else {
                            Task::batch([
                                iced::window::set_mode(id, iced::window::Mode::Windowed),
                                deferred_focus_task(id),
                            ])
                        }
                    }
                } else {
                    Task::none()
                }
            });
        }
        Task::none()
    }

    /// Show a tray-related error to the user
    ///
    /// If connected, shows in active chat tab. Otherwise, shows in settings panel.
    fn show_tray_error(&mut self, error: String) -> Task<Message> {
        // If we have an active connection, show error in chat
        if let Some(conn_id) = self.active_connection {
            return self.add_active_tab_message(conn_id, ChatMessage::error(error));
        }

        // Otherwise, show in settings panel if it's open
        if let Some(ref mut form) = self.settings_form {
            form.error = Some(error);
        }

        Task::none()
    }
}
