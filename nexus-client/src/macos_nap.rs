//! macOS App Nap prevention
//!
//! macOS aggressively throttles background applications via "App Nap",
//! suspending timers and network activity when the app loses focus.
//! This causes the 5-minute keepalive ping in the writer task to stop
//! firing, leading to NAT routers dropping idle TCP connections after
//! 30-60 minutes of silence.
//!
//! This module calls `[[NSProcessInfo processInfo] beginActivityWithOptions:reason:]`
//! at startup to tell macOS that this app has ongoing network activity
//! and its timers must not be throttled. The activity assertion uses
//! `NSActivityUserInitiatedAllowingIdleSystemSleep`, which prevents
//! App Nap timer coalescing while still allowing the system to sleep
//! when idle.
//!
//! Linux and Windows do not have an equivalent mechanism, so this
//! module is macOS-only.

use objc2::runtime::AnyObject;
use objc2::msg_send;
use objc2_foundation::NSString;

/// `NSActivityUserInitiatedAllowingIdleSystemSleep`
///
/// Apple defines `NSActivityUserInitiated` as `0x00FFFFFF`, which has bits
/// 0–23 all set (including bit 20, `NSActivityIdleSystemSleepDisabled`).
/// Clearing bit 20 gives `NSActivityUserInitiatedAllowingIdleSystemSleep`:
///
///   `0x00FFFFFF & !(1 << 20)` = `0x00EFFFFF`
///
/// This prevents App Nap timer throttling while still allowing the Mac
/// to sleep normally when idle.
const NS_ACTIVITY_USER_INITIATED_ALLOWING_IDLE_SYSTEM_SLEEP: u64 = 0x00EF_FFFF;

/// Disable App Nap by beginning a long-lived activity assertion.
///
/// The returned `NSObject` (the activity token) is intentionally leaked
/// so the assertion lasts for the entire process lifetime. Dropping it
/// would re-enable App Nap.
///
/// Must be called from the main thread (after Iced/winit initializes AppKit).
pub fn disable_app_nap() {
    // [NSProcessInfo processInfo]
    let process_info: *mut AnyObject = unsafe {
        msg_send![
            objc2::runtime::AnyClass::get(c"NSProcessInfo").expect("NSProcessInfo class not found"),
            processInfo
        ]
    };
    assert!(
        !process_info.is_null(),
        "NSProcessInfo.processInfo returned nil"
    );

    let reason = NSString::from_str("Maintaining server connections and keepalive timers");

    // [[NSProcessInfo processInfo] beginActivityWithOptions:reason:]
    // Returns an id<NSObject> activity token that must be retained.
    let activity: Option<objc2::rc::Retained<AnyObject>> = unsafe {
        msg_send![
            &*process_info,
            beginActivityWithOptions: NS_ACTIVITY_USER_INITIATED_ALLOWING_IDLE_SYSTEM_SLEEP,
            reason: &*reason
        ]
    };

    match activity {
        Some(token) => {
            // Leak the token so the assertion lives for the entire process.
            std::mem::forget(token);
        }
        None => {
            eprintln!("macos_nap: beginActivityWithOptions returned nil");
        }
    }
}
