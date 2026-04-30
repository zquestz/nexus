//! Certificate fingerprint formatting.
//!
//! Single source of truth for SHA-256 cert fingerprint shape used across
//! the workspace: 32 hex pairs, uppercase, colon-separated (95 chars).
//! Both client and server format fingerprints via this function so any
//! comparison can use plain `==` without normalization.

use sha2::{Digest, Sha256};

/// Panic message: `hex::encode_upper` somehow produced non-ASCII bytes.
/// Programmer-error: hex encoding is guaranteed to produce ASCII; this
/// `.expect()` only exists because `from_utf8` returns `Result`.
const ERR_HEX_NOT_ASCII: &str = "hex encoding produces valid ASCII";

/// Validate that `fp` is the canonical SHA-256 fingerprint form: 32
/// uppercase hex pairs separated by colons, exactly 95 ASCII bytes.
/// Inverse of [`format_certificate_fingerprint`] — used at protocol
/// boundaries where peers send a fingerprint we need to verify before
/// accepting / comparing.
#[must_use]
pub fn is_canonical_fingerprint(fp: &str) -> bool {
    if fp.len() != 95 {
        return false;
    }
    for (i, &b) in fp.as_bytes().iter().enumerate() {
        if i % 3 == 2 {
            if b != b':' {
                return false;
            }
        } else if !(b.is_ascii_digit() || (b'A'..=b'F').contains(&b)) {
            return false;
        }
    }
    true
}

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
        out.push_str(std::str::from_utf8(chunk).expect(ERR_HEX_NOT_ASCII));
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

    #[test]
    fn test_is_canonical_accepts_workspace_format() {
        // Round-trip: every fingerprint we produce must be canonical.
        let fp = format_certificate_fingerprint(b"some cert bytes");
        assert!(is_canonical_fingerprint(&fp));
    }

    #[test]
    fn test_is_canonical_rejects_wrong_length() {
        assert!(!is_canonical_fingerprint(""));
        assert!(!is_canonical_fingerprint("AA:BB"));
        assert!(!is_canonical_fingerprint(&"A".repeat(94)));
        assert!(!is_canonical_fingerprint(&"A".repeat(96)));
    }

    #[test]
    fn test_is_canonical_rejects_lowercase_hex() {
        let lower = "aa:bb:cc:dd:ee:ff:00:11:22:33:44:55:66:77:88:99:\
                     aa:bb:cc:dd:ee:ff:00:11:22:33:44:55:66:77:88:99";
        assert_eq!(lower.len(), 95);
        assert!(!is_canonical_fingerprint(lower));
    }

    #[test]
    fn test_is_canonical_rejects_wrong_separator() {
        // 95 chars but uses dashes instead of colons.
        let dashed = "AA-BB-CC-DD-EE-FF-00-11-22-33-44-55-66-77-88-99-\
                      AA-BB-CC-DD-EE-FF-00-11-22-33-44-55-66-77-88-99";
        assert_eq!(dashed.len(), 95);
        assert!(!is_canonical_fingerprint(dashed));
    }

    #[test]
    fn test_is_canonical_rejects_non_hex_bytes() {
        // Replace one hex char with a 'G'.
        let bad = "AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:\
                   AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:9G";
        assert_eq!(bad.len(), 95);
        assert!(!is_canonical_fingerprint(bad));
    }
}
