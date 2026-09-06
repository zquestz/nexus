//! Failure injection shared by transfer executor tests.

use std::future::{Future, poll_fn};
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll, ready};
use std::time::Duration;

use tokio::io::AsyncWrite;

/// Limit each executor poll to one cooperative completion, then delay the next
/// poll past a keepalive boundary. Even cached file reads must yield.
pub(super) async fn with_slow_polls<F: Future>(future: F) -> F::Output {
    tokio::pin!(future);
    let mut delay = None;
    poll_fn(|cx| {
        ready!(
            delay
                .get_or_insert_with(|| Box::pin(tokio::time::sleep(
                    nexus_common::KEEPALIVE_INTERVAL + Duration::from_secs(1),
                )))
                .as_mut()
                .poll(cx)
        );
        loop {
            let budget = ready!(tokio::task::coop::poll_proceed(cx));
            if !tokio::task::coop::has_budget_remaining() {
                // Restoring the final reservation leaves exactly one completion.
                drop(budget);
                break;
            }
            budget.made_progress();
        }
        let result = future.as_mut().poll(cx);
        delay = None;
        result
    })
    .await
}

#[derive(Clone, Copy, Debug)]
pub(super) enum WriteFailure {
    Error,
    Zero,
    Stall,
    FlushError,
    FlushStall,
}

/// Non-stalling failures recover immediately, exposing any later frame writes.
pub(super) struct FailingWriter {
    pub(super) bytes: Vec<u8>,
    flushes_to_skip: usize,
    prefix_remaining: usize,
    failure: WriteFailure,
    is_failed: bool,
}

impl FailingWriter {
    pub(super) fn new(flushes_to_skip: usize, prefix_len: usize, failure: WriteFailure) -> Self {
        Self {
            bytes: Vec::new(),
            flushes_to_skip,
            prefix_remaining: prefix_len,
            failure,
            is_failed: false,
        }
    }
}

impl AsyncWrite for FailingWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        if this.flushes_to_skip > 0
            || this.is_failed
            || matches!(
                this.failure,
                WriteFailure::FlushError | WriteFailure::FlushStall
            )
        {
            this.bytes.extend_from_slice(buf);
            return Poll::Ready(Ok(buf.len()));
        }
        if this.prefix_remaining > 0 {
            let len = buf.len().min(this.prefix_remaining);
            this.bytes.extend_from_slice(&buf[..len]);
            this.prefix_remaining -= len;
            return Poll::Ready(Ok(len));
        }
        match this.failure {
            WriteFailure::Stall => Poll::Pending,
            WriteFailure::Zero => {
                this.is_failed = true;
                Poll::Ready(Ok(0))
            }
            _ => {
                this.is_failed = true;
                Poll::Ready(Err(io::Error::other("injected transfer write failure")))
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if this.flushes_to_skip == 0 && !this.is_failed {
            match this.failure {
                WriteFailure::FlushError => {
                    this.is_failed = true;
                    return Poll::Ready(Err(io::Error::other("injected transfer flush failure")));
                }
                WriteFailure::FlushStall => return Poll::Pending,
                _ => {}
            }
        }
        this.flushes_to_skip = this.flushes_to_skip.saturating_sub(1);
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}
