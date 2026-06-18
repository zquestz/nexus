//! Client-side TLS certificate fingerprint pin helpers.
//!
//! Shared by every config entity that stores a TOFU fingerprint pin:
//! [`super::bookmark::ServerBookmark`] (BBS server pin) and
//! [`super::tracker::ClientTracker`] (tracker pin). Centralising the helpers
//! here keeps the storage invariant — and the security reasoning behind it —
//! in one place so the two pin sites can't drift.

use serde::{Deserialize, Deserializer};

/// Normalize a certificate fingerprint pin: an empty string collapses to `None`
/// (unpinned). Apply at every assignment site for any field that holds a
/// fingerprint pin so the in-memory and on-disk shape stays consistent — Stage 1
/// TOFU never has to disambiguate "empty string" from "no pin".
pub fn normalize_certificate_fingerprint(fp: Option<String>) -> Option<String> {
    fp.filter(|s| !s.is_empty())
}

/// Deserializer that runs `Option<String>` through
/// [`normalize_certificate_fingerprint`] so a hand-edited `config.json`
/// containing `"certificate_fingerprint": ""` loads as `None` rather than
/// `Some("")` — which would otherwise reach Stage 1 and produce a confusing
/// mismatch dialog with `expected = ""`.
pub fn deserialize_normalized_fingerprint<'de, D>(
    deserializer: D,
) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<String>::deserialize(deserializer)?;
    Ok(normalize_certificate_fingerprint(opt))
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_none() {
        assert_eq!(normalize_certificate_fingerprint(None), None);
    }

    #[test]
    fn test_normalize_empty_string() {
        assert_eq!(normalize_certificate_fingerprint(Some(String::new())), None);
    }

    #[test]
    fn test_normalize_preserves_whitespace_only() {
        // Only an empty string collapses to None.
        assert_eq!(
            normalize_certificate_fingerprint(Some("   ".to_string())),
            Some("   ".to_string())
        );
    }

    #[test]
    fn test_normalize_preserves_surrounding_whitespace() {
        assert_eq!(
            normalize_certificate_fingerprint(Some("  AA:BB  ".to_string())),
            Some("  AA:BB  ".to_string())
        );
    }

    #[test]
    fn test_normalize_passes_through_canonical() {
        let canonical = "AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99";
        assert_eq!(
            normalize_certificate_fingerprint(Some(canonical.to_string())),
            Some(canonical.to_string())
        );
    }
}
