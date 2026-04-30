//! DNS resolver abstraction for the address-validation step.
//!
//! `validate_and_authenticate` calls `Resolver::lookup` to resolve a
//! registrant-supplied hostname to a set of `IpAddr`s for comparison
//! against the peer's source IP. Production wires in [`TokioResolver`],
//! which delegates to `tokio::net::lookup_host`. Tests substitute a
//! mock implementation via [`crate::state::TrackerState::with_resolver`]
//! so DNS-failure paths and resolved-address mismatches can be exercised
//! deterministically without depending on the host's resolver state.
//!
//! Errors are surfaced as `io::Error`. The handler differentiates
//! `io::ErrorKind::NotFound` (and an empty-result `Ok`) — both treated
//! as definitive NXDOMAIN-style rejections — from any other error or a
//! lookup timeout, which the handler treats as a transient resolver
//! failure. Transient outcomes are mode-asymmetric: initial register
//! hard-rejects (so an unverified entry can't slip in during a DNS
//! blip), refresh soft-passes (so an established entry isn't evicted
//! by the same blip).

use std::io;
use std::net::IpAddr;

use async_trait::async_trait;

/// Async DNS resolver.
#[async_trait]
pub trait Resolver: Send + Sync {
    /// Resolve `host` to zero or more `IpAddr`s. The host is the
    /// already-Punycoded ASCII form; implementations should not perform
    /// IDN conversion themselves.
    async fn lookup(&self, host: &str) -> io::Result<Vec<IpAddr>>;
}

/// Production resolver backed by `tokio::net::lookup_host`.
///
/// `lookup_host` requires a port to satisfy its `(host, port)` parsing,
/// so we pass a sentinel `0` and discard the port from the resulting
/// `SocketAddr`s — only the IP portion is meaningful for our match.
pub struct TokioResolver;

#[async_trait]
impl Resolver for TokioResolver {
    async fn lookup(&self, host: &str) -> io::Result<Vec<IpAddr>> {
        let iter = tokio::net::lookup_host((host, 0)).await?;
        Ok(iter.map(|sa| sa.ip()).collect())
    }
}
