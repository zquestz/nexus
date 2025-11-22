//! Nexus Common Library
//!
//! Shared types, protocols, and utilities for the Nexus BBS system.

pub mod protocol;
pub mod yggdrasil;

/// Version information for the Nexus protocol
pub const PROTOCOL_VERSION: &str = "0.1.0";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_version() {
        assert!(!PROTOCOL_VERSION.is_empty());
    }
}
