//! Microphone test functionality
//!
//! Provides a subscription for monitoring microphone input levels
//! in the Settings > Audio tab.
//!
//! Note: cpal's audio streams are not Send-safe, so mic testing runs
//! on a dedicated OS thread that sends level updates through a channel.

use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use iced::Subscription;
use iced::futures::{SinkExt, Stream};
use iced::stream;
use tokio::sync::mpsc;

use super::audio::AudioCapture;
use crate::types::Message;

// =============================================================================
// Types
// =============================================================================

/// Result from mic test thread - either a level update or an error
enum MicTestResult {
    /// Microphone level (0.0 - 1.0)
    Level(f32),
    /// Error message
    Error(String),
}

// =============================================================================
// Constants
// =============================================================================

/// How often to update the mic level display (10 fps — 8-segment meter
/// doesn't benefit from higher rates, and each update triggers a full
/// iced update/view cycle)
const MIC_LEVEL_UPDATE_INTERVAL_MS: u64 = 100;

/// Channel size for the stream
const STREAM_CHANNEL_SIZE: usize = 10;

// =============================================================================
// Mic Test Thread
// =============================================================================

/// Run mic test on a dedicated thread
///
/// This function runs the audio capture on a separate OS thread because
/// cpal's Stream type is not Send-safe.
fn run_mic_test_thread(
    input_device: String,
    result_tx: mpsc::UnboundedSender<MicTestResult>,
    running: Arc<AtomicBool>,
) {
    // Create audio capture
    let capture = match AudioCapture::new(&input_device) {
        Ok(c) => c,
        Err(e) => {
            // Send error to UI
            let _ = result_tx.send(MicTestResult::Error(format!(
                "Failed to open input device: {}",
                e
            )));
            return;
        }
    };

    // Start capturing
    if let Err(e) = capture.start() {
        let _ = result_tx.send(MicTestResult::Error(format!(
            "Failed to start capture: {}",
            e
        )));
        return;
    }

    // Poll at regular intervals
    let interval = Duration::from_millis(MIC_LEVEL_UPDATE_INTERVAL_MS);

    while running.load(Ordering::SeqCst) {
        // Check if capture is still active
        if !capture.is_active() {
            break;
        }

        // Check for capture errors
        if let Some(err) = capture.check_error() {
            let _ = result_tx.send(MicTestResult::Error(format!("Capture error: {}", err)));
            break;
        }

        // Get current input level
        let level = capture.get_input_level();

        // Send level update
        if result_tx.send(MicTestResult::Level(level)).is_err() {
            // Receiver dropped, stop the test
            break;
        }

        // Sleep until next update
        thread::sleep(interval);
    }

    // Capture stops automatically when dropped
}

// =============================================================================
// Subscription
// =============================================================================

/// Create a subscription for mic test level updates
///
/// This subscription captures audio from the microphone and emits
/// `Message::AudioMicLevel` messages with the current input level.
pub fn mic_test_subscription(input_device: String) -> Subscription<Message> {
    Subscription::run_with(input_device, mic_test_stream)
}

/// Stream that monitors microphone input level
///
/// Takes a reference to input_device for compatibility with Subscription::run_with.
/// Returns a boxed stream to allow use as a function pointer.
#[allow(clippy::ptr_arg)] // Required for Subscription::run_with function pointer signature
pub fn mic_test_stream(input_device: &String) -> Pin<Box<dyn Stream<Item = Message> + Send>> {
    let input_device = input_device.clone();
    Box::pin(stream::channel(
        STREAM_CHANNEL_SIZE,
        move |mut output: iced::futures::channel::mpsc::Sender<Message>| async move {
            // Create channel for results from the audio thread
            let (result_tx, mut result_rx) = mpsc::unbounded_channel::<MicTestResult>();

            // Flag to signal the thread to stop
            let running = Arc::new(AtomicBool::new(true));
            let running_clone = running.clone();

            // Spawn dedicated thread for audio capture
            let _handle: JoinHandle<()> = thread::spawn(move || {
                run_mic_test_thread(input_device, result_tx, running_clone);
            });

            // Receive results and forward to Iced
            while let Some(result) = result_rx.recv().await {
                let message = match result {
                    MicTestResult::Level(level) => Message::AudioMicLevel(level),
                    MicTestResult::Error(err) => Message::AudioMicError(err),
                };
                if output.send(message).await.is_err() {
                    // Channel closed, signal thread to stop
                    running.store(false, Ordering::SeqCst);
                    break;
                }
            }

            // Signal thread to stop (in case loop ended due to level_rx closing)
            running.store(false, Ordering::SeqCst);

            // Thread will stop on its own when running flag is false
            // or when level_tx is dropped (which happens when thread exits)
        },
    ))
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mic_level_update_interval() {
        // 10 fps for an 8-segment meter — each tick triggers a full view rebuild
        let interval = MIC_LEVEL_UPDATE_INTERVAL_MS;
        assert!(
            interval <= 200,
            "Update interval should be <= 200ms for responsive meter"
        );
        assert!(
            interval >= 50,
            "Update interval should be >= 50ms to avoid unnecessary view rebuilds"
        );
    }
}
