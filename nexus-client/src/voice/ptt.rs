//! Push-to-talk (PTT) handling
//!
//! Provides global hotkey support for voice chat push-to-talk functionality
//! using the global-hotkey crate.

use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crossbeam_channel::TryRecvError;
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager};
use iced::Subscription;
use iced::futures::Stream;
use iced::stream;

use crate::config::audio::PttMode;
use crate::types::Message;

// =============================================================================
// Constants
// =============================================================================

/// Channel size for PTT event stream
const PTT_STREAM_CHANNEL_SIZE: usize = 10;

/// Poll interval for checking hotkey events (milliseconds)
const PTT_POLL_INTERVAL_MS: u64 = 10;

/// Hotkey string separator (e.g., "Ctrl+Space")
pub const HOTKEY_SEPARATOR: &str = "+";

/// Modifier name: Control
pub const MOD_CTRL: &str = "Ctrl";
const MOD_CONTROL: &str = "Control";

/// Modifier name: Alt
pub const MOD_ALT: &str = "Alt";

/// Modifier name: Shift
pub const MOD_SHIFT: &str = "Shift";

/// Modifier name: Super/Command/Windows key
pub const MOD_SUPER: &str = "Super";
pub const MOD_CMD: &str = "Cmd";
const MOD_COMMAND: &str = "Command";
const MOD_WIN: &str = "Win";
const MOD_META: &str = "Meta";

// =============================================================================
// PTT State
// =============================================================================

/// Current state of push-to-talk
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PttState {
    /// Not transmitting
    #[default]
    Idle,
    /// Currently transmitting (key held or toggled on)
    Transmitting,
}

// =============================================================================
// PTT Manager
// =============================================================================

/// Manages push-to-talk hotkey registration and state
pub struct PttManager {
    /// The global hotkey manager
    manager: GlobalHotKeyManager,
    /// Currently registered hotkey
    hotkey: Option<HotKey>,
    /// Current PTT mode (hold or toggle)
    mode: PttMode,
    /// Whether PTT is currently active (transmitting)
    active: Arc<AtomicBool>,
    /// Whether we're in a voice session (hotkey should be active)
    in_voice: bool,
}

impl PttManager {
    /// Create a new PTT manager
    ///
    /// # Returns
    /// * `Ok(PttManager)` - Manager ready for use
    /// * `Err(String)` - Error if hotkey system couldn't be initialized
    pub fn new() -> Result<Self, String> {
        let manager = GlobalHotKeyManager::new()
            .map_err(|e| format!("Failed to initialize hotkey manager: {}", e))?;

        Ok(Self {
            manager,
            hotkey: None,
            mode: PttMode::default(),
            active: Arc::new(AtomicBool::new(false)),
            in_voice: false,
        })
    }

    /// Register the PTT hotkey
    ///
    /// # Arguments
    /// * `key` - Key code string (e.g., "`", "F1", "Space")
    ///
    /// # Returns
    /// * `Ok(())` - Hotkey registered successfully
    /// * `Err(String)` - Error if registration failed
    pub fn register_hotkey(&mut self, key: &str) -> Result<(), String> {
        // Unregister existing hotkey first
        self.unregister_hotkey();

        let (modifiers, code) = parse_hotkey(key)?;
        let hotkey = HotKey::new(Some(modifiers), code);

        self.manager
            .register(hotkey)
            .map_err(|e| format!("Failed to register hotkey: {}", e))?;

        self.hotkey = Some(hotkey);
        Ok(())
    }

    /// Unregister the current PTT hotkey
    pub fn unregister_hotkey(&mut self) {
        if let Some(hotkey) = self.hotkey.take() {
            let _ = self.manager.unregister(hotkey);
        }
    }

    /// Set the PTT mode
    pub fn set_mode(&mut self, mode: PttMode) {
        self.mode = mode;
        // Reset active state when changing mode
        self.active.store(false, Ordering::SeqCst);
    }

    /// Set whether we're in a voice session
    ///
    /// When not in voice, the hotkey is effectively disabled.
    pub fn set_in_voice(&mut self, in_voice: bool) {
        self.in_voice = in_voice;
        if !in_voice {
            self.active.store(false, Ordering::SeqCst);
        }
    }

    /// Handle a hotkey event
    ///
    /// # Arguments
    /// * `event` - The global hotkey event
    ///
    /// # Returns
    /// * `Some(PttState)` - State changed, caller should act on it
    /// * `None` - Not our hotkey or no state change
    pub fn handle_event(&mut self, event: GlobalHotKeyEvent) -> Option<PttState> {
        // Check if it's our hotkey
        let hotkey = self.hotkey?;
        if event.id() != hotkey.id() {
            return None;
        }

        // Ignore if not in voice
        if !self.in_voice {
            return None;
        }

        match event.state() {
            global_hotkey::HotKeyState::Pressed => self.handle_press(),
            global_hotkey::HotKeyState::Released => self.handle_release(),
        }
    }

    /// Handle key press
    fn handle_press(&mut self) -> Option<PttState> {
        match self.mode {
            PttMode::Hold => {
                // Start transmitting on press
                if !self.active.load(Ordering::SeqCst) {
                    self.active.store(true, Ordering::SeqCst);
                    return Some(PttState::Transmitting);
                }
            }
            PttMode::Toggle => {
                // Toggle state on press
                let was_active = self.active.fetch_xor(true, Ordering::SeqCst);
                return Some(if was_active {
                    PttState::Idle
                } else {
                    PttState::Transmitting
                });
            }
        }
        None
    }

    /// Handle key release
    fn handle_release(&mut self) -> Option<PttState> {
        match self.mode {
            PttMode::Hold => {
                // Stop transmitting on release
                if self.active.load(Ordering::SeqCst) {
                    self.active.store(false, Ordering::SeqCst);
                    return Some(PttState::Idle);
                }
            }
            PttMode::Toggle => {
                // Toggle mode ignores release
            }
        }
        None
    }
}

impl Drop for PttManager {
    fn drop(&mut self) {
        self.unregister_hotkey();
    }
}

// =============================================================================
// PTT Subscription
// =============================================================================

/// Subscription for receiving global hotkey events
///
/// This subscription listens for global hotkey events and emits
/// `Message::VoicePttStateChanged` when the PTT key is pressed or released.
///
/// Note: The PttManager must be created and have a hotkey registered
/// before events will be received. The subscription itself doesn't
/// manage the PttManager - it just forwards events to be handled.
pub fn ptt_subscription() -> Subscription<Message> {
    Subscription::run(ptt_event_stream)
}

/// Stream that receives global hotkey events
fn ptt_event_stream() -> Pin<Box<dyn Stream<Item = Message> + Send>> {
    Box::pin(stream::channel(
        PTT_STREAM_CHANNEL_SIZE,
        |mut output: iced::futures::channel::mpsc::Sender<Message>| async move {
            use iced::futures::SinkExt;

            // Get the global hotkey event receiver
            let receiver = GlobalHotKeyEvent::receiver();

            loop {
                // Use try_recv with a small sleep to avoid busy-waiting
                // and to keep the stream cancellable
                match receiver.try_recv() {
                    Ok(event) => {
                        // We don't have access to the PttManager here, so we send
                        // a raw event. The handler will check if it's our hotkey.
                        let _ = output.send(Message::VoicePttEvent(event)).await;
                    }
                    Err(TryRecvError::Empty) => {
                        // No event, sleep briefly
                        tokio::time::sleep(Duration::from_millis(PTT_POLL_INTERVAL_MS)).await;
                    }
                    Err(TryRecvError::Disconnected) => {
                        // Channel closed, exit
                        break;
                    }
                }
            }
        },
    ))
}

// =============================================================================
// Key Code Parsing
// =============================================================================

/// Parse a hotkey string into Modifiers and Code
///
/// Supports formats like:
/// - `"F1"` - Just a key, no modifiers
/// - `"Ctrl+Space"` - Single modifier + key
/// - `"Ctrl+Shift+A"` - Multiple modifiers + key
///
/// Modifier names (case-insensitive):
/// - `Ctrl` / `Control`
/// - `Alt`
/// - `Shift`
/// - `Super` / `Cmd` / `Command` / `Win` / `Meta`
///
/// # Arguments
/// * `key` - Hotkey string (e.g., "Ctrl+Space", "F1", "Alt+Shift+A")
///
/// # Returns
/// * `Ok((Modifiers, Code))` - Parsed modifiers and key code
/// * `Err(String)` - Error if key string is invalid
pub fn parse_hotkey(key: &str) -> Result<(Modifiers, Code), String> {
    let parts: Vec<&str> = key.split('+').collect();

    if parts.is_empty() {
        return Err("Empty hotkey string".to_string());
    }

    // Last part is the key, everything before is modifiers
    let key_part = parts.last().unwrap();
    let modifier_parts = &parts[..parts.len() - 1];

    // Parse modifiers (case-insensitive)
    let mut modifiers = Modifiers::empty();
    for modifier in modifier_parts {
        let mod_lower = modifier.to_lowercase();
        if mod_lower == MOD_CTRL.to_lowercase() || mod_lower == MOD_CONTROL.to_lowercase() {
            modifiers |= Modifiers::CONTROL;
        } else if mod_lower == MOD_ALT.to_lowercase() {
            modifiers |= Modifiers::ALT;
        } else if mod_lower == MOD_SHIFT.to_lowercase() {
            modifiers |= Modifiers::SHIFT;
        } else if mod_lower == MOD_SUPER.to_lowercase()
            || mod_lower == MOD_CMD.to_lowercase()
            || mod_lower == MOD_COMMAND.to_lowercase()
            || mod_lower == MOD_WIN.to_lowercase()
            || mod_lower == MOD_META.to_lowercase()
        {
            modifiers |= Modifiers::SUPER;
        } else {
            return Err(format!("Unknown modifier: {}", modifier));
        }
    }

    // Parse key code
    let code = parse_key_code(key_part)?;

    Ok((modifiers, code))
}

/// Convert modifiers and code to a display string
///
/// Uses platform-appropriate names:
/// - macOS: `Cmd` for Super key
/// - Windows/Linux: `Super` for Super key
///
/// # Arguments
/// * `modifiers` - The modifier keys
/// * `code` - The key code
///
/// # Returns
/// Display string like "Ctrl+Shift+Space" or "Cmd+A"
pub fn hotkey_to_string(modifiers: Modifiers, code: Code) -> String {
    let mut parts: Vec<&str> = Vec::new();

    // Order: Ctrl, Alt, Shift, Super/Cmd
    if modifiers.contains(Modifiers::CONTROL) {
        parts.push(MOD_CTRL);
    }
    if modifiers.contains(Modifiers::ALT) {
        parts.push(MOD_ALT);
    }
    if modifiers.contains(Modifiers::SHIFT) {
        parts.push(MOD_SHIFT);
    }
    if modifiers.contains(Modifiers::SUPER) {
        #[cfg(target_os = "macos")]
        parts.push(MOD_CMD);
        #[cfg(not(target_os = "macos"))]
        parts.push(MOD_SUPER);
    }

    let key_str = code_to_string_internal(code);
    if parts.is_empty() {
        key_str
    } else {
        format!(
            "{}{}{}",
            parts.join(HOTKEY_SEPARATOR),
            HOTKEY_SEPARATOR,
            key_str
        )
    }
}

/// Build a hotkey string from Iced keyboard modifiers and a key string
///
/// This is used during PTT key capture to build the stored hotkey string
/// from Iced's keyboard event modifiers.
///
/// # Arguments
/// * `modifiers` - Iced keyboard modifiers from the key event
/// * `key` - The key string (e.g., "Space", "F1", "A")
///
/// # Returns
/// Hotkey string like "Ctrl+Shift+Space" or just "F1" if no modifiers
pub fn build_hotkey_string(modifiers: &iced::keyboard::Modifiers, key: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();

    // Order: Ctrl, Alt, Shift, Super/Cmd
    if modifiers.control() {
        parts.push(MOD_CTRL);
    }
    if modifiers.alt() {
        parts.push(MOD_ALT);
    }
    if modifiers.shift() {
        parts.push(MOD_SHIFT);
    }
    if modifiers.logo() {
        #[cfg(target_os = "macos")]
        parts.push(MOD_CMD);
        #[cfg(not(target_os = "macos"))]
        parts.push(MOD_SUPER);
    }

    if parts.is_empty() {
        key.to_string()
    } else {
        format!(
            "{}{}{}",
            parts.join(HOTKEY_SEPARATOR),
            HOTKEY_SEPARATOR,
            key
        )
    }
}

/// Parse a key code string into a Code enum (internal helper)
///
/// # Arguments
/// * `key` - Key string (e.g., "`", "F1", "Space", "KeyA")
///
/// # Returns
/// * `Ok(Code)` - Parsed key code
/// * `Err(String)` - Error if key string is invalid
pub fn parse_key_code(key: &str) -> Result<Code, String> {
    let code = match key.to_lowercase().as_str() {
        // Special characters
        "`" | "backquote" | "grave" => Code::Backquote,
        "-" | "minus" => Code::Minus,
        "=" | "equal" => Code::Equal,
        "[" | "bracketleft" => Code::BracketLeft,
        "]" | "bracketright" => Code::BracketRight,
        "\\" | "backslash" => Code::Backslash,
        ";" | "semicolon" => Code::Semicolon,
        "'" | "quote" => Code::Quote,
        "," | "comma" => Code::Comma,
        "." | "period" => Code::Period,
        "/" | "slash" => Code::Slash,

        // Function keys
        "f1" => Code::F1,
        "f2" => Code::F2,
        "f3" => Code::F3,
        "f4" => Code::F4,
        "f5" => Code::F5,
        "f6" => Code::F6,
        "f7" => Code::F7,
        "f8" => Code::F8,
        "f9" => Code::F9,
        "f10" => Code::F10,
        "f11" => Code::F11,
        "f12" => Code::F12,
        "f13" => Code::F13,
        "f14" => Code::F14,
        "f15" => Code::F15,
        "f16" => Code::F16,
        "f17" => Code::F17,
        "f18" => Code::F18,
        "f19" => Code::F19,
        "f20" => Code::F20,
        "f21" => Code::F21,
        "f22" => Code::F22,
        "f23" => Code::F23,
        "f24" => Code::F24,

        // Number keys
        "0" | "digit0" => Code::Digit0,
        "1" | "digit1" => Code::Digit1,
        "2" | "digit2" => Code::Digit2,
        "3" | "digit3" => Code::Digit3,
        "4" | "digit4" => Code::Digit4,
        "5" | "digit5" => Code::Digit5,
        "6" | "digit6" => Code::Digit6,
        "7" | "digit7" => Code::Digit7,
        "8" | "digit8" => Code::Digit8,
        "9" | "digit9" => Code::Digit9,

        // Letter keys
        "a" | "keya" => Code::KeyA,
        "b" | "keyb" => Code::KeyB,
        "c" | "keyc" => Code::KeyC,
        "d" | "keyd" => Code::KeyD,
        "e" | "keye" => Code::KeyE,
        "f" | "keyf" => Code::KeyF,
        "g" | "keyg" => Code::KeyG,
        "h" | "keyh" => Code::KeyH,
        "i" | "keyi" => Code::KeyI,
        "j" | "keyj" => Code::KeyJ,
        "k" | "keyk" => Code::KeyK,
        "l" | "keyl" => Code::KeyL,
        "m" | "keym" => Code::KeyM,
        "n" | "keyn" => Code::KeyN,
        "o" | "keyo" => Code::KeyO,
        "p" | "keyp" => Code::KeyP,
        "q" | "keyq" => Code::KeyQ,
        "r" | "keyr" => Code::KeyR,
        "s" | "keys" => Code::KeyS,
        "t" | "keyt" => Code::KeyT,
        "u" | "keyu" => Code::KeyU,
        "v" | "keyv" => Code::KeyV,
        "w" | "keyw" => Code::KeyW,
        "x" | "keyx" => Code::KeyX,
        "y" | "keyy" => Code::KeyY,
        "z" | "keyz" => Code::KeyZ,

        // Control keys
        "space" => Code::Space,
        "enter" | "return" => Code::Enter,
        "tab" => Code::Tab,
        "escape" | "esc" => Code::Escape,
        "backspace" => Code::Backspace,
        "delete" => Code::Delete,
        "insert" => Code::Insert,
        "home" => Code::Home,
        "end" => Code::End,
        "pageup" => Code::PageUp,
        "pagedown" => Code::PageDown,

        // Arrow keys
        "arrowup" | "up" => Code::ArrowUp,
        "arrowdown" | "down" => Code::ArrowDown,
        "arrowleft" | "left" => Code::ArrowLeft,
        "arrowright" | "right" => Code::ArrowRight,

        // Numpad keys
        "numpad0" => Code::Numpad0,
        "numpad1" => Code::Numpad1,
        "numpad2" => Code::Numpad2,
        "numpad3" => Code::Numpad3,
        "numpad4" => Code::Numpad4,
        "numpad5" => Code::Numpad5,
        "numpad6" => Code::Numpad6,
        "numpad7" => Code::Numpad7,
        "numpad8" => Code::Numpad8,
        "numpad9" => Code::Numpad9,
        "numpadadd" | "numpadplus" => Code::NumpadAdd,
        "numpadsubtract" | "numpadminus" => Code::NumpadSubtract,
        "numpadmultiply" => Code::NumpadMultiply,
        "numpaddivide" => Code::NumpadDivide,
        "numpaddecimal" => Code::NumpadDecimal,
        "numpadenter" => Code::NumpadEnter,

        _ => return Err(format!("Unknown key code: {}", key)),
    };

    Ok(code)
}

/// Convert a Code enum to a display string (internal helper)
fn code_to_string_internal(code: Code) -> String {
    match code {
        // Special characters
        Code::Backquote => "`".to_string(),
        Code::Minus => "-".to_string(),
        Code::Equal => "=".to_string(),
        Code::BracketLeft => "[".to_string(),
        Code::BracketRight => "]".to_string(),
        Code::Backslash => "\\".to_string(),
        Code::Semicolon => ";".to_string(),
        Code::Quote => "'".to_string(),
        Code::Comma => ",".to_string(),
        Code::Period => ".".to_string(),
        Code::Slash => "/".to_string(),

        // Function keys
        Code::F1 => "F1".to_string(),
        Code::F2 => "F2".to_string(),
        Code::F3 => "F3".to_string(),
        Code::F4 => "F4".to_string(),
        Code::F5 => "F5".to_string(),
        Code::F6 => "F6".to_string(),
        Code::F7 => "F7".to_string(),
        Code::F8 => "F8".to_string(),
        Code::F9 => "F9".to_string(),
        Code::F10 => "F10".to_string(),
        Code::F11 => "F11".to_string(),
        Code::F12 => "F12".to_string(),
        Code::F13 => "F13".to_string(),
        Code::F14 => "F14".to_string(),
        Code::F15 => "F15".to_string(),
        Code::F16 => "F16".to_string(),
        Code::F17 => "F17".to_string(),
        Code::F18 => "F18".to_string(),
        Code::F19 => "F19".to_string(),
        Code::F20 => "F20".to_string(),
        Code::F21 => "F21".to_string(),
        Code::F22 => "F22".to_string(),
        Code::F23 => "F23".to_string(),
        Code::F24 => "F24".to_string(),

        // Number keys
        Code::Digit0 => "0".to_string(),
        Code::Digit1 => "1".to_string(),
        Code::Digit2 => "2".to_string(),
        Code::Digit3 => "3".to_string(),
        Code::Digit4 => "4".to_string(),
        Code::Digit5 => "5".to_string(),
        Code::Digit6 => "6".to_string(),
        Code::Digit7 => "7".to_string(),
        Code::Digit8 => "8".to_string(),
        Code::Digit9 => "9".to_string(),

        // Letter keys
        Code::KeyA => "A".to_string(),
        Code::KeyB => "B".to_string(),
        Code::KeyC => "C".to_string(),
        Code::KeyD => "D".to_string(),
        Code::KeyE => "E".to_string(),
        Code::KeyF => "F".to_string(),
        Code::KeyG => "G".to_string(),
        Code::KeyH => "H".to_string(),
        Code::KeyI => "I".to_string(),
        Code::KeyJ => "J".to_string(),
        Code::KeyK => "K".to_string(),
        Code::KeyL => "L".to_string(),
        Code::KeyM => "M".to_string(),
        Code::KeyN => "N".to_string(),
        Code::KeyO => "O".to_string(),
        Code::KeyP => "P".to_string(),
        Code::KeyQ => "Q".to_string(),
        Code::KeyR => "R".to_string(),
        Code::KeyS => "S".to_string(),
        Code::KeyT => "T".to_string(),
        Code::KeyU => "U".to_string(),
        Code::KeyV => "V".to_string(),
        Code::KeyW => "W".to_string(),
        Code::KeyX => "X".to_string(),
        Code::KeyY => "Y".to_string(),
        Code::KeyZ => "Z".to_string(),

        // Control keys
        Code::Space => "Space".to_string(),
        Code::Enter => "Enter".to_string(),
        Code::Tab => "Tab".to_string(),
        Code::Escape => "Escape".to_string(),
        Code::Backspace => "Backspace".to_string(),
        Code::Delete => "Delete".to_string(),
        Code::Insert => "Insert".to_string(),
        Code::Home => "Home".to_string(),
        Code::End => "End".to_string(),
        Code::PageUp => "PageUp".to_string(),
        Code::PageDown => "PageDown".to_string(),

        // Arrow keys
        Code::ArrowUp => "Up".to_string(),
        Code::ArrowDown => "Down".to_string(),
        Code::ArrowLeft => "Left".to_string(),
        Code::ArrowRight => "Right".to_string(),

        // Numpad keys
        Code::Numpad0 => "Numpad0".to_string(),
        Code::Numpad1 => "Numpad1".to_string(),
        Code::Numpad2 => "Numpad2".to_string(),
        Code::Numpad3 => "Numpad3".to_string(),
        Code::Numpad4 => "Numpad4".to_string(),
        Code::Numpad5 => "Numpad5".to_string(),
        Code::Numpad6 => "Numpad6".to_string(),
        Code::Numpad7 => "Numpad7".to_string(),
        Code::Numpad8 => "Numpad8".to_string(),
        Code::Numpad9 => "Numpad9".to_string(),
        Code::NumpadAdd => "NumpadPlus".to_string(),
        Code::NumpadSubtract => "NumpadMinus".to_string(),
        Code::NumpadMultiply => "NumpadMultiply".to_string(),
        Code::NumpadDivide => "NumpadDivide".to_string(),
        Code::NumpadDecimal => "NumpadDecimal".to_string(),
        Code::NumpadEnter => "NumpadEnter".to_string(),

        // Default for unknown codes
        _ => format!("{:?}", code),
    }
}

/// Convert a Code enum to a display string (used in tests)
#[cfg(test)]
pub fn code_to_string(code: Code) -> String {
    code_to_string_internal(code)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_key_code_special() {
        assert_eq!(parse_key_code("`").unwrap(), Code::Backquote);
        assert_eq!(parse_key_code("backquote").unwrap(), Code::Backquote);
        assert_eq!(parse_key_code("-").unwrap(), Code::Minus);
        assert_eq!(parse_key_code("/").unwrap(), Code::Slash);
    }

    #[test]
    fn test_parse_key_code_function() {
        assert_eq!(parse_key_code("F1").unwrap(), Code::F1);
        assert_eq!(parse_key_code("f1").unwrap(), Code::F1);
        assert_eq!(parse_key_code("F12").unwrap(), Code::F12);
        assert_eq!(parse_key_code("F13").unwrap(), Code::F13);
        assert_eq!(parse_key_code("f13").unwrap(), Code::F13);
        assert_eq!(parse_key_code("F24").unwrap(), Code::F24);
        assert_eq!(parse_key_code("f24").unwrap(), Code::F24);
    }

    #[test]
    fn test_parse_key_code_letters() {
        assert_eq!(parse_key_code("a").unwrap(), Code::KeyA);
        assert_eq!(parse_key_code("A").unwrap(), Code::KeyA);
        assert_eq!(parse_key_code("KeyA").unwrap(), Code::KeyA);
        assert_eq!(parse_key_code("z").unwrap(), Code::KeyZ);
    }

    #[test]
    fn test_parse_key_code_numbers() {
        assert_eq!(parse_key_code("0").unwrap(), Code::Digit0);
        assert_eq!(parse_key_code("9").unwrap(), Code::Digit9);
        assert_eq!(parse_key_code("digit5").unwrap(), Code::Digit5);
    }

    #[test]
    fn test_parse_key_code_control() {
        assert_eq!(parse_key_code("space").unwrap(), Code::Space);
        assert_eq!(parse_key_code("enter").unwrap(), Code::Enter);
        assert_eq!(parse_key_code("tab").unwrap(), Code::Tab);
        assert_eq!(parse_key_code("escape").unwrap(), Code::Escape);
    }

    #[test]
    fn test_parse_key_code_invalid() {
        assert!(parse_key_code("invalid").is_err());
        assert!(parse_key_code("").is_err());
    }

    #[test]
    fn test_code_to_string_roundtrip() {
        let codes = vec![
            Code::Backquote,
            Code::F1,
            Code::F13,
            Code::F24,
            Code::KeyA,
            Code::Digit5,
            Code::Space,
        ];

        for code in codes {
            let s = code_to_string(code);
            let parsed = parse_key_code(&s).unwrap();
            assert_eq!(code, parsed, "Roundtrip failed for {:?}", code);
        }
    }

    #[test]
    fn test_ptt_state_default() {
        assert_eq!(PttState::default(), PttState::Idle);
    }

    #[test]
    fn test_parse_hotkey_no_modifiers() {
        let (modifiers, code) = parse_hotkey("F1").unwrap();
        assert_eq!(modifiers, Modifiers::empty());
        assert_eq!(code, Code::F1);

        let (modifiers, code) = parse_hotkey("Space").unwrap();
        assert_eq!(modifiers, Modifiers::empty());
        assert_eq!(code, Code::Space);
    }

    #[test]
    fn test_parse_hotkey_single_modifier() {
        let (modifiers, code) = parse_hotkey("Ctrl+Space").unwrap();
        assert_eq!(modifiers, Modifiers::CONTROL);
        assert_eq!(code, Code::Space);

        let (modifiers, code) = parse_hotkey("Alt+F1").unwrap();
        assert_eq!(modifiers, Modifiers::ALT);
        assert_eq!(code, Code::F1);

        let (modifiers, code) = parse_hotkey("Shift+A").unwrap();
        assert_eq!(modifiers, Modifiers::SHIFT);
        assert_eq!(code, Code::KeyA);
    }

    #[test]
    fn test_parse_hotkey_multiple_modifiers() {
        let (modifiers, code) = parse_hotkey("Ctrl+Shift+Space").unwrap();
        assert_eq!(modifiers, Modifiers::CONTROL | Modifiers::SHIFT);
        assert_eq!(code, Code::Space);

        let (modifiers, code) = parse_hotkey("Ctrl+Alt+Shift+A").unwrap();
        assert_eq!(
            modifiers,
            Modifiers::CONTROL | Modifiers::ALT | Modifiers::SHIFT
        );
        assert_eq!(code, Code::KeyA);
    }

    #[test]
    fn test_parse_hotkey_super_variants() {
        // All these should map to SUPER modifier
        let (modifiers, _) = parse_hotkey("Super+A").unwrap();
        assert_eq!(modifiers, Modifiers::SUPER);

        let (modifiers, _) = parse_hotkey("Cmd+A").unwrap();
        assert_eq!(modifiers, Modifiers::SUPER);

        let (modifiers, _) = parse_hotkey("Command+A").unwrap();
        assert_eq!(modifiers, Modifiers::SUPER);

        let (modifiers, _) = parse_hotkey("Win+A").unwrap();
        assert_eq!(modifiers, Modifiers::SUPER);

        let (modifiers, _) = parse_hotkey("Meta+A").unwrap();
        assert_eq!(modifiers, Modifiers::SUPER);
    }

    #[test]
    fn test_parse_hotkey_case_insensitive() {
        let (modifiers, code) = parse_hotkey("ctrl+shift+space").unwrap();
        assert_eq!(modifiers, Modifiers::CONTROL | Modifiers::SHIFT);
        assert_eq!(code, Code::Space);

        let (modifiers, code) = parse_hotkey("CTRL+SHIFT+SPACE").unwrap();
        assert_eq!(modifiers, Modifiers::CONTROL | Modifiers::SHIFT);
        assert_eq!(code, Code::Space);
    }

    #[test]
    fn test_parse_hotkey_invalid() {
        assert!(parse_hotkey("").is_err());
        assert!(parse_hotkey("InvalidMod+A").is_err());
        assert!(parse_hotkey("Ctrl+InvalidKey").is_err());
    }

    #[test]
    fn test_hotkey_to_string_no_modifiers() {
        let s = hotkey_to_string(Modifiers::empty(), Code::F1);
        assert_eq!(s, "F1");

        let s = hotkey_to_string(Modifiers::empty(), Code::Space);
        assert_eq!(s, "Space");
    }

    #[test]
    fn test_hotkey_to_string_with_modifiers() {
        let s = hotkey_to_string(Modifiers::CONTROL, Code::Space);
        assert_eq!(s, "Ctrl+Space");

        let s = hotkey_to_string(Modifiers::CONTROL | Modifiers::SHIFT, Code::KeyA);
        assert_eq!(s, "Ctrl+Shift+A");

        let s = hotkey_to_string(
            Modifiers::CONTROL | Modifiers::ALT | Modifiers::SHIFT,
            Code::F1,
        );
        assert_eq!(s, "Ctrl+Alt+Shift+F1");
    }

    #[test]
    fn test_hotkey_to_string_modifier_order() {
        // Regardless of input order, output should be Ctrl+Alt+Shift+Super
        let s = hotkey_to_string(Modifiers::SHIFT | Modifiers::CONTROL, Code::KeyA);
        assert_eq!(s, "Ctrl+Shift+A");

        let s = hotkey_to_string(Modifiers::ALT | Modifiers::CONTROL, Code::KeyA);
        assert_eq!(s, "Ctrl+Alt+A");
    }

    #[test]
    fn test_hotkey_roundtrip() {
        // Test that parse -> to_string -> parse gives same result
        let test_cases = vec![
            "F1",
            "Ctrl+Space",
            "Alt+F1",
            "Shift+A",
            "Ctrl+Shift+Space",
            "Ctrl+NumpadPlus",
        ];

        for input in test_cases {
            let (modifiers, code) = parse_hotkey(input).unwrap();
            let output = hotkey_to_string(modifiers, code);
            let (modifiers2, code2) = parse_hotkey(&output).unwrap();
            assert_eq!(modifiers, modifiers2, "Modifiers mismatch for {}", input);
            assert_eq!(code, code2, "Code mismatch for {}", input);
        }
    }

    #[test]
    fn test_numpad_plus_aliases() {
        // NumpadPlus can be parsed as "numpadplus" or "numpadadd"
        let (_, code1) = parse_hotkey("NumpadPlus").unwrap();
        let (_, code2) = parse_hotkey("NumpadAdd").unwrap();
        assert_eq!(code1, Code::NumpadAdd);
        assert_eq!(code2, Code::NumpadAdd);

        // Display should be "NumpadPlus" (not "Numpad+" which would break parsing)
        let s = hotkey_to_string(Modifiers::empty(), Code::NumpadAdd);
        assert_eq!(s, "NumpadPlus");

        // With modifiers should work correctly
        let (modifiers, code) = parse_hotkey("Ctrl+NumpadPlus").unwrap();
        assert_eq!(modifiers, Modifiers::CONTROL);
        assert_eq!(code, Code::NumpadAdd);
    }

    #[test]
    fn test_numpad_operator_roundtrip() {
        // All numpad operator keys must roundtrip correctly
        let numpad_codes = vec![
            Code::NumpadAdd,
            Code::NumpadSubtract,
            Code::NumpadMultiply,
            Code::NumpadDivide,
            Code::NumpadDecimal,
        ];

        for code in numpad_codes {
            let s = code_to_string(code);
            let parsed = parse_key_code(&s).unwrap();
            assert_eq!(code, parsed, "Roundtrip failed for {:?} -> {}", code, s);

            // Also test with modifiers
            let hotkey_str = hotkey_to_string(Modifiers::CONTROL, code);
            let (modifiers, parsed_code) = parse_hotkey(&hotkey_str).unwrap();
            assert_eq!(modifiers, Modifiers::CONTROL);
            assert_eq!(code, parsed_code, "Hotkey roundtrip failed for {:?}", code);
        }
    }

    #[test]
    fn test_numpad_minus_aliases() {
        // NumpadMinus can be parsed as "numpadminus" or "numpadsubtract"
        let (_, code1) = parse_hotkey("NumpadMinus").unwrap();
        let (_, code2) = parse_hotkey("NumpadSubtract").unwrap();
        assert_eq!(code1, Code::NumpadSubtract);
        assert_eq!(code2, Code::NumpadSubtract);
    }

    #[test]
    fn test_special_char_keys_with_modifiers() {
        // Special character keys like -, /, etc. should work with modifiers
        // The key is after the last +, so "Ctrl+-" splits to ["Ctrl", "-"]
        let test_cases = vec![
            ("Ctrl+-", Modifiers::CONTROL, Code::Minus),
            ("Ctrl+/", Modifiers::CONTROL, Code::Slash),
            ("Ctrl+=", Modifiers::CONTROL, Code::Equal),
            ("Alt+`", Modifiers::ALT, Code::Backquote),
            ("Shift+[", Modifiers::SHIFT, Code::BracketLeft),
            (
                "Ctrl+Shift+;",
                Modifiers::CONTROL | Modifiers::SHIFT,
                Code::Semicolon,
            ),
        ];

        for (input, expected_mods, expected_code) in test_cases {
            let (modifiers, code) = parse_hotkey(input).unwrap();
            assert_eq!(modifiers, expected_mods, "Modifiers mismatch for {}", input);
            assert_eq!(code, expected_code, "Code mismatch for {}", input);

            // Also verify roundtrip
            let output = hotkey_to_string(modifiers, code);
            let (mods2, code2) = parse_hotkey(&output).unwrap();
            assert_eq!(modifiers, mods2, "Roundtrip modifiers failed for {}", input);
            assert_eq!(code, code2, "Roundtrip code failed for {}", input);
        }
    }
}
