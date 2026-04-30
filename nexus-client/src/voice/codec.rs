//! Opus codec wrapper for voice encoding/decoding
//!
//! Provides a simple interface to the Opus codec for encoding microphone
//! input and decoding received voice packets. Uses f32 samples throughout
//! for compatibility with WebRTC audio processing.

use std::collections::HashMap;

use crate::constants::ERR_DECODER_MISSING_AFTER_INSERT;
use nexus_common::voice::{
    VOICE_CHANNELS, VOICE_SAMPLE_RATE, VOICE_SAMPLES_PER_FRAME, VoiceQuality,
};
use opus::{Application, Channels, Decoder, Encoder};

// =============================================================================
// Constants
// =============================================================================

/// Maximum encoded frame size in bytes
/// At 96kbps with 10ms frames: 96000 * 0.010 / 8 = 120 bytes typical
/// We allow extra headroom for packet overhead
const MAX_ENCODED_FRAME_SIZE: usize = 512;

// =============================================================================
// Voice Encoder
// =============================================================================

/// Opus encoder for outgoing voice audio
pub struct VoiceEncoder {
    /// The Opus encoder instance
    encoder: Encoder,
}

impl VoiceEncoder {
    /// Create a new voice encoder with the specified quality
    ///
    /// # Arguments
    /// * `quality` - Voice quality preset (affects bitrate)
    ///
    /// # Returns
    /// * `Ok(VoiceEncoder)` - Encoder ready for use
    /// * `Err(String)` - Error message if encoder couldn't be created
    pub fn new(quality: VoiceQuality) -> Result<Self, String> {
        let channels = if VOICE_CHANNELS == 1 {
            Channels::Mono
        } else {
            Channels::Stereo
        };

        let mut encoder = Encoder::new(VOICE_SAMPLE_RATE, channels, Application::Voip)
            .map_err(|e| format!("Failed to create Opus encoder: {}", e))?;

        // Set the bitrate based on quality
        encoder
            .set_bitrate(opus::Bitrate::Bits(quality.bitrate()))
            .map_err(|e| format!("Failed to set bitrate: {}", e))?;

        // Enable in-band FEC for automatic packet loss recovery
        // This embeds redundant data from the previous frame (~50% overhead)
        // which allows the decoder to recover from single packet losses
        encoder
            .set_inband_fec(true)
            .map_err(|e| format!("Failed to enable FEC: {}", e))?;

        // Enable DTX (discontinuous transmission) to skip sending during silence
        // This saves bandwidth and CPU when users aren't actively speaking
        encoder
            .set_dtx(true)
            .map_err(|e| format!("Failed to enable DTX: {}", e))?;

        Ok(Self { encoder })
    }

    /// Update the encoder's bitrate dynamically
    ///
    /// # Arguments
    /// * `quality` - New voice quality preset
    ///
    /// # Returns
    /// * `Ok(())` - Bitrate updated successfully
    /// * `Err(String)` - Error message if bitrate couldn't be set
    pub fn set_quality(&mut self, quality: VoiceQuality) -> Result<(), String> {
        self.encoder
            .set_bitrate(opus::Bitrate::Bits(quality.bitrate()))
            .map_err(|e| format!("Failed to set bitrate: {}", e))
    }

    /// Encode a frame of audio samples
    ///
    /// # Arguments
    /// * `samples` - Audio samples in f32 format (must be VOICE_SAMPLES_PER_FRAME samples).
    ///   Values should be normalized to [-1.0, 1.0].
    ///
    /// # Returns
    /// * `Ok(Vec<u8>)` - Encoded Opus frame
    /// * `Err(String)` - Error message if encoding failed
    pub fn encode(&mut self, samples: &[f32]) -> Result<Vec<u8>, String> {
        if samples.len() != VOICE_SAMPLES_PER_FRAME as usize {
            return Err(format!(
                "Expected {} samples, got {}",
                VOICE_SAMPLES_PER_FRAME,
                samples.len()
            ));
        }

        let mut output = vec![0u8; MAX_ENCODED_FRAME_SIZE];

        let len = self
            .encoder
            .encode_float(samples, &mut output)
            .map_err(|e| format!("Opus encode error: {}", e))?;

        output.truncate(len);
        Ok(output)
    }
}

// =============================================================================
// Voice Decoder
// =============================================================================

/// Opus decoder for incoming voice audio
pub struct VoiceDecoder {
    /// The Opus decoder instance
    decoder: Decoder,
}

impl VoiceDecoder {
    /// Create a new voice decoder
    ///
    /// # Returns
    /// * `Ok(VoiceDecoder)` - Decoder ready for use
    /// * `Err(String)` - Error message if decoder couldn't be created
    pub fn new() -> Result<Self, String> {
        let channels = if VOICE_CHANNELS == 1 {
            Channels::Mono
        } else {
            Channels::Stereo
        };

        let decoder = Decoder::new(VOICE_SAMPLE_RATE, channels)
            .map_err(|e| format!("Failed to create Opus decoder: {}", e))?;

        Ok(Self { decoder })
    }

    /// Decode an Opus frame to audio samples
    ///
    /// # Arguments
    /// * `data` - Encoded Opus frame
    ///
    /// # Returns
    /// * `Ok(Vec<f32>)` - Decoded audio samples normalized to [-1.0, 1.0]
    /// * `Err(String)` - Error message if decoding failed
    pub fn decode(&mut self, data: &[u8]) -> Result<Vec<f32>, String> {
        let mut output = vec![0f32; VOICE_SAMPLES_PER_FRAME as usize];

        let len = self
            .decoder
            .decode_float(data, &mut output, false)
            .map_err(|e| format!("Opus decode error: {}", e))?;

        output.truncate(len);
        Ok(output)
    }

    /// Decode with packet loss concealment
    ///
    /// Call this when a packet is lost to generate interpolated audio.
    ///
    /// # Returns
    /// * `Ok(Vec<f32>)` - Concealed audio samples
    /// * `Err(String)` - Error message if PLC failed
    pub fn decode_lost(&mut self) -> Result<Vec<f32>, String> {
        let mut output = vec![0f32; VOICE_SAMPLES_PER_FRAME as usize];

        let len = self
            .decoder
            .decode_float(&[], &mut output, true)
            .map_err(|e| format!("Opus PLC error: {}", e))?;

        output.truncate(len);
        Ok(output)
    }
}

// =============================================================================
// Decoder Pool
// =============================================================================

/// Pool of decoders for multiple voice chat participants
///
/// Maintains one decoder per sender to preserve codec state for
/// better packet loss concealment.
pub struct DecoderPool {
    /// Decoders keyed by sender nickname (lowercase)
    decoders: HashMap<String, VoiceDecoder>,
}

impl DecoderPool {
    /// Create a new empty decoder pool
    pub fn new() -> Self {
        Self {
            decoders: HashMap::new(),
        }
    }

    /// Decode audio from a specific sender
    ///
    /// Creates a new decoder for the sender if one doesn't exist.
    ///
    /// # Arguments
    /// * `sender` - Nickname of the sender
    /// * `data` - Encoded Opus frame
    ///
    /// # Returns
    /// * `Ok(Vec<f32>)` - Decoded audio samples
    /// * `Err(String)` - Error message if decoding failed
    pub fn decode(&mut self, sender: &str, data: &[u8]) -> Result<Vec<f32>, String> {
        let key = sender.to_lowercase();

        let decoder = if let Some(d) = self.decoders.get_mut(&key) {
            d
        } else {
            let new_decoder = VoiceDecoder::new()?;
            self.decoders.insert(key.clone(), new_decoder);
            self.decoders
                .get_mut(&key)
                .expect(ERR_DECODER_MISSING_AFTER_INSERT)
        };

        decoder.decode(data)
    }

    /// Signal packet loss for a specific sender
    ///
    /// Generates concealed audio using the sender's decoder state.
    ///
    /// # Arguments
    /// * `sender` - Nickname of the sender
    ///
    /// # Returns
    /// * `Ok(Vec<f32>)` - Concealed audio samples
    /// * `Err(String)` - Error message if PLC failed or sender unknown
    pub fn decode_lost(&mut self, sender: &str) -> Result<Vec<f32>, String> {
        let key = sender.to_lowercase();

        let decoder = self
            .decoders
            .get_mut(&key)
            .ok_or_else(|| format!("No decoder for sender: {}", sender))?;

        decoder.decode_lost()
    }

    /// Remove a sender's decoder
    ///
    /// Call this when a user leaves voice to free resources.
    pub fn remove(&mut self, sender: &str) {
        self.decoders.remove(&sender.to_lowercase());
    }
}

impl Default for DecoderPool {
    fn default() -> Self {
        Self::new()
    }
}

// Test-only methods
#[cfg(test)]
impl DecoderPool {
    /// Check if the pool is empty (test-only)
    pub fn is_empty(&self) -> bool {
        self.decoders.is_empty()
    }

    /// Get the number of decoders in the pool (test-only)
    pub fn len(&self) -> usize {
        self.decoders.len()
    }

    /// Clear all decoders from the pool (test-only)
    pub fn clear(&mut self) {
        self.decoders.clear();
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encoder_creation() {
        let encoder = VoiceEncoder::new(VoiceQuality::High);
        assert!(encoder.is_ok());
    }

    #[test]
    fn test_encoder_set_quality() {
        let mut encoder = VoiceEncoder::new(VoiceQuality::High).unwrap();

        // Change quality dynamically
        assert!(encoder.set_quality(VoiceQuality::Low).is_ok());
        assert!(encoder.set_quality(VoiceQuality::Medium).is_ok());
        assert!(encoder.set_quality(VoiceQuality::VeryHigh).is_ok());

        // Encoding should still work after quality change
        let samples = vec![0.0f32; VOICE_SAMPLES_PER_FRAME as usize];
        assert!(encoder.encode(&samples).is_ok());
    }

    #[test]
    fn test_decoder_creation() {
        let decoder = VoiceDecoder::new();
        assert!(decoder.is_ok());
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let mut encoder = VoiceEncoder::new(VoiceQuality::High).unwrap();
        let mut decoder = VoiceDecoder::new().unwrap();

        // Create a simple test signal (sine wave) - f32 samples are already normalized
        let samples: Vec<f32> = (0..VOICE_SAMPLES_PER_FRAME)
            .map(|i| {
                let t = i as f32 / VOICE_SAMPLE_RATE as f32;
                f32::sin(2.0 * std::f32::consts::PI * 440.0 * t) * 0.5
            })
            .collect();

        // Encode
        let encoded = encoder.encode(&samples).unwrap();
        assert!(!encoded.is_empty());
        assert!(encoded.len() < samples.len() * 4); // Should be smaller than raw PCM f32

        // Decode
        let decoded = decoder.decode(&encoded).unwrap();
        assert_eq!(decoded.len(), VOICE_SAMPLES_PER_FRAME as usize);

        // Values should be similar (lossy compression means not exact)
        // Just verify we got reasonable output, not silence
        let max_amplitude: f32 = decoded.iter().map(|&s| s.abs()).fold(0.0, f32::max);
        assert!(max_amplitude > 0.1, "Decoded audio seems too quiet");
    }

    #[test]
    fn test_encoder_wrong_frame_size() {
        let mut encoder = VoiceEncoder::new(VoiceQuality::High).unwrap();

        // Too few samples
        let samples = vec![0.0f32; 100];
        assert!(encoder.encode(&samples).is_err());

        // Too many samples
        let samples = vec![0.0f32; VOICE_SAMPLES_PER_FRAME as usize * 2];
        assert!(encoder.encode(&samples).is_err());
    }

    #[test]
    fn test_decoder_pool() {
        let mut pool = DecoderPool::new();
        assert!(pool.is_empty());

        // Create encoder and encode a frame
        let mut encoder = VoiceEncoder::new(VoiceQuality::High).unwrap();
        let samples = vec![0.0f32; VOICE_SAMPLES_PER_FRAME as usize];
        let encoded = encoder.encode(&samples).unwrap();

        // Decode from two different senders
        let decoded1 = pool.decode("Alice", &encoded);
        assert!(decoded1.is_ok());
        assert_eq!(pool.len(), 1);

        let decoded2 = pool.decode("Bob", &encoded);
        assert!(decoded2.is_ok());
        assert_eq!(pool.len(), 2);

        // Same sender reuses decoder
        let decoded3 = pool.decode("alice", &encoded); // lowercase
        assert!(decoded3.is_ok());
        assert_eq!(pool.len(), 2); // Still 2, not 3

        // Remove a sender
        pool.remove("Alice");
        assert_eq!(pool.len(), 1);

        // Clear all
        pool.clear();
        assert!(pool.is_empty());
    }

    #[test]
    fn test_decoder_plc() {
        let mut encoder = VoiceEncoder::new(VoiceQuality::High).unwrap();
        let mut decoder = VoiceDecoder::new().unwrap();

        // First decode a real frame to initialize decoder state
        let samples: Vec<f32> = (0..VOICE_SAMPLES_PER_FRAME)
            .map(|i| {
                let t = i as f32 / VOICE_SAMPLE_RATE as f32;
                f32::sin(2.0 * std::f32::consts::PI * 440.0 * t) * 0.5
            })
            .collect();
        let encoded = encoder.encode(&samples).unwrap();
        decoder.decode(&encoded).unwrap();

        // Now simulate packet loss
        let concealed = decoder.decode_lost();
        assert!(concealed.is_ok());
        assert_eq!(concealed.unwrap().len(), VOICE_SAMPLES_PER_FRAME as usize);
    }
}
