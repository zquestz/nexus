//! macOS URL scheme handler for `nexus://` deep links
//!
//! On macOS, clicking a `nexus://` link delivers the URL via Apple Events
//! (`application:openURLs:`), not as a command-line argument. This module
//! registers a custom `NSApplicationDelegate` to receive those events and
//! forwards URLs through a crossbeam channel consumed by an async stream
//! subscription.

use crossbeam_channel::{Receiver, Sender};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{define_class, msg_send, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSApplication, NSApplicationDelegate};
use objc2_foundation::{NSArray, NSObject, NSObjectProtocol, NSURL};
use once_cell::sync::Lazy;

/// Channel for forwarding URLs from the Apple Event handler to the Iced event loop.
///
/// `crossbeam_channel` is used because both `Sender` and `Receiver` are
/// `Send + Sync`, which is required for use in a `static`. The standard
/// library's `mpsc::Receiver` is not `Sync` and would fail to compile.
static URL_CHANNEL: Lazy<(Sender<String>, Receiver<String>)> =
    Lazy::new(crossbeam_channel::unbounded);

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "NexusURLDelegate"]
    struct NexusAppDelegate;

    unsafe impl NSObjectProtocol for NexusAppDelegate {}

    unsafe impl NSApplicationDelegate for NexusAppDelegate {
        #[unsafe(method(application:openURLs:))]
        #[unsafe(method_family = none)]
        fn application_open_urls(&self, _application: &NSApplication, urls: &NSArray<NSURL>) {
            for url in urls.iter() {
                if let Some(abs) = url.absoluteString() {
                    let url_str = abs.to_string();
                    if crate::uri::is_nexus_uri(&url_str) {
                        let _ = URL_CHANNEL.0.send(url_str);
                    }
                }
            }
        }
    }
);

impl NexusAppDelegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        // SAFETY: `Self::alloc(mtm)` returns a valid allocated instance of our
        // NSObject subclass. Sending `init` to a freshly allocated NSObject
        // subclass with no custom ivars is the standard Objective-C
        // initialisation pattern and always succeeds.
        unsafe { msg_send![Self::alloc(mtm), init] }
    }
}

/// Install the macOS URL scheme delegate.
///
/// Must be called **after** the Iced/winit event loop has been created
/// (i.e. from `NexusApp::new()`), because `NSApplication::sharedApplication`
/// needs the event loop to exist first.
pub fn install() {
    let Some(mtm) = MainThreadMarker::new() else {
        eprintln!("macos_url: not on main thread, skipping delegate install");
        return;
    };

    let delegate = NexusAppDelegate::new(mtm);
    let app = NSApplication::sharedApplication(mtm);
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));

    // Leak the delegate so it lives for the entire process.
    //
    // `NexusAppDelegate` is `MainThreadOnly` (`!Send + !Sync`), so
    // `Retained<NexusAppDelegate>` cannot be stored in a `static`. Leaking
    // is the standard pattern for process-lifetime Objective-C objects.
    //
    // This is necessary because AppKit's `NSApplication.delegate` property
    // is `weak` — if we dropped our `Retained`, the delegate would be
    // deallocated and the weak reference would become nil.
    std::mem::forget(delegate);
}

/// Async stream that yields URLs received via Apple Events.
///
/// Blocks on `crossbeam_channel::Receiver::recv()` inside `spawn_blocking`,
/// so it consumes no CPU while waiting. Each received URL is emitted as a
/// `UriReceivedFromIpc` message, reusing the same handler path as the
/// Unix/Windows IPC mechanism.
pub fn url_stream() -> impl iced::futures::Stream<Item = crate::types::Message> {
    iced::futures::stream::unfold((), |()| async {
        // Block in a tokio worker thread until a URL arrives.
        let url = tokio::task::spawn_blocking(|| URL_CHANNEL.1.recv().ok())
            .await
            .ok()
            .flatten()?;

        Some((crate::types::Message::UriReceivedFromIpc(url), ()))
    })
}