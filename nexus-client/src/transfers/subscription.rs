//! Transfer subscription - executes file transfers
//!
//! This module provides an Iced subscription that executes file transfers.
//! Each transfer gets its own subscription, keyed by the transfer's stable UUID.
//! This ensures the subscription remains alive as the transfer status changes
//! from Queued -> Connecting -> Transferring -> Completed/Failed.
//!
//! Uses a global registry pattern similar to network_stream to pass transfer
//! data to the subscription without closure captures.

use std::collections::HashMap;
use std::sync::Mutex;

use iced::futures::{SinkExt, Stream};
use iced::stream;
use once_cell::sync::Lazy;
use tokio::sync::mpsc;
use uuid::Uuid;

use super::executor::{TransferEvent, execute_transfer};
use super::{Transfer, TransferStatus};
use crate::types::Message;

/// Channel size for the transfer event stream
const TRANSFER_CHANNEL_SIZE: usize = 100;

/// Global registry for pending transfers
///
/// When a transfer is queued, it's added here. The subscription retrieves
/// it by ID and removes it from the registry.
///
/// Uses std::sync::Mutex (not tokio) because operations are trivial and
/// this avoids the need for block_on which can deadlock if called from async context.
pub static TRANSFER_REGISTRY: Lazy<Mutex<HashMap<Uuid, Transfer>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Register a transfer in the global registry for the subscription to pick up
fn register_transfer(transfer: Transfer) {
    let mut registry = TRANSFER_REGISTRY
        .lock()
        .expect("transfer registry poisoned");
    registry.insert(transfer.id, transfer);
}

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
pub fn transfer_subscription(transfer: Transfer) -> iced::Subscription<Message> {
    let transfer_id = transfer.id;

    // Only register and start execution for Queued transfers
    // Active transfers already have a running stream - just return the same
    // subscription ID to keep it alive
    if transfer.status == TransferStatus::Queued {
        // Register the transfer in the global registry
        // Uses sync Mutex so no async/block_on needed
        register_transfer(transfer);
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
            // Retrieve transfer from registry
            let transfer = {
                let mut registry = TRANSFER_REGISTRY
                    .lock()
                    .expect("transfer registry poisoned");
                registry.remove(&transfer_id)
            };

            let Some(transfer) = transfer else {
                // Transfer not found in registry - shouldn't happen
                let _ = output.send(Message::TransferStartNext).await;
                return;
            };

            // Create channel for executor events
            let (event_tx, mut event_rx) = mpsc::unbounded_channel::<TransferEvent>();

            // Spawn the executor task
            let executor_handle =
                tokio::spawn(async move { execute_transfer(&transfer, event_tx).await });

            // Forward events from executor to Iced
            // Use a flag to track if we've seen a terminal event
            let mut seen_terminal = false;

            // Keep receiving events until channel is closed (executor dropped event_tx)
            loop {
                tokio::select! {
                    event = event_rx.recv() => {
                        match event {
                            Some(evt) => {
                                let is_terminal = matches!(
                                    evt,
                                    TransferEvent::Completed { .. } | TransferEvent::Failed { .. }
                                );

                                // Send event to UI
                                let send_result = output.send(Message::TransferProgress(evt)).await;
                                if send_result.is_err() {
                                    executor_handle.abort();
                                    break;
                                }

                                // If transfer finished, signal to start next one
                                if is_terminal {
                                    seen_terminal = true;
                                    let _ = output.send(Message::TransferStartNext).await;
                                    break;
                                }
                            }
                            None => {
                                // Channel closed - executor finished
                                break;
                            }
                        }
                    }
                }
            }

            // Wait for executor to finish
            let exec_result = executor_handle.await;

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
                        let _ = output.send(Message::TransferStartNext).await;
                    }
                    Ok(Err(err)) => {
                        // Executor returned error
                        let _ = output
                            .send(Message::TransferProgress(TransferEvent::Failed {
                                id: transfer_id,
                                error: format!("Transfer error: {err}"),
                                error_kind: Some(err),
                            }))
                            .await;
                        let _ = output.send(Message::TransferStartNext).await;
                    }
                    Err(e) => {
                        // Task panicked or was cancelled
                        if !e.is_cancelled() {
                            let _ = output
                                .send(Message::TransferProgress(TransferEvent::Failed {
                                    id: transfer_id,
                                    error: format!("Transfer task failed: {e}"),
                                    error_kind: None,
                                }))
                                .await;
                            let _ = output.send(Message::TransferStartNext).await;
                        }
                    }
                }
            }
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transfers::TransferConnectionInfo;
    use std::path::PathBuf;

    fn create_test_transfer() -> Transfer {
        let connection = TransferConnectionInfo {
            server_name: "Test Server".to_string(),
            server_address: "127.0.0.1".to_string(),
            transfer_port: 7501,
            certificate_fingerprint: "AA:BB:CC".to_string(),
            username: "testuser".to_string(),
            password: "testpass".to_string(),
            nickname: None,
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

        register_transfer(transfer);

        let registry = TRANSFER_REGISTRY.lock().expect("registry poisoned");
        assert!(registry.contains_key(&id));
    }
}
