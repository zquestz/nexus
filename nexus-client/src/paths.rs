//! Test-safe wrappers around the `dirs::*` base-directory lookups.
//!
//! Every persisted client file resolves its base directory through here
//! (`config.json`, `transfers.json`, chat history, downloads). Under
//! `#[cfg(test)]` these redirect to a throwaway per-user temp root, so no test
//! can read from or write to the user's real config / data / download
//! directories — isolating the *directory* protects every file in it, present
//! and future, rather than patching each path function individually.

use std::path::PathBuf;

#[cfg(not(test))]
pub(crate) fn config_dir() -> Option<PathBuf> {
    dirs::config_dir()
}

#[cfg(not(test))]
pub(crate) fn data_dir() -> Option<PathBuf> {
    dirs::data_dir()
}

#[cfg(not(test))]
pub(crate) fn download_dir() -> Option<PathBuf> {
    dirs::download_dir()
}

#[cfg(test)]
pub(crate) fn config_dir() -> Option<PathBuf> {
    Some(test_root().join("config"))
}

#[cfg(test)]
pub(crate) fn data_dir() -> Option<PathBuf> {
    Some(test_root().join("data"))
}

#[cfg(test)]
pub(crate) fn download_dir() -> Option<PathBuf> {
    Some(test_root().join("downloads"))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_dirs_redirect_under_temp_in_tests() {
        let temp = std::env::temp_dir();
        for dir in [config_dir(), data_dir(), download_dir()] {
            let dir = dir.expect("wrapper returns Some under cfg(test)");
            assert!(
                dir.starts_with(&temp),
                "base dir must redirect under temp in tests, got {dir:?}"
            );
        }
    }
}
