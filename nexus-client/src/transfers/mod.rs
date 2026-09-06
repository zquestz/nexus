//! File transfer management
//!
//! This module handles file transfers (downloads and uploads) which operate on a separate
//! port (7501) from the main BBS protocol. Transfers are persisted to disk to support
//! resume across application restarts.
//!
//! Key types:
//! - `Transfer` - A single file or directory transfer
//! - `TransferManager` - Manages all transfers and persistence
//! - `TransferEvent` - Progress events from the executor

mod executor;
mod persistence;
mod subscription;
mod types;

use std::path::{Component, Path};

pub use executor::TransferEvent;
pub use persistence::TransferManager;
pub use subscription::{request_cancel, transfer_subscription, update_registry_fingerprint};
pub use types::{Transfer, TransferDirection, TransferStatus};

/// A download name must remain one normal component on the current platform.
pub(crate) fn is_safe_download_name(name: &str) -> bool {
    let mut components = Path::new(name).components();
    // Match the original name too: components() normalizes "name/" and "name/.".
    if !matches!(components.next(), Some(Component::Normal(component)) if component == name)
        || components.next().is_some()
        || name.contains('\0')
    {
        return false;
    }

    // These characters and aliases are unsafe on Windows, but ordinary names on Unix.
    if cfg!(windows)
        && (name
            .chars()
            .any(|c| c < ' ' || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
            || name.ends_with([' ', '.'])
            || is_windows_reserved_name(name))
    {
        return false;
    }

    true
}

/// Recognize Windows device names independently of the host platform.
/// Filename validation gates this on Windows; generated root labels use it everywhere.
pub(crate) fn is_windows_reserved_name(component: &str) -> bool {
    // Extensions and trailing spaces/dots do not make a device name safe.
    let normalized_component = component.trim_end_matches([' ', '.']);
    let basename = normalized_component
        .split('.')
        .next()
        .unwrap_or(normalized_component)
        .trim_end_matches([' ', '.']);
    let upper = basename.to_ascii_uppercase();
    matches!(
        upper.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "COM\u{b9}"
            | "COM\u{b2}"
            | "COM\u{b3}"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
            | "LPT\u{b9}"
            | "LPT\u{b2}"
            | "LPT\u{b3}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_download_names_preserve_normal_and_unicode_names() {
        for name in [
            "report.txt",
            "My Documents",
            ".hidden",
            "...notes",
            " leading space.txt",
            "Uploads [NEXUS-UL]",
            "COM10.txt",
            "CONSOLE",
            "file\u{85}.txt",
            "\u{6587}\u{4ef6}.txt",
            "donn\u{e9}es.zip",
        ] {
            assert!(is_safe_download_name(name), "{name:?}");
        }
    }

    #[test]
    fn test_unsafe_download_names_are_rejected_on_every_platform() {
        for name in [
            "",
            ".",
            "..",
            "dir/file",
            "dir/",
            "dir/.",
            "/absolute",
            "file\0.txt",
        ] {
            assert!(!is_safe_download_name(name), "{name:?}");
        }
    }

    #[test]
    fn test_windows_filename_restrictions_only_apply_on_windows() {
        for name in [
            "...",
            ".. ",
            "file.",
            "file ",
            "dir\\file",
            "\\rooted",
            "C:",
            "C:file",
            "C:\\file",
            "\\\\server\\share",
            "\\\\?\\C:\\file",
            "file:stream",
            "file\n.txt",
            "file\t.txt",
            "file<name",
            "file>name",
            "file\"name",
            "file|name",
            "file?name",
            "file*name",
            "CON",
            "con.txt",
            "CON .txt",
            "NUL.log",
            "PRN",
            "AUX",
            "COM1",
            "LPT9.tar.gz",
            "COM\u{b9}.txt",
            "LPT\u{b2}",
            "LPT\u{b3}.log",
        ] {
            assert_eq!(is_safe_download_name(name), !cfg!(windows), "{name:?}");
        }
    }

    #[test]
    fn windows_reserved_names_include_aliases_and_extensions() {
        for name in [
            "CON",
            "PRN",
            "AUX",
            "NUL",
            "COM1",
            "COM2",
            "COM3",
            "COM4",
            "COM5",
            "COM6",
            "COM7",
            "COM8",
            "COM9",
            "COM\u{b9}",
            "COM\u{b2}",
            "COM\u{b3}",
            "LPT1",
            "LPT2",
            "LPT3",
            "LPT4",
            "LPT5",
            "LPT6",
            "LPT7",
            "LPT8",
            "LPT9",
            "LPT\u{b9}",
            "LPT\u{b2}",
            "LPT\u{b3}",
        ] {
            for suffix in ["", ".", " ", ".txt", ".tar.gz", " .txt", ". . "] {
                let component = format!("{name}{suffix}");
                assert!(is_windows_reserved_name(&component), "{component:?}");
                assert!(is_windows_reserved_name(&component.to_ascii_lowercase()));
            }
        }
    }

    #[test]
    fn ordinary_names_are_not_windows_devices() {
        for name in [
            "report.txt",
            "CONSOLE.txt",
            "NULLED",
            "COM0",
            "COM10.txt",
            "LPT0",
            "LPT10.log",
            "_CON.txt",
            ".CON",
            "album\u{b2}",
        ] {
            assert!(!is_windows_reserved_name(name), "{name:?}");
        }
    }
}
