//! Nexus BBS Client - GUI Application
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autostart;
mod avatar;
mod commands;
mod config;
mod constants;
mod events;
mod fonts;
mod handlers;
mod history;
mod i18n;
mod icon;
mod image;
mod network;
mod sound;
mod style;
mod transfers;
mod types;
pub mod uri;
mod views;
mod voice;
mod widgets;

#[cfg(not(target_os = "macos"))]
mod tray;

#[cfg(target_os = "macos")]
mod macos_url;

use std::sync::Arc;
use std::sync::atomic::AtomicU32;

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Mutex;

use once_cell::sync::Lazy;

use uuid::Uuid;

use iced::widget::{Id, operation, text_editor};
use iced::{Element, Subscription, Task, Theme};
use iced_toasts::{ToastContainer, ToastLevel, toast, toast_container};

use style::toast_style;

use config::events::EventType;

use style::{WINDOW_HEIGHT_MIN, WINDOW_TITLE, WINDOW_WIDTH_MIN};
use types::{
    BookmarkEditState, ConnectionFormState, FingerprintMismatch, InputId, Message,
    ServerConnection, SettingsFormState, SettingsTab, UiState, ViewConfig,
};

/// Startup URI passed via command line (consumed by NexusApp::new)
static STARTUP_URI: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));

/// Get the IPC socket path for single-instance communication
fn get_ipc_socket_path() -> String {
    #[cfg(unix)]
    {
        // Linux: prefer XDG_RUNTIME_DIR, fallback to /tmp/nexus-$USER.sock
        // macOS: use TMPDIR
        if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
            return format!("{}/nexus.sock", runtime_dir);
        }
        if let Ok(tmpdir) = std::env::var("TMPDIR") {
            return format!("{}/nexus.sock", tmpdir.trim_end_matches('/'));
        }
        if let Ok(user) = std::env::var("USER") {
            return format!("/tmp/nexus-{}.sock", user);
        }
        "/tmp/nexus.sock".to_string()
    }
    #[cfg(windows)]
    {
        // Windows: use named pipe
        let username = std::env::var("USERNAME").unwrap_or_else(|_| "user".to_string());
        format!("nexus-{}", username)
    }
}

/// Try to send a URI to an existing instance
///
/// Returns Ok(true) if successfully sent to existing instance (caller should exit),
/// Returns Ok(false) if no existing instance (caller should become the primary),
/// Returns Err if something went wrong.
fn try_send_to_existing_instance(uri: &str) -> Result<bool, Box<dyn std::error::Error>> {
    #[cfg(unix)]
    use std::io::Write;
    use std::io::{BufRead, BufReader};
    #[cfg(unix)]
    use std::time::Duration;

    #[cfg(unix)]
    const IPC_TIMEOUT: Duration = Duration::from_secs(5);

    #[cfg(unix)]
    {
        use std::os::unix::net::UnixStream;
        let socket_path = get_ipc_socket_path();

        match UnixStream::connect(&socket_path) {
            Ok(mut stream) => {
                // Set timeouts to avoid blocking forever if primary instance hangs
                stream.set_read_timeout(Some(IPC_TIMEOUT))?;
                stream.set_write_timeout(Some(IPC_TIMEOUT))?;

                // Connected to existing instance - send URI
                writeln!(stream, "{}", uri)?;
                stream.flush()?;

                // Wait for acknowledgment
                let mut reader = BufReader::new(stream);
                let mut response = String::new();
                reader.read_line(&mut response)?;

                Ok(true)
            }
            Err(_) => {
                // No existing instance
                Ok(false)
            }
        }
    }

    #[cfg(windows)]
    {
        use interprocess::os::windows::named_pipe::{DuplexPipeStream, pipe_mode};
        use std::io::Write;

        let pipe_name = get_ipc_socket_path();

        match DuplexPipeStream::<pipe_mode::Bytes>::connect_by_path(pipe_name.as_str()) {
            Ok(mut stream) => {
                // Note: interprocess named pipes don't support timeouts directly,
                // but the operations are typically fast. If this becomes an issue,
                // we could spawn a thread with a timeout wrapper.
                writeln!(stream, "{}", uri)?;
                stream.flush()?;

                // Wait for acknowledgment
                let mut reader = BufReader::new(stream);
                let mut response = String::new();
                reader.read_line(&mut response)?;

                Ok(true)
            }
            Err(_) => Ok(false),
        }
    }
}

/// Parse command line arguments for a nexus:// URI
fn get_startup_uri() -> Option<String> {
    let args: Vec<String> = std::env::args().collect();

    // Look for a nexus:// URI in arguments
    // Handle both direct URI and %u placeholder from desktop files
    for arg in args.iter().skip(1) {
        let arg = arg.trim();
        if arg == "%u" {
            // Placeholder not substituted, skip
            continue;
        }
        if uri::is_nexus_uri(arg) {
            return Some(arg.to_string());
        }
    }

    None
}

/// Application entry point
///
/// Configures the Iced application with window settings, fonts, and theme,
/// then starts the event loop.
pub fn main() -> iced::Result {
    // Install rustls crypto provider before any TLS/DTLS operations
    // This is required because both tokio-rustls and dtls use rustls 0.23
    // which needs an explicit crypto provider selection
    tokio_rustls::rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    // Check for startup URI and single-instance handling
    let startup_uri = get_startup_uri();

    if let Some(ref uri_str) = startup_uri {
        // Try to send to existing instance
        match try_send_to_existing_instance(uri_str) {
            Ok(true) => {
                // Successfully sent to existing instance, exit
                return Ok(());
            }
            Ok(false) => {
                // No existing instance, continue startup
            }
            Err(e) => {
                // Error connecting, log and continue
                eprintln!("IPC error: {}", e);
            }
        }
    }

    // Store startup URI in a static for NexusApp::new to pick up
    if let Some(uri_str) = startup_uri {
        STARTUP_URI.lock().unwrap().replace(uri_str);
    }

    // Load config early to get saved window position/size
    let config = config::Config::load();
    let window_size = iced::Size::new(config.settings.window_width, config.settings.window_height);
    let window_position = match (config.settings.window_x, config.settings.window_y) {
        (Some(x), Some(y)) => {
            iced::window::Position::Specific(iced::Point::new(x as f32, y as f32))
        }
        _ => iced::window::Position::default(),
    };

    iced::application(NexusApp::new, NexusApp::update, NexusApp::view)
        .title(WINDOW_TITLE)
        .theme(NexusApp::theme)
        .subscription(NexusApp::subscription)
        .window(iced::window::Settings {
            size: window_size,
            min_size: Some(iced::Size::new(WINDOW_WIDTH_MIN, WINDOW_HEIGHT_MIN)),
            position: window_position,
            exit_on_close_request: false,
            #[cfg(target_os = "linux")]
            platform_specific: iced::window::settings::PlatformSpecific {
                application_id: "nexus".to_string(),
                ..Default::default()
            },
            ..Default::default()
        })
        .font(fonts::SAUCECODE_PRO_MONO)
        .font(fonts::SAUCECODE_PRO_MONO_BOLD)
        .font(fonts::SAUCECODE_PRO_MONO_ITALIC)
        .font(fonts::SAUCECODE_PRO_MONO_BOLD_ITALIC)
        .font(icon::FONT)
        .font(iced_aw::ICED_AW_FONT_BYTES)
        .run()
}

/// Main application state for the Nexus BBS client
struct NexusApp {
    // -------------------------------------------------------------------------
    // Persistence
    // -------------------------------------------------------------------------
    /// Application configuration and server bookmarks
    config: config::Config,

    // -------------------------------------------------------------------------
    // Connections
    // -------------------------------------------------------------------------
    /// Active server connections by connection_id
    connections: HashMap<usize, ServerConnection>,
    /// Currently displayed connection
    active_connection: Option<usize>,
    /// Counter for generating unique connection IDs
    next_connection_id: usize,
    /// Set of bookmark IDs currently connecting (prevents duplicate attempts)
    connecting_bookmarks: HashSet<Uuid>,

    // -------------------------------------------------------------------------
    // Forms
    // -------------------------------------------------------------------------
    /// Connection form inputs and state
    connection_form: ConnectionFormState,
    /// State for bookmark add/edit dialog
    bookmark_edit: BookmarkEditState,
    /// Currently focused input field
    focused_field: InputId,

    // -------------------------------------------------------------------------
    // UI State
    // -------------------------------------------------------------------------
    /// UI panel visibility toggles
    ui_state: UiState,
    /// Settings panel form state (present when settings panel is open)
    settings_form: Option<SettingsFormState>,
    /// Active settings tab (persisted across panel opens)
    settings_tab: SettingsTab,
    /// Selected event type in Events tab (persisted across panel opens)
    selected_event_type: EventType,

    // -------------------------------------------------------------------------
    // Async / Transient
    // -------------------------------------------------------------------------
    /// Certificate fingerprint mismatch queue (for handling multiple mismatches)
    fingerprint_mismatch_queue: VecDeque<FingerprintMismatch>,
    /// Transient per-bookmark connection errors (not persisted to disk)
    bookmark_errors: HashMap<Uuid, String>,

    // -------------------------------------------------------------------------
    // Text Editor State (not Clone, stored outside ServerConnection)
    // -------------------------------------------------------------------------
    /// News body editor content, keyed by connection_id (used for both create and edit)
    news_body_content: HashMap<usize, text_editor::Content>,

    // -------------------------------------------------------------------------
    // Chat History
    // -------------------------------------------------------------------------
    /// History managers for user message persistence, keyed by base_dir path
    /// (shared across connections to the same server+account)
    history_managers: HashMap<std::path::PathBuf, history::HistoryManager>,
    /// Maps connection_id to its history manager's base_dir key
    connection_history_keys: HashMap<usize, std::path::PathBuf>,

    // -------------------------------------------------------------------------
    // Transfers
    // -------------------------------------------------------------------------
    /// Transfer manager for file downloads/uploads (global, not per-connection)
    transfer_manager: transfers::TransferManager,

    // -------------------------------------------------------------------------
    // Voice
    // -------------------------------------------------------------------------
    /// Connection ID with active voice session (only one voice session per client)
    active_voice_connection: Option<usize>,
    /// Active voice session handle (for controlling the session)
    voice_session_handle: Option<voice::manager::VoiceSessionHandle>,
    /// PTT (push-to-talk) manager for global hotkey handling
    ptt_manager: Option<voice::ptt::PttManager>,
    /// Whether local user is currently transmitting (PTT active)
    is_local_speaking: bool,
    /// Whether local user has deafened (muted all incoming voice audio)
    is_deafened: bool,
    /// Generation counter for PTT release delay timers (used to cancel stale timers)
    ptt_release_delay_generation: u64,
    /// Shared mic level for VU meter display (f32 stored as bits, 0.0-1.0)
    mic_level: Arc<AtomicU32>,

    // -------------------------------------------------------------------------
    // Drag and Drop
    // -------------------------------------------------------------------------
    /// Whether files are currently being dragged over the window
    dragging_files: bool,

    // -------------------------------------------------------------------------
    // Window State
    // -------------------------------------------------------------------------
    /// Whether the application window is currently focused
    window_focused: bool,
    /// Whether the application window is currently visible (for minimize to tray)
    #[cfg(not(target_os = "macos"))]
    window_visible: bool,
    /// Whether the window was maximized before hiding to tray (to restore state)
    #[cfg(not(target_os = "macos"))]
    window_was_maximized: bool,

    // -------------------------------------------------------------------------
    // System Tray (Windows/Linux only)
    // -------------------------------------------------------------------------
    /// System tray manager (None on macOS or if tray creation failed)
    #[cfg(not(target_os = "macos"))]
    tray_manager: Option<tray::TrayManager>,

    // -------------------------------------------------------------------------
    // Toasts
    // -------------------------------------------------------------------------
    /// Toast notification container for transient feedback messages
    toasts: ToastContainer<'static, Message>,
}

impl Default for NexusApp {
    fn default() -> Self {
        let config = config::Config::load();
        let transfer_manager = transfers::TransferManager::load();
        let selected_event_type = config.settings.selected_event_type;
        Self {
            // Persistence
            config,
            // Connections
            connections: HashMap::new(),
            active_connection: None,
            active_voice_connection: None,
            voice_session_handle: None,
            // PTT manager is created lazily on first voice join (not at startup).
            // This ensures the native event loop is active when the hotkey system initializes.
            ptt_manager: None,
            is_local_speaking: false,
            is_deafened: false,
            ptt_release_delay_generation: 0,
            mic_level: Arc::new(AtomicU32::new(0)),
            next_connection_id: 0,
            connecting_bookmarks: HashSet::new(),
            // Forms
            connection_form: ConnectionFormState::default(),
            bookmark_edit: BookmarkEditState::default(),
            focused_field: InputId::ServerName,
            // UI State
            ui_state: UiState::default(),
            settings_form: None,
            settings_tab: SettingsTab::default(),
            selected_event_type,
            // Async / Transient
            fingerprint_mismatch_queue: VecDeque::new(),
            bookmark_errors: HashMap::new(),
            // Text Editor State
            news_body_content: HashMap::new(),
            // Chat History
            history_managers: HashMap::new(),
            connection_history_keys: HashMap::new(),
            // Transfers
            transfer_manager,
            // Drag and Drop
            dragging_files: false,
            // Window State
            window_focused: true,
            #[cfg(not(target_os = "macos"))]
            window_visible: true,
            #[cfg(not(target_os = "macos"))]
            window_was_maximized: false,
            // System Tray (Windows/Linux only)
            #[cfg(not(target_os = "macos"))]
            tray_manager: None,
            // Toasts
            toasts: toast_container(Message::ToastDismiss)
                .timeout(std::time::Duration::from_secs(style::TOAST_TIMEOUT_SECS))
                .style(toast_style),
        }
    }
}

impl NexusApp {
    /// Initialize the application with default state and auto-connect tasks
    ///
    /// Called once at startup to set up initial state and generate tasks for
    /// focusing the input field and auto-connecting to bookmarks.
    fn new() -> (Self, Task<Message>) {
        #[cfg(not(target_os = "macos"))]
        let mut app = Self::default();
        #[cfg(target_os = "macos")]
        let app = Self::default();

        // Initialize tray icon on startup if setting is enabled (Windows/Linux only)
        #[cfg(not(target_os = "macos"))]
        if app.config.settings.show_tray_icon
            && let Some(tray) = tray::TrayManager::new()
        {
            app.tray_manager = Some(tray);
            // State will be Disconnected on startup, which is correct
            // If tray creation fails on startup, silently continue without it
            // (user can toggle the setting to see the error)
        }

        // Install macOS URL scheme delegate to receive nexus:// links via Apple Events
        #[cfg(target_os = "macos")]
        macos_url::install();

        // Check for startup URI
        let startup_uri = STARTUP_URI.lock().unwrap().take();
        let mut tasks: Vec<Task<Message>> = vec![operation::focus(Id::from(InputId::ServerName))];

        if let Some(uri_str) = startup_uri {
            if let Ok(parsed_uri) = uri::parse(&uri_str) {
                // Queue URI handling as a task
                tasks.push(Task::done(Message::HandleNexusUri(parsed_uri)));
            }
        } else {
            // No startup URI - generate auto-connect tasks for bookmarks
            let auto_connect_tasks = autostart::generate_auto_connect_tasks(&app.config);
            tasks.extend(auto_connect_tasks);
        }

        (app, Task::batch(tasks))
    }

    /// Process a message and update application state
    ///
    /// Central message dispatcher that routes messages to their handlers.
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            // Keyboard and window events
            Message::Event(event) => self.handle_keyboard_event(event),
            Message::NextChatTab => self.handle_next_chat_tab(),
            Message::PrevChatTab => self.handle_prev_chat_tab(),
            Message::TabPressed => self.handle_tab_navigation(),
            Message::WindowCloseRequested(id) => {
                // Check if we should minimize to tray instead of closing (Windows/Linux only)
                #[cfg(not(target_os = "macos"))]
                if self.config.settings.minimize_to_tray
                    && self.config.settings.show_tray_icon
                    && self.tray_manager.is_some()
                {
                    // Query maximized state, then hide window
                    return iced::window::is_maximized(id).map(move |maximized| {
                        Message::TrayHideWindow {
                            id,
                            was_maximized: maximized,
                        }
                    });
                }

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
            }
            Message::WindowFocused => {
                self.window_focused = true;
                Task::none()
            }
            Message::WindowUnfocused => {
                self.window_focused = false;
                Task::none()
            }
            Message::WindowSaveAndClose {
                id,
                width,
                height,
                x,
                y,
            } => {
                // Save window dimensions and position
                self.config.settings.window_width = width;
                self.config.settings.window_height = height;
                self.config.settings.window_x = x;
                self.config.settings.window_y = y;
                let _ = self.config.save();

                // Save any pending transfer progress
                let _ = self.transfer_manager.save();

                // Clean up IPC socket
                #[cfg(unix)]
                {
                    let _ = std::fs::remove_file(get_ipc_socket_path());
                }

                iced::window::close(id)
            }

            // Connection management
            Message::ConnectPressed => self.handle_connect_pressed(),
            Message::ConnectToBookmark(id) => self.handle_connect_to_bookmark(id),
            Message::DisconnectFromServer(connection_id) => {
                self.handle_disconnect_from_server(connection_id)
            }
            Message::SwitchToConnection(connection_id) => {
                self.handle_switch_to_connection(connection_id)
            }

            // Connection form fields
            Message::AddBookmarkToggled(enabled) => self.handle_add_bookmark_toggled(enabled),
            Message::PasswordChanged(password) => self.handle_password_changed(password),
            Message::PortChanged(port) => self.handle_port_changed(port),
            Message::ServerAddressChanged(addr) => self.handle_server_address_changed(addr),
            Message::ServerNameChanged(name) => self.handle_server_name_changed(name),
            Message::UsernameChanged(username) => self.handle_username_changed(username),
            Message::NicknameChanged(nickname) => self.handle_nickname_changed(nickname),
            Message::ConnectionFormTabPressed => self.handle_connection_form_tab_pressed(),
            Message::ConnectionFormFocusResult(
                name,
                address,
                port,
                username,
                password,
                nickname,
            ) => self.handle_connection_form_focus_result(
                name, address, port, username, password, nickname,
            ),

            // Bookmark management
            Message::BookmarkAddressChanged(addr) => self.handle_bookmark_address_changed(addr),
            Message::BookmarkAutoConnectToggled(enabled) => {
                self.handle_bookmark_auto_connect_toggled(enabled)
            }
            Message::BookmarkNameChanged(name) => self.handle_bookmark_name_changed(name),
            Message::BookmarkPasswordChanged(password) => {
                self.handle_bookmark_password_changed(password)
            }
            Message::BookmarkPortChanged(port) => self.handle_bookmark_port_changed(port),
            Message::BookmarkUsernameChanged(username) => {
                self.handle_bookmark_username_changed(username)
            }
            Message::BookmarkNicknameChanged(nickname) => {
                self.handle_bookmark_nickname_changed(nickname)
            }
            Message::CancelBookmarkEdit => self.handle_cancel_bookmark_edit(),
            Message::DeleteBookmark(id) => self.handle_delete_bookmark(id),
            Message::SaveBookmark => self.handle_save_bookmark(),
            Message::ShowAddBookmark => self.handle_show_add_bookmark(),
            Message::ShowEditBookmark(id) => self.handle_show_edit_bookmark(id),
            Message::BookmarkEditTabPressed => self.handle_bookmark_edit_tab_pressed(),
            Message::BookmarkEditFocusResult(name, address, port, username, password, nickname) => {
                self.handle_bookmark_edit_focus_result(
                    name, address, port, username, password, nickname,
                )
            }

            // Certificate fingerprint
            Message::AcceptNewFingerprint => self.handle_accept_new_fingerprint(),
            Message::CancelFingerprintMismatch => self.handle_cancel_fingerprint_mismatch(),

            // Chat
            Message::ChatInputChanged(input) => self.handle_message_input_changed(input),
            Message::ChatTabComplete => self.handle_chat_tab_complete(),
            Message::ChatScrolled(viewport) => self.handle_chat_scrolled(viewport),
            Message::CloseChannelTab(channel) => self.handle_close_channel_tab(channel),
            Message::CloseUserMessageTab(nickname) => self.handle_close_user_message_tab(nickname),
            Message::SendMessagePressed => self.handle_send_message_pressed(),
            Message::SwitchChatTab(tab) => self.handle_switch_chat_tab(tab),

            // User list interactions
            Message::UserInfoIconClicked(nickname) => self.handle_user_info_icon_clicked(nickname),
            Message::DisconnectIconClicked(nickname) => {
                self.handle_disconnect_icon_clicked(nickname)
            }
            Message::DisconnectDialogActionChanged(action) => {
                self.handle_disconnect_dialog_action_changed(action)
            }
            Message::DisconnectDialogDurationChanged(duration) => {
                self.handle_disconnect_dialog_duration_changed(duration)
            }
            Message::DisconnectDialogReasonChanged(reason) => {
                self.handle_disconnect_dialog_reason_changed(reason)
            }
            Message::DisconnectDialogCancel => self.handle_disconnect_dialog_cancel(),
            Message::DisconnectDialogSubmit => self.handle_disconnect_dialog_submit(),
            Message::UserListItemClicked(nickname) => self.handle_user_list_item_clicked(nickname),
            Message::UserMessageIconClicked(nickname) => {
                self.handle_user_message_icon_clicked(nickname)
            }

            // User management
            Message::CancelUserManagement => self.handle_cancel_user_management(),
            Message::ToggleUserManagement => self.handle_toggle_user_management(),
            Message::UserManagementShowCreate => self.handle_user_management_show_create(),
            Message::UserManagementUsernameChanged(username) => {
                self.handle_user_management_username_changed(username)
            }
            Message::UserManagementPasswordChanged(password) => {
                self.handle_user_management_password_changed(password)
            }
            Message::UserManagementIsAdminToggled(is_admin) => {
                self.handle_user_management_is_admin_toggled(is_admin)
            }
            Message::UserManagementIsSharedToggled(is_shared) => {
                self.handle_user_management_is_shared_toggled(is_shared)
            }
            Message::UserManagementEnabledToggled(enabled) => {
                self.handle_user_management_enabled_toggled(enabled)
            }
            Message::UserManagementPermissionToggled(permission, enabled) => {
                self.handle_user_management_permission_toggled(permission, enabled)
            }
            Message::UserManagementCreatePressed => self.handle_user_management_create_pressed(),
            Message::UserManagementEditClicked(username) => {
                self.handle_user_management_edit_clicked(username)
            }
            Message::UserManagementDeleteClicked(username) => {
                self.handle_user_management_delete_clicked(username)
            }
            Message::UserManagementConfirmDelete => self.handle_user_management_confirm_delete(),
            Message::UserManagementCancelDelete => self.handle_user_management_cancel_delete(),
            Message::UserManagementEditUsernameChanged(username) => {
                self.handle_user_management_edit_username_changed(username)
            }
            Message::UserManagementEditPasswordChanged(password) => {
                self.handle_user_management_edit_password_changed(password)
            }
            Message::UserManagementEditIsAdminToggled(is_admin) => {
                self.handle_user_management_edit_is_admin_toggled(is_admin)
            }
            Message::UserManagementEditEnabledToggled(enabled) => {
                self.handle_user_management_edit_enabled_toggled(enabled)
            }
            Message::UserManagementEditPermissionToggled(permission, enabled) => {
                self.handle_user_management_edit_permission_toggled(permission, enabled)
            }
            Message::UserManagementUpdatePressed => self.handle_user_management_update_pressed(),
            Message::ValidateUserManagementCreate => self.handle_validate_user_management_create(),
            Message::ValidateUserManagementEdit => self.handle_validate_user_management_edit(),
            Message::UserManagementCreateTabPressed => {
                self.handle_user_management_create_tab_pressed()
            }
            Message::UserManagementCreateFocusResult(username, password) => {
                self.handle_user_management_create_focus_result(username, password)
            }
            Message::UserManagementEditTabPressed => self.handle_user_management_edit_tab_pressed(),
            Message::UserManagementEditFocusResult(username, password) => {
                self.handle_user_management_edit_focus_result(username, password)
            }

            // Broadcast
            Message::BroadcastMessageChanged(input) => self.handle_broadcast_message_changed(input),
            Message::CancelBroadcast => self.handle_cancel_broadcast(),
            Message::SendBroadcastPressed => self.handle_send_broadcast_pressed(),
            Message::ValidateBroadcast => self.handle_validate_broadcast(),

            // UI toggles
            Message::ShowChatView => self.handle_show_chat_view(),
            Message::ToggleBookmarks => self.handle_toggle_bookmarks(),
            Message::ToggleBroadcast => self.handle_toggle_broadcast(),
            Message::ToggleUserList => self.handle_toggle_user_list(),

            // Settings
            Message::CancelSettings => self.handle_cancel_settings(),
            Message::ChatFontSizeSelected(size) => self.handle_chat_font_size_selected(size),
            Message::MaxScrollbackChanged(value) => self.handle_max_scrollback_changed(value),
            Message::ChatHistoryRetentionSelected(retention) => {
                self.handle_chat_history_retention_selected(retention)
            }
            Message::ClearAvatarPressed => self.handle_clear_avatar_pressed(),
            Message::ConnectionNotificationsToggled(enabled) => {
                self.handle_connection_notifications_toggled(enabled)
            }
            Message::ChannelNotificationsToggled(enabled) => {
                self.handle_channel_notifications_toggled(enabled)
            }
            Message::AvatarLoaded(result) => self.handle_avatar_loaded(result),
            Message::PickAvatarPressed => self.handle_pick_avatar_pressed(),
            Message::SaveSettings => self.handle_save_settings(),
            Message::ShowSecondsToggled(enabled) => self.handle_show_seconds_toggled(enabled),
            Message::ShowTimestampsToggled(enabled) => self.handle_show_timestamps_toggled(enabled),
            Message::SettingsTabSelected(tab) => self.handle_settings_tab_selected(tab),
            Message::EventTypeSelected(event_type) => self.handle_event_type_selected(event_type),
            Message::ToggleNotificationsEnabled(enabled) => {
                self.config.settings.notifications_enabled = enabled;
                Task::none()
            }
            Message::EventShowNotificationToggled(enabled) => {
                self.handle_event_show_notification_toggled(enabled)
            }
            Message::EventNotificationContentSelected(content) => {
                self.handle_event_notification_content_selected(content)
            }
            Message::TestNotification => self.handle_test_notification(),
            Message::EventShowToastToggled(enabled) => {
                self.handle_event_show_toast_toggled(enabled)
            }
            Message::EventToastContentSelected(content) => {
                self.handle_event_toast_content_selected(content)
            }
            Message::TestToast => self.handle_test_toast(),
            Message::ToggleSoundEnabled(enabled) => {
                self.config.settings.sound_enabled = enabled;
                Task::none()
            }
            Message::SoundVolumeChanged(volume) => {
                self.config.settings.sound_volume = volume;
                Task::none()
            }
            Message::EventPlaySoundToggled(enabled) => {
                self.handle_event_play_sound_toggled(enabled)
            }
            Message::EventSoundSelected(sound) => self.handle_event_sound_selected(sound),
            Message::EventAlwaysPlaySoundToggled(enabled) => {
                self.handle_event_always_play_sound_toggled(enabled)
            }
            Message::TestSound => self.handle_test_sound(),
            Message::ThemeSelected(theme) => self.handle_theme_selected(theme),
            Message::SettingsNicknameChanged(nickname) => {
                self.handle_settings_nickname_changed(nickname)
            }
            Message::ToggleSettings => self.handle_toggle_settings(),
            Message::Use24HourTimeToggled(enabled) => self.handle_use_24_hour_time_toggled(enabled),
            Message::ProxyEnabledToggled(enabled) => self.handle_proxy_enabled_toggled(enabled),
            Message::ProxyAddressChanged(address) => self.handle_proxy_address_changed(address),
            Message::ProxyPortChanged(port) => self.handle_proxy_port_changed(port),
            Message::ProxyUsernameChanged(username) => self.handle_proxy_username_changed(username),
            Message::ProxyPasswordChanged(password) => self.handle_proxy_password_changed(password),
            Message::SettingsTabPressed => self.handle_settings_tab_pressed(),
            Message::SettingsNetworkFocusResult(address, port, username, password) => {
                self.handle_settings_network_focus_result(address, port, username, password)
            }
            Message::BrowseDownloadPathPressed => self.handle_browse_download_path_pressed(),
            Message::DownloadPathSelected(path) => self.handle_download_path_selected(path),
            Message::QueueTransfersToggled(enabled) => self.handle_queue_transfers_toggled(enabled),
            Message::DownloadLimitChanged(limit) => self.handle_download_limit_changed(limit),
            Message::UploadLimitChanged(limit) => self.handle_upload_limit_changed(limit),

            // About
            Message::CloseAbout => self.handle_close_about(),
            Message::OpenUrl(url) => self.handle_open_url(url),
            Message::ShowAbout => self.handle_show_about(),

            // Transfers
            Message::ToggleTransfers => self.handle_toggle_transfers(),
            Message::CloseTransfers => self.handle_close_transfers(),

            // Connection Monitor
            Message::ToggleConnectionMonitor => self.handle_toggle_connection_monitor(),
            Message::CloseConnectionMonitor => self.handle_close_connection_monitor(),
            Message::RefreshConnectionMonitor => self.handle_refresh_connection_monitor(),
            Message::ConnectionMonitorResponse {
                connection_id,
                success,
                error,
                connections,
                transfers,
            } => self.handle_connection_monitor_response(
                connection_id,
                success,
                error,
                connections,
                transfers,
            ),
            Message::ConnectionMonitorInfo(nickname) => {
                self.handle_connection_monitor_info(nickname)
            }
            Message::ConnectionMonitorKick(nickname) => {
                self.handle_connection_monitor_kick(nickname)
            }
            Message::ConnectionMonitorBan(nickname) => self.handle_connection_monitor_ban(nickname),
            Message::ConnectionMonitorCopy(value) => self.handle_connection_monitor_copy(value),
            Message::ConnectionMonitorSortBy(column) => {
                self.handle_connection_monitor_sort_by(column)
            }
            Message::ConnectionMonitorTabSelected(tab) => {
                self.handle_connection_monitor_tab_selected(tab)
            }
            Message::ConnectionMonitorTransferSortBy(column) => {
                self.handle_connection_monitor_transfer_sort_by(column)
            }

            // Server info
            Message::CancelEditServerInfo => self.handle_cancel_edit_server_info(),
            Message::ClearServerImagePressed => self.handle_clear_server_image_pressed(),
            Message::CloseServerInfo => self.handle_close_server_info(),
            Message::EditServerInfoDescriptionChanged(description) => {
                self.handle_edit_server_info_description_changed(description)
            }
            Message::EditServerInfoImageLoaded(result) => {
                self.handle_edit_server_info_image_loaded(result)
            }
            Message::EditServerInfoMaxConnectionsChanged(max_connections) => {
                self.handle_edit_server_info_max_connections_changed(max_connections)
            }
            Message::EditServerInfoMaxTransfersChanged(max_transfers) => {
                self.handle_edit_server_info_max_transfers_changed(max_transfers)
            }
            Message::EditServerInfoFileReindexIntervalChanged(interval) => {
                self.handle_edit_server_info_file_reindex_interval_changed(interval)
            }
            Message::EditServerInfoNameChanged(name) => {
                self.handle_edit_server_info_name_changed(name)
            }
            Message::EditServerInfoPersistentChannelsChanged(channels) => {
                self.handle_edit_server_info_persistent_channels_changed(channels)
            }
            Message::EditServerInfoAutoJoinChannelsChanged(channels) => {
                self.handle_edit_server_info_auto_join_channels_changed(channels)
            }
            Message::EditServerInfoPressed => self.handle_edit_server_info_pressed(),
            Message::ServerInfoTabChanged(tab) => self.handle_server_info_tab_changed(tab),
            Message::PickServerImagePressed => self.handle_pick_server_image_pressed(),
            Message::ShowServerInfo => self.handle_show_server_info(),
            Message::UpdateServerInfoPressed => self.handle_update_server_info_pressed(),
            Message::ServerInfoEditTabPressed => self.handle_server_info_edit_tab_pressed(),
            Message::ServerInfoEditFocusResult(name, description) => {
                self.handle_server_info_edit_focus_result(name, description)
            }

            // User info
            Message::CloseUserInfo => self.handle_close_user_info(),

            // Password change
            Message::ChangePasswordPressed => self.handle_change_password_pressed(),
            Message::ChangePasswordCurrentChanged(value) => {
                self.handle_change_password_current_changed(value)
            }
            Message::ChangePasswordNewChanged(value) => {
                self.handle_change_password_new_changed(value)
            }
            Message::ChangePasswordConfirmChanged(value) => {
                self.handle_change_password_confirm_changed(value)
            }
            Message::ChangePasswordCancelPressed => self.handle_change_password_cancel_pressed(),
            Message::ChangePasswordSavePressed => self.handle_change_password_save_pressed(),
            Message::ChangePasswordTabPressed => self.handle_change_password_tab_pressed(),
            Message::ChangePasswordFocusResult(current, new, confirm) => {
                self.handle_change_password_focus_result(current, new, confirm)
            }

            // Network events (async results)
            Message::BookmarkConnectionResult {
                result,
                bookmark_id,
                display_name,
            } => self.handle_bookmark_connection_result(result, bookmark_id, display_name),
            Message::ConnectionResult(result) => self.handle_connection_result(result),
            Message::NetworkError(connection_id, error) => {
                self.handle_network_error(connection_id, error)
            }
            Message::ServerMessageReceived(connection_id, message_id, msg, timestamp) => {
                self.handle_server_message_received(connection_id, message_id, msg, timestamp)
            }

            // News management
            Message::ToggleNews => self.handle_toggle_news(),
            Message::CancelNews => self.handle_cancel_news(),
            Message::NewsShowCreate => self.handle_news_show_create(),
            Message::NewsEditClicked(id) => self.handle_news_edit_clicked(id),
            Message::NewsDeleteClicked(id) => self.handle_news_delete_clicked(id),
            Message::NewsConfirmDelete => self.handle_news_confirm_delete(),
            Message::NewsCancelDelete => self.handle_news_cancel_delete(),
            Message::NewsBodyAction(action) => self.handle_news_body_action(action),
            Message::NewsPickImagePressed => self.handle_news_pick_image_pressed(),
            Message::NewsImageLoaded(result) => self.handle_news_image_loaded(result),
            Message::NewsClearImagePressed => self.handle_news_clear_image_pressed(),
            Message::NewsSubmitPressed => self.handle_news_submit_pressed(),

            // Files panel
            Message::ToggleFiles => self.handle_toggle_files(),
            Message::CancelFiles => self.handle_cancel_files(),
            Message::FileNavigate(path) => self.handle_file_navigate(path),
            Message::FileNavigateUp => self.handle_file_navigate_up(),
            Message::FileNavigateHome => self.handle_file_navigate_home(),
            Message::FileRefresh => self.handle_file_refresh(),
            Message::FileToggleRoot => self.handle_file_toggle_root(),
            Message::FileToggleHidden => self.handle_file_toggle_hidden(),
            Message::FileNewDirectoryClicked => self.handle_file_new_directory_clicked(),
            Message::FileNewDirectoryNameChanged(name) => {
                self.handle_file_new_directory_name_changed(name)
            }
            Message::FileNewDirectorySubmit => self.handle_file_new_directory_submit(),
            Message::FileNewDirectoryCancel => self.handle_file_new_directory_cancel(),
            Message::FileDeleteClicked(path) => self.handle_file_delete_clicked(path),
            Message::FileConfirmDelete => self.handle_file_confirm_delete(),
            Message::FileCancelDelete => self.handle_file_cancel_delete(),
            Message::FileInfoClicked(name) => self.handle_file_info_clicked(name),
            Message::CloseFileInfo => self.handle_close_file_info(),
            Message::FileRenameClicked(name) => self.handle_file_rename_clicked(name),
            Message::FileRenameNameChanged(name) => self.handle_file_rename_name_changed(name),
            Message::FileRenameSubmit => self.handle_file_rename_submit(),
            Message::FileRenameCancel => self.handle_file_rename_cancel(),
            Message::FileCut(path, name) => self.handle_file_cut(path, name),
            Message::FileCopyToClipboard(path, name) => {
                self.handle_file_copy_to_clipboard(path, name)
            }
            Message::FilePaste => self.handle_file_paste(),
            Message::FilePasteInto(dir) => self.handle_file_paste_into(dir),
            Message::FileClearClipboard => self.handle_file_clear_clipboard(),
            Message::FileSortBy(column) => self.handle_file_sort_by(column),
            Message::FileOverwriteConfirm => self.handle_file_overwrite_confirm(),
            Message::FileOverwriteCancel => self.handle_file_overwrite_cancel(),
            Message::FileTabNew => self.handle_file_tab_new(),
            Message::FileTabSwitch(tab_id) => self.handle_file_tab_switch(tab_id),
            Message::FileTabClose(tab_id) => self.handle_file_tab_close(tab_id),
            Message::FileShare(path) => self.handle_file_share(path),
            Message::FileDownload(path) => self.handle_file_download(path),
            Message::FileDownloadAll(path) => self.handle_file_download_all(path),
            Message::FileUpload(destination) => self.handle_file_upload(destination),
            Message::FileUploadCancelled => Task::none(),
            Message::FileUploadSelected(destination, paths) => {
                self.handle_file_upload_selected(destination, paths)
            }
            Message::FileDragHovered => self.handle_file_drag_hovered(),
            Message::FileDragDropped(path) => self.handle_file_drag_dropped(path),
            Message::FileDragLeft => self.handle_file_drag_left(),

            // File search
            Message::FileSearchInputChanged(value) => self.handle_file_search_input_changed(value),
            Message::FileSearchSubmit => self.handle_file_search_submit(),
            Message::FileSearchResultClicked(result) => {
                self.handle_file_search_result_clicked(result)
            }
            Message::FileSearchResultDownload(result) => {
                self.handle_file_search_result_download(result)
            }
            Message::FileSearchResultInfo(result) => self.handle_file_search_result_info(result),
            Message::FileSearchResultOpen(result) => self.handle_file_search_result_open(result),
            Message::FileSearchSortBy(column) => self.handle_file_search_sort_by(column),

            // Transfer management
            Message::TransferProgress(event) => self.handle_transfer_progress(event),
            Message::TransferPause(id) => self.handle_transfer_pause(id),
            Message::TransferResume(id) => self.handle_transfer_resume(id),
            Message::TransferCancel(id) => self.handle_transfer_cancel(id),
            Message::TransferRemove(id) => self.handle_transfer_remove(id),
            Message::TransferOpenFolder(id) => self.handle_transfer_open_folder(id),
            Message::TransferClearInactive => self.handle_transfer_clear_inactive(),
            Message::TransferMoveUp(id) => self.handle_transfer_move_up(id),
            Message::TransferMoveDown(id) => self.handle_transfer_move_down(id),
            Message::TransferRetry(id) => self.handle_transfer_retry(id),

            // Voice
            Message::VoiceJoinPressed(target) => self.handle_voice_join_pressed(target),
            Message::VoiceLeavePressed => self.handle_voice_leave_pressed(),
            Message::VoiceSessionEvent(connection_id, event) => {
                self.handle_voice_session_event(connection_id, event)
            }
            Message::VoicePttStateChanged(state) => self.handle_voice_ptt_state_changed(state),
            Message::VoicePttEvent(event) => self.handle_voice_ptt_event(event),
            Message::VoicePttReleaseDelayExpired(generation) => {
                self.handle_voice_ptt_release_delay_expired(generation)
            }
            Message::VoiceUserMute(nickname) => self.handle_voice_user_mute(nickname),
            Message::VoiceUserUnmute(nickname) => self.handle_voice_user_unmute(nickname),
            Message::VoiceDeafenToggle => self.handle_voice_deafen_toggle(),
            Message::VoiceMeterTick => Task::none(), // Just triggers re-render

            // Audio settings
            Message::AudioRefreshDevices => self.handle_audio_refresh_devices(),
            Message::AudioOutputDeviceSelected(device) => {
                self.handle_audio_output_device_selected(device)
            }
            Message::AudioInputDeviceSelected(device) => {
                self.handle_audio_input_device_selected(device)
            }
            Message::AudioQualitySelected(quality) => self.handle_audio_quality_selected(quality),
            Message::AudioPttKeyCapture => self.handle_audio_ptt_key_capture(),
            Message::AudioPttKeyCaptured(key) => self.handle_audio_ptt_key_captured(key),
            Message::AudioPttModeSelected(mode) => self.handle_audio_ptt_mode_selected(mode),
            Message::AudioPttReleaseDelaySelected(delay) => {
                self.handle_audio_ptt_release_delay_selected(delay)
            }
            Message::AudioTestMicStart => self.handle_audio_test_mic_start(),
            Message::AudioTestMicStop => self.handle_audio_test_mic_stop(),
            Message::AudioMicLevel(level) => self.handle_audio_mic_level(level),
            Message::AudioMicError(error) => self.handle_audio_mic_error(error),
            Message::AudioNoiseSuppression(enabled) => {
                self.config.settings.audio.noise_suppression = enabled;
                let _ = self.config.save();
                self.update_voice_processor_settings();
                Task::none()
            }
            Message::AudioEchoCancellation(enabled) => {
                self.config.settings.audio.echo_cancellation = enabled;
                let _ = self.config.save();
                self.update_voice_processor_settings();
                Task::none()
            }
            Message::AudioAgc(enabled) => {
                self.config.settings.audio.agc = enabled;
                let _ = self.config.save();
                self.update_voice_processor_settings();
                Task::none()
            }
            Message::AudioTransientSuppression(enabled) => {
                self.config.settings.audio.transient_suppression = enabled;
                let _ = self.config.save();
                self.update_voice_processor_settings();
                Task::none()
            }

            // Toasts
            Message::ToastDismiss(id) => {
                self.toasts.dismiss(id);
                Task::none()
            }
            Message::ShowToast(text) => {
                self.toasts.push(toast(&text).level(ToastLevel::Success));
                Task::none()
            }

            // System Tray (Windows/Linux only)
            #[cfg(not(target_os = "macos"))]
            Message::TrayPoll => {
                // On Linux, poll from the TrayManager's channel
                // On Windows, poll from the static tray-icon receivers
                #[cfg(target_os = "linux")]
                if let Some(ref mut tray) = self.tray_manager
                    && let Some(msg) = tray.try_recv()
                {
                    return self.update(msg);
                }
                #[cfg(target_os = "windows")]
                if let Some(msg) = tray::poll_tray_events() {
                    return self.update(msg);
                }
                Task::none()
            }
            #[cfg(not(target_os = "macos"))]
            Message::TrayIconClicked => self.handle_tray_icon_clicked(),
            #[cfg(not(target_os = "macos"))]
            Message::TrayMenuShowHide => self.handle_tray_show_hide(),
            #[cfg(not(target_os = "macos"))]
            Message::TrayMenuMute => self.handle_tray_mute(),
            #[cfg(not(target_os = "macos"))]
            Message::TrayMenuQuit => self.handle_tray_quit(),
            #[cfg(not(target_os = "macos"))]
            Message::TrayHideWindow { id, was_maximized } => {
                self.handle_tray_hide_window(id, was_maximized)
            }
            #[cfg(not(target_os = "macos"))]
            Message::TrayShowWindow(id) => self.handle_tray_show_window(id),
            #[cfg(not(target_os = "macos"))]
            Message::TrayRestoreMinimized { id, maximized } => {
                self.handle_tray_restore_minimized(id, maximized)
            }
            #[cfg(not(target_os = "macos"))]
            Message::ShowTrayIconToggled(enabled) => {
                self.config.settings.show_tray_icon = enabled;
                self.update_tray_from_settings()
            }
            #[cfg(not(target_os = "macos"))]
            Message::MinimizeToTrayToggled(enabled) => {
                self.config.settings.minimize_to_tray = enabled;
                Task::none()
            }
            #[cfg(not(target_os = "macos"))]
            Message::TrayServiceClosed => {
                // ksni service died (D-Bus connection dropped, e.g., after system sleep)
                // Drop the dead manager and let update_tray_from_settings() recreate it
                self.tray_manager = None;
                self.update_tray_from_settings()
            }

            // URI scheme
            Message::HandleNexusUri(uri) => self.handle_nexus_uri(uri),
            Message::UriReceivedFromIpc(uri_str) => {
                if let Ok(parsed) = uri::parse(&uri_str) {
                    // Focus the window and handle the URI
                    let uri_task = self.handle_nexus_uri(parsed);
                    let focus_task = iced::window::oldest().then(|opt_id| {
                        opt_id
                            .map(iced::window::gain_focus)
                            .unwrap_or_else(Task::none)
                    });
                    Task::batch([focus_task, uri_task])
                } else {
                    Task::none()
                }
            }
            Message::UriConnectionResult {
                result,
                target_host,
                display_name,
                path,
            } => self.handle_uri_connection_result(result, target_host, display_name, path),
        }
    }

    /// Set up subscriptions for keyboard events, window events, and network streams
    ///
    /// Creates subscriptions for keyboard events, window resize/move/close events,
    /// and network message streams for each active connection.
    fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions = vec![
            // Keyboard and general events
            iced::event::listen().map(Message::Event),
            // Window close requests (we handle saving before exit)
            iced::window::close_requests().map(Message::WindowCloseRequested),
            // IPC listener for receiving URIs from other instances
            Subscription::run(ipc_listener_stream),
        ];

        // Listen for macOS URL scheme events (Apple Events from clicking nexus:// links)
        #[cfg(target_os = "macos")]
        subscriptions.push(Subscription::run(macos_url::url_stream));

        // Subscribe to all active connections
        for conn in self.connections.values() {
            subscriptions.push(Subscription::run_with(
                conn.connection_id,
                network::network_stream,
            ));
        }

        // Subscribe to transfer execution - one subscription per queued/active transfer
        // Each subscription is keyed by the transfer's stable UUID, so it remains
        // stable even as the transfer status changes from Queued -> Connecting -> Transferring
        //
        // If queue_transfers is enabled, we limit concurrent transfers PER SERVER.
        // Limits are separate for downloads and uploads (0 = unlimited).
        let active_transfers: Vec<_> = self.transfer_manager.active().collect();
        let mut queued_transfers: Vec<_> = self.transfer_manager.queued().collect();

        // Sort queued transfers by queue_position for priority ordering
        queued_transfers.sort_by_key(|t| t.queue_position);

        // Active transfers always get subscriptions
        for transfer in &active_transfers {
            subscriptions.push(transfers::transfer_subscription(
                transfer,
                &self.config.settings.proxy,
            ));
        }

        // Queued transfers: respect per-server limits if queue_transfers is enabled
        if self.config.settings.queue_transfers {
            use std::collections::HashMap;

            // Server key: address:port
            fn server_key(t: &transfers::Transfer) -> String {
                format!("{}:{}", t.connection_info.address, t.connection_info.port)
            }

            // Count active transfers per server per direction
            let mut active_downloads_per_server: HashMap<String, usize> = HashMap::new();
            let mut active_uploads_per_server: HashMap<String, usize> = HashMap::new();

            for t in &active_transfers {
                let key = server_key(t);
                match t.direction {
                    transfers::TransferDirection::Download => {
                        *active_downloads_per_server.entry(key).or_insert(0) += 1;
                    }
                    transfers::TransferDirection::Upload => {
                        *active_uploads_per_server.entry(key).or_insert(0) += 1;
                    }
                }
            }

            let download_limit = self.config.settings.download_limit as usize;
            let upload_limit = self.config.settings.upload_limit as usize;

            // Process queued transfers, respecting per-server limits
            for transfer in &queued_transfers {
                let key = server_key(transfer);

                let (active_count, limit, active_map) = match transfer.direction {
                    transfers::TransferDirection::Download => (
                        *active_downloads_per_server.get(&key).unwrap_or(&0),
                        download_limit,
                        &mut active_downloads_per_server,
                    ),
                    transfers::TransferDirection::Upload => (
                        *active_uploads_per_server.get(&key).unwrap_or(&0),
                        upload_limit,
                        &mut active_uploads_per_server,
                    ),
                };

                // 0 = unlimited, otherwise check limit
                if limit == 0 || active_count < limit {
                    subscriptions.push(transfers::transfer_subscription(
                        transfer,
                        &self.config.settings.proxy,
                    ));
                    // Track this transfer as "will be active" for subsequent checks
                    *active_map.entry(key).or_insert(0) += 1;
                }
            }
        } else {
            // No queuing - start all queued transfers immediately
            for transfer in &queued_transfers {
                subscriptions.push(transfers::transfer_subscription(
                    transfer,
                    &self.config.settings.proxy,
                ));
            }
        }

        // Subscribe to mic test when active in settings
        if let Some(form) = &self.settings_form
            && form.mic_testing
        {
            subscriptions.push(voice::mic_test::mic_test_subscription(
                self.config.settings.audio.input_device.clone(),
            ));
        }

        // Subscribe to voice events when in an active voice session
        if let Some(connection_id) = self.active_voice_connection {
            subscriptions.push(voice::subscription::voice_event_subscription(connection_id));

            // Subscribe to PTT hotkey events when in voice
            subscriptions.push(voice::ptt::ptt_subscription());

            // Subscribe to VU meter ticks when transmitting (for UI updates)
            if self.is_local_speaking {
                subscriptions.push(
                    iced::time::every(std::time::Duration::from_millis(16))
                        .map(|_| Message::VoiceMeterTick),
                );
            }
        }

        // Subscribe to tray events when tray is active (Windows/Linux only)
        #[cfg(not(target_os = "macos"))]
        if self.tray_manager.is_some() {
            subscriptions.push(tray::tray_subscription());
        }

        Subscription::batch(subscriptions)
    }

    /// Render the current application state to the UI
    ///
    /// Delegates to `views::main_layout()` for all rendering logic.
    fn view(&self) -> Element<'_, Message> {
        // Get current connection state
        let active_conn = self
            .active_connection
            .and_then(|id| self.connections.get(&id));
        let message_input = active_conn.map(|c| c.message_input.as_str()).unwrap_or("");
        let user_management = active_conn.map(|c| &c.user_management);

        // Get news body content for current connection
        let news_body_content = self
            .active_connection
            .and_then(|id| self.news_body_content.get(&id));

        // Build view configuration
        // Get audio state from settings form (for PTT capture, mic test) or defaults
        let (ptt_capturing, mic_testing, mic_error) = self
            .settings_form
            .as_ref()
            .map(|f| (f.ptt_capturing, f.mic_testing, f.mic_error.as_deref()))
            .unwrap_or((false, false, None));

        // Get mic level: prefer voice session atomic (when in voice), fallback to settings form (mic test)
        let mic_level = if self.is_local_speaking {
            // In voice and transmitting - read from shared atomic
            f32::from_bits(self.mic_level.load(std::sync::atomic::Ordering::Relaxed))
        } else if let Some(ref form) = self.settings_form {
            // Mic testing in settings
            form.mic_level
        } else {
            0.0
        };

        // Get audio device lists from settings form cache (if open) or use empty slices
        // Device lists are only needed when settings panel is open, and are cached there
        // to avoid ALSA device enumeration spam on every frame
        static EMPTY_DEVICES: &[voice::audio::AudioDevice] = &[];
        let (output_devices, input_devices): (
            &[voice::audio::AudioDevice],
            &[voice::audio::AudioDevice],
        ) = self
            .settings_form
            .as_ref()
            .map(|f| (f.output_devices.as_slice(), f.input_devices.as_slice()))
            .unwrap_or((EMPTY_DEVICES, EMPTY_DEVICES));

        let selected_output_device = output_devices
            .iter()
            .find(|d| d.name == self.config.settings.audio.output_device)
            .cloned()
            .unwrap_or_else(voice::audio::AudioDevice::system_default);

        let selected_input_device = input_devices
            .iter()
            .find(|d| d.name == self.config.settings.audio.input_device)
            .cloned()
            .unwrap_or_else(voice::audio::AudioDevice::system_default);

        let config = ViewConfig {
            theme: self.theme(),
            show_connection_events: self.config.settings.show_connection_events,
            show_join_leave_events: self.config.settings.show_join_leave_events,
            chat_history_retention: self.config.settings.chat_history_retention,
            chat_font_size: self.config.settings.chat_font_size,
            show_timestamps: self.config.settings.show_timestamps,
            use_24_hour_time: self.config.settings.use_24_hour_time,
            show_seconds: self.config.settings.show_seconds,
            settings_form: self.settings_form.as_ref(),
            connections: &self.connections,
            active_connection: self.active_connection,
            bookmarks: &self.config.bookmarks,
            bookmark_errors: &self.bookmark_errors,
            connection_form: &self.connection_form,
            bookmark_edit: &self.bookmark_edit,
            message_input,
            nickname: self.config.settings.nickname.as_deref().unwrap_or(""),
            user_management,
            ui_state: &self.ui_state,
            active_panel: self.active_panel(),
            news_body_content,
            proxy: &self.config.settings.proxy,
            download_path: self.config.settings.download_path.as_deref(),
            show_hidden: self.config.settings.show_hidden_files,
            transfer_manager: &self.transfer_manager,
            queue_transfers: self.config.settings.queue_transfers,
            download_limit: self.config.settings.download_limit,
            upload_limit: self.config.settings.upload_limit,
            max_scrollback: self.config.settings.max_scrollback,
            show_drop_overlay: self.dragging_files && self.can_accept_file_drop(),
            event_settings: &self.config.settings.event_settings,
            notifications_enabled: self.config.settings.notifications_enabled,
            sound_enabled: self.config.settings.sound_enabled,
            sound_volume: self.config.settings.sound_volume,
            voice_target: self.get_voice_target_for_current_tab(),
            // Audio settings
            output_devices,
            selected_output_device,
            input_devices,
            selected_input_device,
            voice_quality: self.config.settings.audio.voice_quality,
            ptt_key: &self.config.settings.audio.ptt_key,
            ptt_capturing,
            ptt_mode: self.config.settings.audio.ptt_mode,
            ptt_release_delay: self.config.settings.audio.ptt_release_delay,
            mic_testing,
            mic_level,
            mic_error,
            noise_suppression: self.config.settings.audio.noise_suppression,
            echo_cancellation: self.config.settings.audio.echo_cancellation,
            agc: self.config.settings.audio.agc,
            transient_suppression: self.config.settings.audio.transient_suppression,
            is_local_speaking: self.is_local_speaking,
            is_deafened: self.is_deafened,
            // System Tray settings
            show_tray_icon: self.config.settings.show_tray_icon,
            minimize_to_tray: self.config.settings.minimize_to_tray,
        };

        let main_view = views::main_layout(config);

        // Overlay fingerprint mismatch dialog if present (show first in queue)
        if let Some(mismatch) = self.fingerprint_mismatch_queue.front() {
            return views::fingerprint_mismatch_dialog(mismatch);
        }

        // Wrap with toast container for transient notifications
        self.toasts.view(main_view)
    }

    fn theme(&self) -> Theme {
        self.config.settings.theme.to_iced_theme()
    }
}

/// Stream that listens for IPC connections from other instances
fn ipc_listener_stream() -> impl iced::futures::Stream<Item = Message> {
    iced::futures::stream::unfold(None, |listener_state| async move {
        #[cfg(unix)]
        {
            use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
            use tokio::net::UnixListener;

            // Initialize listener on first call
            let listener = match listener_state {
                Some(l) => l,
                None => {
                    let socket_path = get_ipc_socket_path();

                    // Remove stale socket file if it exists
                    let _ = std::fs::remove_file(&socket_path);

                    match UnixListener::bind(&socket_path) {
                        Ok(l) => {
                            // Set socket permissions to user-only (0600) for security
                            // This prevents other users from sending URIs to our client
                            use std::os::unix::fs::PermissionsExt;
                            let _ = std::fs::set_permissions(
                                &socket_path,
                                std::fs::Permissions::from_mode(0o600),
                            );
                            l
                        }
                        Err(_) => {
                            // Failed to bind - another instance might be running
                            // or we don't have permissions. Just return empty stream.
                            return None;
                        }
                    }
                }
            };

            // Wait for a connection
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        let mut reader = BufReader::new(stream);
                        let mut line = String::new();

                        if reader.read_line(&mut line).await.is_ok() && !line.is_empty() {
                            // Send acknowledgment
                            let mut stream = reader.into_inner();
                            let _ = stream.write_all(b"ok\n").await;

                            let uri = line.trim().to_string();
                            if uri::is_nexus_uri(&uri) {
                                return Some((Message::UriReceivedFromIpc(uri), Some(listener)));
                            }
                        }
                        // Invalid message, continue listening
                    }
                    Err(_) => {
                        // Accept error, continue listening
                    }
                }
            }
        }

        #[cfg(windows)]
        {
            use interprocess::os::windows::named_pipe::{
                PipeListenerOptions, PipeMode, pipe_mode,
                tokio::{PipeListener, PipeListenerOptionsExt, PipeStream as TokioPipeStream},
            };
            use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

            let pipe_name = get_ipc_socket_path();

            type WinPipeListener = PipeListener<pipe_mode::Bytes, pipe_mode::Bytes>;
            type WinPipeStream = TokioPipeStream<pipe_mode::Bytes, pipe_mode::Bytes>;

            // Initialize listener on first call
            let listener: WinPipeListener = match listener_state {
                Some(l) => l,
                None => {
                    match PipeListenerOptions::new()
                        .path(pipe_name.as_str())
                        .mode(PipeMode::Bytes)
                        .create_tokio::<pipe_mode::Bytes, pipe_mode::Bytes>()
                    {
                        Ok(l) => l,
                        Err(_) => return None,
                    }
                }
            };

            loop {
                match listener.accept().await {
                    Ok(stream) => {
                        let mut reader = BufReader::new(stream);
                        let mut line = String::new();

                        if reader.read_line(&mut line).await.is_ok() && !line.is_empty() {
                            // Send acknowledgment
                            let mut inner: WinPipeStream = reader.into_inner();
                            let _ = inner.write_all(b"ok\n").await;

                            let uri = line.trim().to_string();
                            if uri::is_nexus_uri(&uri) {
                                return Some((Message::UriReceivedFromIpc(uri), Some(listener)));
                            }
                        }
                    }
                    Err(_) => {
                        // Accept error, continue listening
                    }
                }
            }
        }

        #[cfg(not(any(unix, windows)))]
        {
            // Unsupported platform - no IPC
            let _state = listener_state;
            iced::futures::future::pending::<()>().await;
            None
        }
    })
}
