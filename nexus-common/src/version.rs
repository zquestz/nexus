//! Protocol version handling with semantic versioning
//!
//! This module provides semver compatibility checking for the Nexus protocol
//! handshake. It uses the `semver` crate for parsing and determines whether
//! a client version is compatible with the server version.

pub use semver::Version;

/// Panic message: [`crate::PROTOCOL_VERSION`] failed to parse as semver.
/// Programmer-error: the constant is hand-edited at release time and
/// should always be valid.
const ERR_PROTOCOL_VERSION_INVALID: &str = "PROTOCOL_VERSION must be valid semver";

/// Panic message: [`crate::TRACKER_PROTOCOL_VERSION`] failed to parse
/// as semver. Same reasoning as `ERR_PROTOCOL_VERSION_INVALID`.
const ERR_TRACKER_PROTOCOL_VERSION_INVALID: &str =
    "TRACKER_PROTOCOL_VERSION must be valid semver";

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
    /// Minor version mismatch during pre-1.0 development (0.x)
    MinorMismatch {
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

/// Parse the BBS protocol version constant.
///
/// # Panics
///
/// Panics if `PROTOCOL_VERSION` is not valid semver. This should never
/// happen as the constant is defined in this crate.
#[must_use]
pub fn protocol_version() -> Version {
    crate::PROTOCOL_VERSION
        .parse()
        .expect(ERR_PROTOCOL_VERSION_INVALID)
}

/// Parse the tracker protocol version constant.
///
/// Independent from the BBS protocol version — see
/// [`crate::TRACKER_PROTOCOL_VERSION`].
///
/// # Panics
///
/// Panics if `TRACKER_PROTOCOL_VERSION` is not valid semver. This should
/// never happen as the constant is defined in this crate.
#[must_use]
pub fn tracker_protocol_version() -> Version {
    crate::TRACKER_PROTOCOL_VERSION
        .parse()
        .expect(ERR_TRACKER_PROTOCOL_VERSION_INVALID)
}

/// Check whether `client`'s protocol version is compatible with `server`'s.
///
/// The rules apply identically to either protocol (BBS or tracker); the
/// caller passes the appropriate server version.
///
/// Compatibility rules:
/// - Major versions must match
/// - **Pre-1.0 (`0.x`):** Minor versions must match exactly. Each minor bump
///   can be a breaking change per semver (e.g., `0.5` and `0.7` are incompatible)
/// - **Post-1.0 (`1.x+`):** Client minor version must be ≤ server minor version
///   (server can have features client doesn't know about)
/// - Patch versions do not affect compatibility
/// - Pre-release versions are compared based on their base version
///
/// # Returns
///
/// A `CompatibilityResult` indicating whether the versions are compatible
/// and, if not, why.
#[must_use]
pub fn check_compatibility(server: &Version, client: &Version) -> CompatibilityResult {
    if server.major != client.major {
        return CompatibilityResult::MajorMismatch {
            server_major: server.major,
            client_major: client.major,
        };
    }

    // Pre-1.0: minor versions must match exactly (each minor bump can break)
    if server.major == 0 && client.minor != server.minor {
        return CompatibilityResult::MinorMismatch {
            server_minor: server.minor,
            client_minor: client.minor,
        };
    }

    // Post-1.0: client minor must be ≤ server minor
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
        let server = protocol_version();
        let result = check_compatibility(&server, &server);
        assert!(result.is_compatible());
        assert_eq!(result, CompatibilityResult::Compatible);
    }

    #[test]
    fn test_pre_1_0_rejects_older_client_minor() {
        let server = protocol_version();
        // During 0.x, older minor versions are incompatible
        if server.major == 0 && server.minor > 0 {
            let client = Version::new(0, server.minor - 1, 0);
            let result = check_compatibility(&server, &client);
            assert!(!result.is_compatible());
            assert!(matches!(result, CompatibilityResult::MinorMismatch { .. }));
        }
    }

    #[test]
    fn test_pre_1_0_rejects_newer_client_minor() {
        let server = protocol_version();
        // During 0.x, newer minor versions are also incompatible
        if server.major == 0 {
            let client = Version::new(0, server.minor + 1, 0);
            let result = check_compatibility(&server, &client);
            assert!(!result.is_compatible());
            assert!(matches!(result, CompatibilityResult::MinorMismatch { .. }));
        }
    }

    #[test]
    fn test_compatibility_different_patch() {
        let server = protocol_version();

        // Client with different patch
        let client = Version::new(server.major, server.minor, server.patch + 5);
        assert!(check_compatibility(&server, &client).is_compatible());
    }

    #[test]
    fn test_incompatibility_major_mismatch() {
        let server = protocol_version();

        // Client major too high
        let client = Version::new(server.major + 1, 0, 0);
        let result = check_compatibility(&server, &client);
        assert!(!result.is_compatible());
        assert!(matches!(result, CompatibilityResult::MajorMismatch { .. }));
    }

    #[test]
    fn test_incompatibility_client_minor_too_new() {
        // This tests the post-1.0 path; for 0.x, MinorMismatch fires first
        let server = protocol_version();
        let client = Version::new(server.major, server.minor + 1, 0);
        let result = check_compatibility(&server, &client);
        assert!(!result.is_compatible());
        if server.major == 0 {
            assert!(matches!(result, CompatibilityResult::MinorMismatch { .. }));
        } else {
            assert!(matches!(result, CompatibilityResult::ClientTooNew { .. }));
        }
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
        assert!(
            !CompatibilityResult::MinorMismatch {
                server_minor: 7,
                client_minor: 5
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
        assert!(check_compatibility(&server, &client).is_compatible());
    }

    #[test]
    fn test_tracker_protocol_version_parses() {
        let v = tracker_protocol_version();
        assert_eq!(v.to_string(), crate::TRACKER_PROTOCOL_VERSION);
    }

    #[test]
    fn test_check_compatibility_independent_of_server_choice() {
        // The same client+server pair should produce the same result
        // regardless of whether the server happens to be BBS or tracker —
        // proving the function is fully parameterized.
        let server_a = Version::new(0, 8, 0);
        let server_b = Version::new(0, 1, 0);
        let client = Version::new(0, 8, 0);

        let result_a = check_compatibility(&server_a, &client);
        let result_b = check_compatibility(&server_b, &client);

        assert!(result_a.is_compatible());
        assert!(matches!(
            result_b,
            CompatibilityResult::MinorMismatch { .. }
        ));
    }
}
