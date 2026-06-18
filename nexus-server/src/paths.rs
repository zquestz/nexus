//! Test-safe wrapper around the `dirs::*` base-directory lookup.
//!
//! The daemon resolves its data directory through here. Under `#[cfg(test)]`
//! it redirects to a throwaway per-user temp root, so a test that resolves the
//! *default* data directory can never read from or write to the operator's real
//! `~/.local/share/nexusd`. Production code injects the resolved `&Path`
//! everywhere downstream, so only the startup resolution touches this.

use std::path::PathBuf;

#[cfg(not(test))]
pub(crate) fn data_dir() -> Option<PathBuf> {
    dirs::data_dir()
}

#[cfg(test)]
pub(crate) fn data_dir() -> Option<PathBuf> {
    Some(test_root().join("data"))
}

/// A fixed per-user temp root, reused (overwritten) across runs so it never
/// accumulates. Per-user so concurrent users on a shared machine don't collide.
#[cfg(test)]
fn test_root() -> PathBuf {
    static ROOT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    ROOT.get_or_init(|| {
        let user = std::env::var("USER").unwrap_or_else(|_| "shared".to_string());
        std::env::temp_dir().join(format!("nexus-test-{user}"))
    })
    .clone()
}
