//! Protocol version handling with semantic versioning
//!
//! This module provides semver compatibility checking for the Nexus protocol
//! handshake. It uses the `semver` crate for parsing and determines whether
//! a client version is compatible with the server version.

pub use semver::Version;

/// Result of checking version compatibility
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompatibilityResult {
    /// Versions are compatible
    Compatible,
    /// Major version mismatch
    MajorMismatch {
        server_major: u64,
        client_major: u64,
    },
    /// Client minor version is newer than server
    ClientTooNew {
        server_minor: u64,
        client_minor: u64,
    },
}

impl CompatibilityResult {
    /// Returns true if the result indicates compatibility
    #[must_use]
    pub fn is_compatible(&self) -> bool {
        matches!(self, CompatibilityResult::Compatible)
    }
}

/// Parse the protocol version constant
///
/// # Panics
///
/// Panics if `PROTOCOL_VERSION` is not valid semver. This should never
/// happen as the constant is defined in this crate.
#[must_use]
pub fn protocol_version() -> Version {
    crate::PROTOCOL_VERSION
        .parse()
        .expect("PROTOCOL_VERSION must be valid semver")
}

/// Check if a client version is compatible with the server's protocol version.
///
/// Compatibility rules:
/// - Major versions must match (breaking changes)
/// - Client minor version must be ≤ server minor version
///   (server can have features client doesn't know about)
/// - Patch versions do not affect compatibility
/// - Pre-release versions are compared based on their base version
///
/// # Returns
///
/// A `CompatibilityResult` indicating whether the versions are compatible
/// and, if not, why.
#[must_use]
pub fn check_compatibility(client: &Version) -> CompatibilityResult {
    let server = protocol_version();

    if server.major != client.major {
        return CompatibilityResult::MajorMismatch {
            server_major: server.major,
            client_major: client.major,
        };
    }

    if client.minor > server.minor {
        return CompatibilityResult::ClientTooNew {
            server_minor: server.minor,
            client_minor: client.minor,
        };
    }

    CompatibilityResult::Compatible
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_version_parses() {
        // This should not panic
        let v = protocol_version();
        // And should match the constant
        assert_eq!(v.to_string(), crate::PROTOCOL_VERSION);
    }

    #[test]
    fn test_compatibility_same_version() {
        let result = check_compatibility(&protocol_version());
        assert!(result.is_compatible());
        assert_eq!(result, CompatibilityResult::Compatible);
    }

    #[test]
    fn test_compatibility_older_client_minor() {
        let server = protocol_version();
        if server.minor > 0 {
            let client = Version::new(server.major, server.minor - 1, 0);
            assert!(check_compatibility(&client).is_compatible());
        }
    }

    #[test]
    fn test_compatibility_different_patch() {
        let server = protocol_version();

        // Client with different patch
        let client = Version::new(server.major, server.minor, server.patch + 5);
        assert!(check_compatibility(&client).is_compatible());
    }

    #[test]
    fn test_incompatibility_major_mismatch() {
        let server = protocol_version();

        // Client major too high
        let client = Version::new(server.major + 1, 0, 0);
        let result = check_compatibility(&client);
        assert!(!result.is_compatible());
        assert!(matches!(result, CompatibilityResult::MajorMismatch { .. }));
    }

    #[test]
    fn test_incompatibility_client_minor_too_new() {
        let server = protocol_version();
        let client = Version::new(server.major, server.minor + 1, 0);
        let result = check_compatibility(&client);
        assert!(!result.is_compatible());
        assert!(matches!(result, CompatibilityResult::ClientTooNew { .. }));
    }

    #[test]
    fn test_compatibility_result_is_compatible() {
        assert!(CompatibilityResult::Compatible.is_compatible());
        assert!(
            !CompatibilityResult::MajorMismatch {
                server_major: 1,
                client_major: 2
            }
            .is_compatible()
        );
        assert!(
            !CompatibilityResult::ClientTooNew {
                server_minor: 1,
                client_minor: 2
            }
            .is_compatible()
        );
    }

    #[test]
    fn test_prerelease_versions() {
        let server = protocol_version();
        let client: Version = format!("{}.{}.{}-alpha", server.major, server.minor, server.patch)
            .parse()
            .unwrap();
        assert!(check_compatibility(&client).is_compatible());
    }
}
