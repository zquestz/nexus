//! System tray icon support for Windows and Linux
//!
//! This module provides system tray functionality including:
//! - Tray icon with 6 states (priority order):
//!   1. Disconnected (gray) - no active connections
//!   2. VoiceMuted (yellow dot) - in voice, deafened
//!   3. VoiceSpeaking (green dot) - in voice, actively transmitting
//!   4. VoiceActive (purple dot) - in voice, idle
//!   5. Unread (red dot) - has unread DMs
//!   6. Normal (base icon) - connected, no activity
//! - Context menu with show/hide, mute, and quit actions
//! - Dynamic tooltip updates
//!
//! ## Platform Notes
//!
//! - **Linux**: Uses ksni (StatusNotifierItem D-Bus protocol). Left-click toggles
//!   window visibility. Right-click shows the menu.
//! - **Windows**: Uses tray-icon (native system tray). Left-click toggles window
//!   visibility. Right-click shows the menu.
//! - **macOS**: Not supported - uses dock badges instead (separate feature).

#![cfg(not(target_os = "macos"))]

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
pub use linux::{TrayManager, tray_subscription};
#[cfg(target_os = "windows")]
pub use windows::{TrayManager, tray_subscription};

use crate::i18n::{t, t_args};

// =============================================================================
// Constants
// =============================================================================

/// Tray icon identifier (internal, not user-facing)
#[cfg(target_os = "linux")]
pub const TRAY_ID: &str = "nexus-bbs";

/// Tray icon title/name
#[cfg(target_os = "linux")]
pub const TRAY_TITLE: &str = "Nexus BBS";

/// Bytes per pixel for ARGB/RGBA image data
#[cfg(target_os = "linux")]
pub const BYTES_PER_PIXEL: usize = 4;

// =============================================================================
// Embedded Icons
// =============================================================================

/// Normal state icon (base Nexus logo)
pub const ICON_NORMAL: &[u8] = include_bytes!("../../assets/tray/tray-normal.png");

/// Disconnected state icon (grayed out)
pub const ICON_DISCONNECTED: &[u8] = include_bytes!("../../assets/tray/tray-disconnected.png");

/// Voice active state icon (purple dot - in voice session)
pub const ICON_VOICE: &[u8] = include_bytes!("../../assets/tray/tray-voice.png");

/// Voice speaking state icon (green dot - actively transmitting)
pub const ICON_VOICE_SPEAKING: &[u8] = include_bytes!("../../assets/tray/tray-voice-speaking.png");

/// Voice muted state icon (yellow dot - deafened)
pub const ICON_VOICE_MUTED: &[u8] = include_bytes!("../../assets/tray/tray-voice-muted.png");

/// Unread conversations state icon (red dot)
pub const ICON_UNREAD: &[u8] = include_bytes!("../../assets/tray/tray-unread.png");

// =============================================================================
// Tray State
// =============================================================================

/// Tray icon state (priority order - highest priority wins)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TrayState {
    /// No active connections (highest priority - grayed icon)
    #[default]
    Disconnected,
    /// In voice session and deafened (yellow dot)
    VoiceMuted,
    /// In voice session and actively transmitting (green dot)
    VoiceSpeaking,
    /// In voice session but not transmitting (purple dot)
    VoiceActive,
    /// Has unread direct messages (red dot)
    Unread,
    /// Normal connected state (base icon)
    Normal,
}

impl TrayState {
    /// Get the icon data for this state
    pub fn icon_data(self) -> &'static [u8] {
        match self {
            TrayState::Disconnected => ICON_DISCONNECTED,
            TrayState::VoiceMuted => ICON_VOICE_MUTED,
            TrayState::VoiceSpeaking => ICON_VOICE_SPEAKING,
            TrayState::VoiceActive => ICON_VOICE,
            TrayState::Unread => ICON_UNREAD,
            TrayState::Normal => ICON_NORMAL,
        }
    }
}

// =============================================================================
// Tooltip Helpers
// =============================================================================

/// Build a tooltip string for the current state
pub fn build_tooltip(state: TrayState, voice_target: Option<&str>, unread_count: usize) -> String {
    match state {
        TrayState::Disconnected => t("tray-tooltip-disconnected"),
        TrayState::VoiceMuted => {
            if let Some(target) = voice_target {
                t_args("tray-tooltip-voice-muted", &[("target", target)])
            } else {
                t("tray-tooltip-normal")
            }
        }
        TrayState::VoiceSpeaking | TrayState::VoiceActive => {
            if let Some(target) = voice_target {
                t_args("tray-tooltip-voice", &[("target", target)])
            } else {
                t("tray-tooltip-normal")
            }
        }
        TrayState::Unread => t_args(
            "tray-tooltip-unread",
            &[("count", &unread_count.to_string())],
        ),
        TrayState::Normal => t("tray-tooltip-normal"),
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tray_state_default() {
        assert_eq!(TrayState::default(), TrayState::Disconnected);
    }

    #[test]
    fn test_tray_state_icon_data() {
        // Just verify each state returns non-empty data
        assert!(!TrayState::Disconnected.icon_data().is_empty());
        assert!(!TrayState::VoiceMuted.icon_data().is_empty());
        assert!(!TrayState::VoiceSpeaking.icon_data().is_empty());
        assert!(!TrayState::VoiceActive.icon_data().is_empty());
        assert!(!TrayState::Unread.icon_data().is_empty());
        assert!(!TrayState::Normal.icon_data().is_empty());
    }

    #[test]
    fn test_build_tooltip_disconnected() {
        let tooltip = build_tooltip(TrayState::Disconnected, None, 0);
        assert!(!tooltip.is_empty());
    }

    #[test]
    fn test_build_tooltip_voice_active() {
        let tooltip = build_tooltip(TrayState::VoiceActive, Some("#general"), 0);
        assert!(!tooltip.is_empty());
    }

    #[test]
    fn test_build_tooltip_voice_speaking() {
        let tooltip = build_tooltip(TrayState::VoiceSpeaking, Some("#general"), 0);
        assert!(!tooltip.is_empty());
    }

    #[test]
    fn test_build_tooltip_voice_muted() {
        let tooltip = build_tooltip(TrayState::VoiceMuted, Some("#general"), 0);
        assert!(!tooltip.is_empty());
    }

    #[test]
    fn test_build_tooltip_voice_without_target() {
        // Edge case: in voice but no target (shouldn't happen, but defensive)
        let tooltip = build_tooltip(TrayState::VoiceActive, None, 0);
        assert!(!tooltip.is_empty());
    }

    #[test]
    fn test_build_tooltip_unread() {
        let tooltip = build_tooltip(TrayState::Unread, None, 5);
        assert!(!tooltip.is_empty());
    }

    #[test]
    fn test_build_tooltip_normal() {
        let tooltip = build_tooltip(TrayState::Normal, None, 0);
        assert!(!tooltip.is_empty());
    }
}
