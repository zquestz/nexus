//! Test-safe wrapper around the `dirs::*` base-directory lookup.
//!
//! The daemon resolves its data directory through here. Under `#[cfg(test)]`
//! it redirects to a throwaway per-process temp root, so a test that resolves the
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

/// A fixed temp root for this test process.
///
/// The process id prevents stale files from one test run influencing another.
/// `OnceLock` keeps the base stable within the process, so parallel tests still
/// agree on the same redirected default.
#[cfg(test)]
fn test_root() -> PathBuf {
    static ROOT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    ROOT.get_or_init(|| {
        let user = std::env::var("USER").unwrap_or_else(|_| "shared".to_string());
        let pid = std::process::id();
        std::env::temp_dir().join(format!("nexus-test-{user}-{pid}"))
    })
    .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_dir_redirects_under_temp_in_tests() {
        let temp = std::env::temp_dir();
        let root = test_root();
        assert!(
            root.starts_with(&temp),
            "base dir must redirect under temp in tests, got {root:?}"
        );
        assert_eq!(data_dir(), Some(root.join("data")));
    }
}
