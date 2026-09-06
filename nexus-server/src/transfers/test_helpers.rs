//! Shared test helpers for the transfers module.

use std::collections::HashSet;
use std::future::{Future, poll_fn};
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll, ready};
use std::time::Duration;

use tokio::io::AsyncWrite;
use tokio::sync::oneshot;

use super::types::AuthenticatedUser;
use crate::db::Permission;

/// Limit each poll to one cooperative completion, so even cached file reads yield.
/// Delay those polls only after the initial response frames have been flushed.
pub(crate) async fn with_slow_polls<F: Future>(
    future: F,
    delay_enabled: Arc<AtomicBool>,
) -> F::Output {
    tokio::pin!(future);
    let mut delay = None;
    poll_fn(|cx| {
        if delay_enabled.load(Ordering::Relaxed) {
            ready!(
                delay
                    .get_or_insert_with(|| Box::pin(tokio::time::sleep(
                        nexus_common::KEEPALIVE_INTERVAL + Duration::from_secs(1),
                    )))
                    .as_mut()
                    .poll(cx)
            );
        }
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
pub(crate) enum WriteFailure {
    Error,
    Zero,
    Stall,
    FlushError,
    FlushStall,
}

/// Fail after a prefix of a chosen frame. Non-stalling failures recover so a
/// caller that incorrectly sends another frame leaves observable extra bytes.
pub(crate) struct FailingWriter<W> {
    pub(crate) inner: W,
    pub(crate) shutdown: bool,
    pub(crate) failed: bool,
    pub(crate) failure_armed: Arc<AtomicBool>,
    flushes_to_skip: usize,
    prefix_remaining: usize,
    failure: WriteFailure,
}

impl<W> FailingWriter<W> {
    pub(crate) fn new(
        inner: W,
        flushes_to_skip: usize,
        prefix_len: usize,
        failure: WriteFailure,
    ) -> Self {
        Self {
            inner,
            shutdown: false,
            failed: false,
            failure_armed: Arc::new(AtomicBool::new(flushes_to_skip == 0)),
            flushes_to_skip,
            prefix_remaining: prefix_len,
            failure,
        }
    }
}

impl<W: AsyncWrite + Unpin> AsyncWrite for FailingWriter<W> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        if this.flushes_to_skip > 0 || this.failed {
            return Pin::new(&mut this.inner).poll_write(cx, buf);
        }
        if matches!(
            this.failure,
            WriteFailure::FlushError | WriteFailure::FlushStall
        ) {
            return Pin::new(&mut this.inner).poll_write(cx, buf);
        }
        if this.prefix_remaining > 0 {
            let len = buf.len().min(this.prefix_remaining);
            let written = ready!(Pin::new(&mut this.inner).poll_write(cx, &buf[..len]))?;
            this.prefix_remaining -= written;
            return Poll::Ready(Ok(written));
        }
        match this.failure {
            WriteFailure::Stall => Poll::Pending,
            WriteFailure::Zero => {
                this.failed = true;
                Poll::Ready(Ok(0))
            }
            _ => {
                this.failed = true;
                Poll::Ready(Err(io::Error::other("injected write failure")))
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if this.flushes_to_skip == 0 && !this.failed {
            match this.failure {
                WriteFailure::FlushError => {
                    this.failed = true;
                    return Poll::Ready(Err(io::Error::other("injected flush failure")));
                }
                WriteFailure::FlushStall => return Poll::Pending,
                _ => {}
            }
        }
        ready!(Pin::new(&mut this.inner).poll_flush(cx))?;
        this.flushes_to_skip = this.flushes_to_skip.saturating_sub(1);
        this.failure_armed
            .store(this.flushes_to_skip == 0, Ordering::Relaxed);
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        this.shutdown = true;
        Pin::new(&mut this.inner).poll_shutdown(cx)
    }
}

/// Pause once after flushing bytes, before the transfer can acknowledge its chunk.
pub(crate) struct PausedFlushWriter<W> {
    pub(crate) inner: W,
    pub(crate) pause_next_flush: Arc<AtomicBool>,
    pub(crate) shutdown: bool,
    flushed_tx: Option<oneshot::Sender<()>>,
    resume_rx: oneshot::Receiver<()>,
    is_paused: bool,
}

impl<W> PausedFlushWriter<W> {
    pub(crate) fn new(inner: W) -> (Self, oneshot::Receiver<()>, oneshot::Sender<()>) {
        let (flushed_tx, flushed_rx) = oneshot::channel();
        let (resume_tx, resume_rx) = oneshot::channel();
        (
            Self {
                inner,
                pause_next_flush: Arc::new(AtomicBool::new(true)),
                shutdown: false,
                flushed_tx: Some(flushed_tx),
                resume_rx,
                is_paused: false,
            },
            flushed_rx,
            resume_tx,
        )
    }
}

impl<W: AsyncWrite + Unpin> AsyncWrite for PausedFlushWriter<W> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.get_mut().inner).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if !this.is_paused {
            ready!(Pin::new(&mut this.inner).poll_flush(cx))?;
            if this.flushed_tx.is_none() || !this.pause_next_flush.swap(false, Ordering::Relaxed) {
                return Poll::Ready(Ok(()));
            }
            this.is_paused = true;
            this.flushed_tx.take().unwrap().send(()).unwrap();
        }
        ready!(Pin::new(&mut this.resume_rx).poll(cx)).unwrap();
        this.is_paused = false;
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        this.shutdown = true;
        Pin::new(&mut this.inner).poll_shutdown(cx)
    }
}

/// Build an `AuthenticatedUser` with the given admin flag and permission set.
pub(crate) fn make_authenticated_user(is_admin: bool, perms: &[Permission]) -> AuthenticatedUser {
    let mut permissions = HashSet::new();
    for p in perms {
        permissions.insert(*p);
    }
    AuthenticatedUser {
        user_id: 1,
        nickname: "tester".to_string(),
        username: "tester".to_string(),
        is_admin,
        is_shared: false,
        permissions,
    }
}
