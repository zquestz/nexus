//! DNS resolver abstraction for the address-validation step. `Resolver::lookup` resolves a
//! registrant hostname for comparison against the peer's source IP. Production uses
//! [`TokioResolver`]; tests inject a mock via `TrackerState::with_resolver`.
//!
//! The handler treats NotFound / empty as definitive rejection and any other error / timeout as
//! transient. Transient is mode-asymmetric: initial register hard-rejects (no unverified entry
//! during a DNS blip), refresh soft-passes (don't evict an established entry over the same blip).

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

/// Production resolver over `tokio::net::lookup_host`. Passes a sentinel port `0` (required by
/// its `(host, port)` parsing) and discards it — only the IP matters here.
pub struct TokioResolver;

#[async_trait]
impl Resolver for TokioResolver {
    async fn lookup(&self, host: &str) -> io::Result<Vec<IpAddr>> {
        let iter = tokio::net::lookup_host((host, 0)).await?;
        Ok(iter.map(|sa| sa.ip()).collect())
    }
}
