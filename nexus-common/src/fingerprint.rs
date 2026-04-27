//! Certificate fingerprint formatting.
//!
//! Single source of truth for SHA-256 cert fingerprint shape used across
//! the workspace: 32 hex pairs, uppercase, colon-separated (95 chars).
//! Both client and server format fingerprints via this function so any
//! comparison can use plain `==` without normalization.

use sha2::{Digest, Sha256};

/// Compute the SHA-256 fingerprint of certificate DER bytes and format it
/// as a colon-separated uppercase hex string (e.g., "AA:BB:CC:..." — 95 chars).
pub fn format_certificate_fingerprint(cert_bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(cert_bytes);
    let digest = hasher.finalize();
    let hex_str = hex::encode_upper(digest);

    // Build the colon-separated form directly into the result, avoiding the
    // intermediate `Vec<&str>` collect. 32 hex pairs + 31 colons = 95 chars.
    let mut out = String::with_capacity(95);
    for (i, chunk) in hex_str.as_bytes().chunks(2).enumerate() {
        if i > 0 {
            out.push(':');
        }
        out.push_str(std::str::from_utf8(chunk).expect("hex encoding produces valid ASCII"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_shape() {
        let fp = format_certificate_fingerprint(b"test");
        assert_eq!(fp.len(), 95, "fingerprint must be 95 chars");
        assert_eq!(fp.matches(':').count(), 31, "must have 31 colons");
        assert!(
            fp.chars().all(|c| c == ':' || c.is_ascii_hexdigit()),
            "must be hex chars and colons only"
        );
        assert_eq!(fp, fp.to_uppercase(), "must be uppercase");
    }

    #[test]
    fn test_deterministic() {
        let a = format_certificate_fingerprint(b"some cert bytes");
        let b = format_certificate_fingerprint(b"some cert bytes");
        assert_eq!(a, b);
    }

    #[test]
    fn test_known_value() {
        // SHA-256 of empty input — well-known constant.
        let fp = format_certificate_fingerprint(b"");
        assert_eq!(
            fp,
            "E3:B0:C4:42:98:FC:1C:14:9A:FB:F4:C8:99:6F:B9:24:27:AE:41:E4:64:9B:93:4C:A4:95:99:1B:78:52:B8:55"
        );
    }
}
