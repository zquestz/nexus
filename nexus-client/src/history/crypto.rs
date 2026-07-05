//! Cryptographic operations for chat history obfuscation
//!
//! Uses ChaCha20-Poly1305 with keys derived via HKDF-SHA256 from the server's
//! certificate fingerprint.
//!
//! # Security Model
//!
//! **This is obfuscation, not security.** The key is derived from the server's
//! certificate fingerprint, which is public information (visible to anyone who
//! connects to the server). This prevents casual snooping but provides no
//! protection against attackers who know the fingerprint.

use crate::constants::ERR_HKDF_OUTPUT_LENGTH;
use chacha20poly1305::{
    ChaCha20Poly1305, KeyInit, Nonce,
    aead::{Aead, Generate},
};
use hkdf::Hkdf;
use sha2::Sha256;

/// Salt used for HKDF key derivation
const HKDF_SALT: &[u8] = b"nexus-history-v1";

/// Nonce size for ChaCha20-Poly1305 (96 bits / 12 bytes)
const NONCE_SIZE: usize = 12;

/// Handles encryption and decryption of chat history
pub struct HistoryCrypto {
    cipher: ChaCha20Poly1305,
}

impl HistoryCrypto {
    /// Create a new crypto instance from a certificate fingerprint
    ///
    /// The fingerprint should be the hex-encoded SHA-256 hash of the server's certificate.
    pub fn new(fingerprint: &str) -> Self {
        let key = Self::derive_key(fingerprint);
        let cipher = ChaCha20Poly1305::new(&key.into());
        Self { cipher }
    }

    /// Derive a 256-bit key from the fingerprint using HKDF-SHA256
    fn derive_key(fingerprint: &str) -> [u8; 32] {
        let hkdf = Hkdf::<Sha256>::new(Some(HKDF_SALT), fingerprint.as_bytes());
        let mut key = [0u8; 32];
        // info parameter is empty, unwrap is safe as 32 bytes is valid output length
        hkdf.expand(&[], &mut key).expect(ERR_HKDF_OUTPUT_LENGTH);
        key
    }

    /// Encrypt plaintext data
    ///
    /// Returns the nonce prepended to the ciphertext: `[nonce (12 bytes)][ciphertext]`
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        // Generate a random nonce from the system RNG via the aead stack's
        // `Generate` trait; an OS RNG failure surfaces as a normal
        // encryption error rather than a panic.
        let nonce = Nonce::try_generate().map_err(|_| CryptoError::EncryptionFailed)?;

        // Encrypt
        let ciphertext = self
            .cipher
            .encrypt(&nonce, plaintext)
            .map_err(|_| CryptoError::EncryptionFailed)?;

        // Prepend nonce to ciphertext
        let mut result = Vec::with_capacity(NONCE_SIZE + ciphertext.len());
        result.extend_from_slice(&nonce);
        result.extend_from_slice(&ciphertext);

        Ok(result)
    }

    /// Decrypt data that was encrypted with `encrypt`
    ///
    /// Expects the nonce to be prepended to the ciphertext: `[nonce (12 bytes)][ciphertext]`
    pub fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>, CryptoError> {
        if data.len() < NONCE_SIZE {
            return Err(CryptoError::InvalidData);
        }

        let (nonce_bytes, ciphertext) = data.split_at(NONCE_SIZE);
        // Length is guaranteed by the split above; the TryFrom is the
        // non-deprecated slice conversion, not a real failure path.
        let nonce = Nonce::try_from(nonce_bytes).map_err(|_| CryptoError::InvalidData)?;

        self.cipher
            .decrypt(&nonce, ciphertext)
            .map_err(|_| CryptoError::DecryptionFailed)
    }
}

/// Errors that can occur during cryptographic operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CryptoError {
    /// Encryption failed (should not happen with valid input)
    EncryptionFailed,
    /// Decryption failed (wrong key, corrupted data, or tampered ciphertext)
    DecryptionFailed,
    /// Data is too short to contain a valid nonce
    InvalidData,
}

impl std::fmt::Display for CryptoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CryptoError::EncryptionFailed => write!(f, "encryption failed"),
            CryptoError::DecryptionFailed => write!(f, "decryption failed"),
            CryptoError::InvalidData => write!(f, "invalid encrypted data"),
        }
    }
}

impl std::error::Error for CryptoError {}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_FINGERPRINT: &str =
        "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let crypto = HistoryCrypto::new(TEST_FINGERPRINT);
        let plaintext = b"Hello, World! This is a test message.";

        let encrypted = crypto.encrypt(plaintext).unwrap();
        let decrypted = crypto.decrypt(&encrypted).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_produces_different_ciphertexts() {
        let crypto = HistoryCrypto::new(TEST_FINGERPRINT);
        let plaintext = b"Same message";

        let encrypted1 = crypto.encrypt(plaintext).unwrap();
        let encrypted2 = crypto.encrypt(plaintext).unwrap();

        // Different nonces should produce different ciphertexts
        assert_ne!(encrypted1, encrypted2);

        // But both should decrypt to the same plaintext
        assert_eq!(crypto.decrypt(&encrypted1).unwrap(), plaintext);
        assert_eq!(crypto.decrypt(&encrypted2).unwrap(), plaintext);
    }

    #[test]
    fn test_wrong_key_fails_decryption() {
        let crypto1 = HistoryCrypto::new(TEST_FINGERPRINT);
        let crypto2 = HistoryCrypto::new("different_fingerprint");

        let plaintext = b"Secret message";
        let encrypted = crypto1.encrypt(plaintext).unwrap();

        // Decryption with wrong key should fail
        assert_eq!(
            crypto2.decrypt(&encrypted),
            Err(CryptoError::DecryptionFailed)
        );
    }

    #[test]
    fn test_tampered_data_fails_decryption() {
        let crypto = HistoryCrypto::new(TEST_FINGERPRINT);
        let plaintext = b"Original message";

        let mut encrypted = crypto.encrypt(plaintext).unwrap();

        // Tamper with the ciphertext (not the nonce)
        if encrypted.len() > NONCE_SIZE {
            encrypted[NONCE_SIZE] ^= 0xFF;
        }

        // Decryption should fail due to authentication
        assert_eq!(
            crypto.decrypt(&encrypted),
            Err(CryptoError::DecryptionFailed)
        );
    }

    #[test]
    fn test_too_short_data_fails() {
        let crypto = HistoryCrypto::new(TEST_FINGERPRINT);

        // Data shorter than nonce size
        let short_data = vec![0u8; NONCE_SIZE - 1];
        assert_eq!(crypto.decrypt(&short_data), Err(CryptoError::InvalidData));
    }

    #[test]
    fn test_empty_plaintext() {
        let crypto = HistoryCrypto::new(TEST_FINGERPRINT);
        let plaintext = b"";

        let encrypted = crypto.encrypt(plaintext).unwrap();
        let decrypted = crypto.decrypt(&encrypted).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_large_plaintext() {
        let crypto = HistoryCrypto::new(TEST_FINGERPRINT);
        let plaintext = vec![0xAB; 1024 * 1024]; // 1 MB

        let encrypted = crypto.encrypt(&plaintext).unwrap();
        let decrypted = crypto.decrypt(&encrypted).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_key_derivation_deterministic() {
        let crypto1 = HistoryCrypto::new(TEST_FINGERPRINT);
        let crypto2 = HistoryCrypto::new(TEST_FINGERPRINT);

        let plaintext = b"Test message";
        let encrypted = crypto1.encrypt(plaintext).unwrap();

        // Same fingerprint should produce same key, allowing decryption
        let decrypted = crypto2.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }
}
