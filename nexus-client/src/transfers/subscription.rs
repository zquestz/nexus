//! Transfer subscription - executes file transfers
//!
//! This module provides an Iced subscription that executes file transfers.
//! Each transfer gets its own subscription, keyed by the transfer's stable UUID.
//! This ensures the subscription remains alive as the transfer status changes
//! from Queued -> Connecting -> Transferring -> Completed/Failed.
//!
//! Uses a global registry pattern similar to network_stream to pass transfer
//! data to the subscription without closure captures.
//!
//! Cancellation is supported via a cancellation flag in each registry entry.
//! When pause/cancel is requested, the flag is set to true and the executor
//! checks it periodically. The subscription detects the abort and sends
//! appropriate events to the UI.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use iced::futures::{SinkExt, Stream};
use iced::stream;
use once_cell::sync::Lazy;
use tokio::sync::mpsc;
use uuid::Uuid;

use super::executor::{TransferEvent, execute_transfer};
use super::{Transfer, TransferStatus};
use crate::config::settings::ProxySettings;
use crate::i18n::{t, t_args};
use crate::network::ProxyConfig;
use crate::types::Message;

/// Channel size for the transfer event stream
const TRANSFER_CHANNEL_SIZE: usize = 100;

// =============================================================================
// Transfer Registry
// =============================================================================

/// Entry in the transfer registry containing all state for a pending transfer
struct TransferEntry {
    /// The transfer to execute
    transfer: Transfer,
    /// Flag to signal cancellation (checked by executor)
    cancel_flag: Arc<AtomicBool>,
    /// Proxy configuration (captured at queue time)
    proxy: Option<ProxyConfig>,
}

/// Global registry for pending transfers
///
/// When a transfer is queued, it's added here with its cancellation flag and
/// proxy config. The subscription retrieves the entry by ID and removes it
/// from the registry when execution starts.
///
/// Uses std::sync::Mutex (not tokio) because operations are trivial and
/// this avoids the need for block_on which can deadlock if called from async context.
static TRANSFER_REGISTRY: Lazy<Mutex<HashMap<Uuid, TransferEntry>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

// =============================================================================
// Registry Operations
// =============================================================================

/// Register a transfer in the global registry for the subscription to pick up
fn register_transfer(transfer: Transfer, proxy: Option<ProxyConfig>) {
    let id = transfer.id;
    let entry = TransferEntry {
        transfer,
        cancel_flag: Arc::new(AtomicBool::new(false)),
        proxy,
    };

    let mut registry = TRANSFER_REGISTRY
        .lock()
        .expect("transfer registry poisoned");
    registry.insert(id, entry);
}

/// Request cancellation of a transfer
///
/// Sets the cancellation flag for the given transfer ID. The executor will
/// check this flag and abort the transfer.
pub fn request_cancel(transfer_id: Uuid) {
    let registry = TRANSFER_REGISTRY
        .lock()
        .expect("transfer registry poisoned");
    if let Some(entry) = registry.get(&transfer_id) {
        entry.cancel_flag.store(true, Ordering::SeqCst);
    }
}

/// Get a transfer entry from the registry (clones the data, keeps entry for cancel support)
fn get_transfer_entry(
    transfer_id: Uuid,
) -> Option<(Transfer, Arc<AtomicBool>, Option<ProxyConfig>)> {
    let registry = TRANSFER_REGISTRY
        .lock()
        .expect("transfer registry poisoned");
    registry.get(&transfer_id).map(|entry| {
        (
            entry.transfer.clone(),
            Arc::clone(&entry.cancel_flag),
            entry.proxy.clone(),
        )
    })
}

/// Remove a transfer entry from the registry (called when transfer completes)
fn remove_transfer_entry(transfer_id: Uuid) {
    let mut registry = TRANSFER_REGISTRY
        .lock()
        .expect("transfer registry poisoned");
    registry.remove(&transfer_id);
}

// =============================================================================
// Subscription
// =============================================================================

/// Create an Iced subscription for a transfer
///
/// This subscription:
/// 1. Takes a transfer (queued or active)
/// 2. For queued transfers: registers in global registry and starts execution
/// 3. For active transfers: returns same subscription ID to keep stream alive
/// 4. Sends progress events back to the UI
///
/// The subscription ID is based on the transfer's stable UUID, so the subscription
/// remains alive even as status changes from Queued -> Connecting -> Transferring.
/// This is critical - if we returned a different subscription when status changed,
/// Iced would cancel the running stream.
pub fn transfer_subscription(
    transfer: &Transfer,
    proxy_settings: &ProxySettings,
) -> iced::Subscription<Message> {
    let transfer_id = transfer.id;

    // Only register and start execution for Queued transfers
    // Active transfers already have a running stream - just return the same
    // subscription ID to keep it alive
    if transfer.status == TransferStatus::Queued {
        // Register the transfer in the global registry with current proxy settings
        // Clone only for queued transfers that need to be registered
        let proxy = ProxyConfig::from_settings(proxy_settings);
        register_transfer(transfer.clone(), proxy);
    }

    // Use run_with with the transfer ID as key
    // For queued transfers: this starts a new stream that will execute the transfer
    // For active transfers: Iced sees the same key and keeps the existing stream running
    iced::Subscription::run_with(transfer_id, transfer_stream)
}

/// Create the async stream that executes a transfer
///
/// Takes a reference to the transfer ID for compatibility with Subscription::run_with.
/// Returns a boxed stream to allow use as a function pointer.
pub fn transfer_stream(
    transfer_id: &Uuid,
) -> std::pin::Pin<Box<dyn Stream<Item = Message> + Send>> {
    let transfer_id = *transfer_id;

    Box::pin(stream::channel(
        TRANSFER_CHANNEL_SIZE,
        move |mut output: iced::futures::channel::mpsc::Sender<Message>| async move {
            // Retrieve transfer entry from registry (keeps entry for cancel support)
            let Some((transfer, cancel_flag, proxy)) = get_transfer_entry(transfer_id) else {
                // Transfer not found in registry - shouldn't happen for queued transfers
                // For active transfers, this is expected (entry was already taken)
                return;
            };

            // Create channel for executor events
            let (event_tx, mut event_rx) = mpsc::unbounded_channel::<TransferEvent>();

            // Clone cancel flag for the executor
            let executor_cancel_flag = Some(cancel_flag);

            // Spawn the executor task
            let executor_handle = tokio::spawn(async move {
                execute_transfer(&transfer, event_tx, executor_cancel_flag, proxy).await
            });

            // Forward events from executor to Iced
            // Use a flag to track if we've seen a terminal event
            let mut seen_terminal = false;

            // Forward events from executor to Iced until channel closes
            while let Some(evt) = event_rx.recv().await {
                let is_terminal = matches!(
                    evt,
                    TransferEvent::Completed { .. }
                        | TransferEvent::Failed { .. }
                        | TransferEvent::Paused { .. }
                );

                // Send event to UI
                let send_result = output.send(Message::TransferProgress(evt)).await;
                if send_result.is_err() {
                    executor_handle.abort();
                    break;
                }

                if is_terminal {
                    seen_terminal = true;
                    break;
                }
            }

            // Wait for executor to finish
            let exec_result = executor_handle.await;

            // Remove entry from registry now that transfer is complete
            remove_transfer_entry(transfer_id);

            // If we didn't see a terminal event, send one based on executor result
            if !seen_terminal {
                match exec_result {
                    Ok(Ok(())) => {
                        // Executor succeeded but we missed the Completed event
                        let _ = output
                            .send(Message::TransferProgress(TransferEvent::Completed {
                                id: transfer_id,
                            }))
                            .await;
                    }
                    Ok(Err(err)) => {
                        // Executor returned error - use i18n for user-facing message
                        let error = t(err.to_i18n_key());
                        let _ = output
                            .send(Message::TransferProgress(TransferEvent::Failed {
                                id: transfer_id,
                                error,
                                error_kind: Some(err),
                            }))
                            .await;
                    }
                    Err(e) => {
                        // Task panicked or was cancelled
                        if !e.is_cancelled() {
                            let _ = output
                                .send(Message::TransferProgress(TransferEvent::Failed {
                                    id: transfer_id,
                                    error: t_args(
                                        "transfer-task-failed",
                                        &[("error", &e.to_string())],
                                    ),
                                    error_kind: None,
                                }))
                                .await;
                        }
                    }
                }
            }
        },
    ))
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::types::ConnectionInfo;

    fn create_test_transfer() -> Transfer {
        let connection = ConnectionInfo {
            server_name: "Test Server".to_string(),
            address: "127.0.0.1".to_string(),
            port: 7500,
            transfer_port: 7501,
            certificate_fingerprint: "AA:BB:CC".to_string(),
            username: "testuser".to_string(),
            password: "testpass".to_string(),
            nickname: String::new(),
        };

        Transfer::new_download(
            connection,
            "/test/file.txt".to_string(),
            false,
            false,
            PathBuf::from("/tmp/file.txt"),
            None,
        )
    }

    #[test]
    fn test_transfer_creation() {
        // Verify we can create a test transfer
        let transfer = create_test_transfer();
        assert_eq!(transfer.status, TransferStatus::Queued);
    }

    #[test]
    fn test_register_transfer() {
        let transfer = create_test_transfer();
        let id = transfer.id;

        register_transfer(transfer, None);

        let registry = TRANSFER_REGISTRY.lock().expect("registry poisoned");
        assert!(registry.contains_key(&id));
    }

    #[test]
    fn test_get_transfer_entry() {
        let transfer = create_test_transfer();
        let id = transfer.id;

        register_transfer(transfer, None);

        // Get should succeed and entry stays in registry
        let entry = get_transfer_entry(id);
        assert!(entry.is_some());

        // Second get should also succeed (entry not removed)
        let entry = get_transfer_entry(id);
        assert!(entry.is_some());

        // Explicit remove
        remove_transfer_entry(id);
        let entry = get_transfer_entry(id);
        assert!(entry.is_none());
    }

    #[test]
    fn test_request_cancel() {
        let transfer = create_test_transfer();
        let id = transfer.id;

        register_transfer(transfer, None);

        // Verify flag starts as false
        {
            let registry = TRANSFER_REGISTRY.lock().expect("registry poisoned");
            let entry = registry.get(&id).expect("entry should exist");
            assert!(!entry.cancel_flag.load(Ordering::SeqCst));
        }

        // Request cancellation
        request_cancel(id);

        // Verify flag was set
        {
            let registry = TRANSFER_REGISTRY.lock().expect("registry poisoned");
            let entry = registry.get(&id).expect("entry should exist");
            assert!(entry.cancel_flag.load(Ordering::SeqCst));
        }
    }

    #[test]
    fn test_request_cancel_while_active() {
        let transfer = create_test_transfer();
        let id = transfer.id;

        register_transfer(transfer, None);

        // Simulate what happens when a transfer starts: get entry but keep in registry
        let (_, cancel_flag, _) = get_transfer_entry(id).expect("entry should exist");

        // Entry should still be in registry
        assert!(get_transfer_entry(id).is_some());

        // Request cancellation - should still find it in registry
        request_cancel(id);

        // Verify flag was set (via our cloned Arc)
        assert!(cancel_flag.load(Ordering::SeqCst));

        // Clean up
        remove_transfer_entry(id);
        assert!(get_transfer_entry(id).is_none());
    }

    #[test]
    fn test_proxy_config_from_settings() {
        let disabled_settings = ProxySettings {
            enabled: false,
            address: "127.0.0.1".to_string(),
            port: 9050,
            username: None,
            password: None,
        };
        assert!(ProxyConfig::from_settings(&disabled_settings).is_none());

        let enabled_settings = ProxySettings {
            enabled: true,
            address: "proxy.example.com".to_string(),
            port: 1080,
            username: Some("user".to_string()),
            password: Some("pass".to_string()),
        };
        let config = ProxyConfig::from_settings(&enabled_settings);
        assert!(config.is_some());
        let config = config.unwrap();
        assert_eq!(config.address, "proxy.example.com");
        assert_eq!(config.port, 1080);
        assert_eq!(config.username, Some("user".to_string()));
        assert_eq!(config.password, Some("pass".to_string()));
    }

    #[test]
    fn test_proxy_config_debug_redacts_password() {
        let config = ProxyConfig {
            address: "127.0.0.1".to_string(),
            port: 9050,
            username: Some("user".to_string()),
            password: Some("secret_password".to_string()),
        };

        let debug_output = format!("{:?}", config);
        assert!(debug_output.contains("127.0.0.1"));
        assert!(debug_output.contains("user"));
        assert!(!debug_output.contains("secret_password"));
        assert!(debug_output.contains("REDACTED"));
    }
}
