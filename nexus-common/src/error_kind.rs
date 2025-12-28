//! Machine-readable error kinds for file operations
//!
//! These error kinds are serialized to strings in protocol messages,
//! allowing clients to make decisions based on the error type
//! (e.g., showing an overwrite dialog for "exists" errors).

use std::fmt;

/// Error kinds for file move/copy operations
///
/// These are returned in `FileMoveResponse` and `FileCopyResponse`
/// to help clients decide how to handle the error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileErrorKind {
    /// Destination file/directory already exists
    ///
    /// Client may offer to retry with `overwrite: true`
    /// (if user has `file_delete` permission).
    Exists,

    /// Source file/directory not found
    ///
    /// Client should clear clipboard if this was a move/copy operation.
    NotFound,

    /// Permission denied
    ///
    /// User lacks required permission for the operation.
    Permission,

    /// Invalid path
    ///
    /// Path contains invalid characters, traversal attempts,
    /// or the operation is not allowed (e.g., copying file to itself).
    InvalidPath,
}

impl FileErrorKind {
    /// Convert to the string representation used in protocol messages
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Exists => "exists",
            Self::NotFound => "not_found",
            Self::Permission => "permission",
            Self::InvalidPath => "invalid_path",
        }
    }

    /// Parse from string (for client-side handling)
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "exists" => Some(Self::Exists),
            "not_found" => Some(Self::NotFound),
            "permission" => Some(Self::Permission),
            "invalid_path" => Some(Self::InvalidPath),
            _ => None,
        }
    }
}

impl fmt::Display for FileErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<FileErrorKind> for String {
    fn from(kind: FileErrorKind) -> Self {
        kind.as_str().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_as_str() {
        assert_eq!(FileErrorKind::Exists.as_str(), "exists");
        assert_eq!(FileErrorKind::NotFound.as_str(), "not_found");
        assert_eq!(FileErrorKind::Permission.as_str(), "permission");
        assert_eq!(FileErrorKind::InvalidPath.as_str(), "invalid_path");
    }

    #[test]
    fn test_parse() {
        assert_eq!(FileErrorKind::parse("exists"), Some(FileErrorKind::Exists));
        assert_eq!(
            FileErrorKind::parse("not_found"),
            Some(FileErrorKind::NotFound)
        );
        assert_eq!(
            FileErrorKind::parse("permission"),
            Some(FileErrorKind::Permission)
        );
        assert_eq!(
            FileErrorKind::parse("invalid_path"),
            Some(FileErrorKind::InvalidPath)
        );
        assert_eq!(FileErrorKind::parse("unknown"), None);
        assert_eq!(FileErrorKind::parse(""), None);
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", FileErrorKind::Exists), "exists");
        assert_eq!(format!("{}", FileErrorKind::NotFound), "not_found");
    }

    #[test]
    fn test_into_string() {
        let s: String = FileErrorKind::Exists.into();
        assert_eq!(s, "exists");
    }

    #[test]
    fn test_roundtrip() {
        for kind in [
            FileErrorKind::Exists,
            FileErrorKind::NotFound,
            FileErrorKind::Permission,
            FileErrorKind::InvalidPath,
        ] {
            assert_eq!(FileErrorKind::parse(kind.as_str()), Some(kind));
        }
    }
}
