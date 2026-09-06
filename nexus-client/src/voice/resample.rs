//! Audio resampling for voice chat
//!
//! Provides resampling between device native sample rates and the 48kHz
//! required by Opus codec. Uses the rubato crate for high-quality
//! synchronous FFT-based resampling.

use audioadapter_buffers::direct::SequentialSliceOfVecs;
use rubato::{Fft, FixedSync, Resampler};

use nexus_common::voice::{MONO_CHANNELS, VOICE_SAMPLE_RATE, VOICE_SAMPLES_PER_FRAME};

use crate::constants::{
    ERR_INPUT_ADAPTER, ERR_INPUT_RESAMPLER_CREATE, ERR_OUTPUT_ADAPTER, ERR_OUTPUT_RESAMPLER_CREATE,
    ERR_RESAMPLER_PROCESS,
};

// =============================================================================
// Constants
// =============================================================================

/// Target sample rate for voice chat (48kHz, required by Opus)
const TARGET_SAMPLE_RATE: usize = VOICE_SAMPLE_RATE as usize;

/// Chunk size for resampling - matches voice frame size (10ms at 48kHz = 480 samples)
const CHUNK_SIZE: usize = VOICE_SAMPLES_PER_FRAME as usize;

/// Number of channels for mono audio
const MONO: usize = MONO_CHANNELS as usize;

/// Number of sub-chunks for FFT resampler (1 = lowest latency)
const RESAMPLER_SUB_CHUNKS: usize = 1;

// =============================================================================
// Helpers
// =============================================================================

/// Check if resampling is needed for the given device sample rate
pub fn needs_resampling(device_rate: u32) -> bool {
    device_rate != VOICE_SAMPLE_RATE
}

// =============================================================================
// Input Resampler (device rate -> 48kHz)
// =============================================================================

/// Resamples mono audio from device sample rate to 48kHz for Opus encoding
///
/// Used for microphone input when the device doesn't support 48kHz natively.
/// This resampler only handles mono input - stereo-to-mono downmixing should
/// be done by the caller before passing samples to `process()`.
pub struct InputResampler {
    /// The rubato resampler instance
    resampler: Fft<f32>,
    /// Input buffer for accumulating samples
    input_buffer: Vec<f32>,
    /// Working buffer for resampler input (single channel)
    work_in: Vec<Vec<f32>>,
    /// Working buffer for resampler output (single channel)
    work_out: Vec<Vec<f32>>,
}

impl InputResampler {
    /// Create a new input resampler for mono audio
    ///
    /// # Arguments
    /// * `device_rate` - The device's native sample rate
    ///
    /// # Returns
    /// * `Ok(InputResampler)` - Ready to resample
    /// * `Err(String)` - If resampler creation failed
    ///
    /// # Note
    /// This resampler only handles mono input. For stereo devices, downmix
    /// to mono before calling `process()`.
    pub fn new(device_rate: u32) -> Result<Self, String> {
        // Create resampler: device_rate -> 48kHz (mono)
        // Using FixedSync::Output means fixed output size
        let resampler = Fft::<f32>::new(
            device_rate as usize,
            TARGET_SAMPLE_RATE,
            CHUNK_SIZE,
            RESAMPLER_SUB_CHUNKS,
            MONO,
            FixedSync::Output,
        )
        .map_err(|e| format!("{ERR_INPUT_RESAMPLER_CREATE}: {e}"))?;

        let input_frames_max = resampler.input_frames_max();
        let output_frames_max = resampler.output_frames_max();

        Ok(Self {
            resampler,
            input_buffer: Vec::new(),
            work_in: vec![vec![0.0; input_frames_max]],
            work_out: vec![vec![0.0; output_frames_max]],
        })
    }

    /// Process mono input samples and return resampled 48kHz samples
    ///
    /// Accumulates samples internally and returns resampled output when enough
    /// samples are available. May return empty vec if more input is needed.
    ///
    /// # Arguments
    /// * `samples` - Mono samples from device at device sample rate
    ///
    /// # Returns
    /// * `Ok(Vec<f32>)` - Mono samples at 48kHz
    /// * `Err(String)` - If resampling failed
    #[cfg(test)]
    pub fn process(&mut self, samples: &[f32]) -> Result<Vec<f32>, String> {
        let mut output = Vec::new();
        self.process_into(samples, &mut output)?;
        Ok(output)
    }

    /// Process mono input samples into a caller-owned output buffer.
    ///
    /// This avoids allocating a fresh output `Vec` from real-time capture
    /// callbacks while preserving the convenience `process()` wrapper for
    /// tests and non-callback callers.
    pub fn process_into(&mut self, samples: &[f32], output: &mut Vec<f32>) -> Result<(), String> {
        // Accumulate input samples
        self.input_buffer.extend_from_slice(samples);

        // Process chunks while we have enough input
        while self.input_buffer.len() >= self.resampler.input_frames_next() {
            let frames_needed = self.resampler.input_frames_next();

            // Copy input to working buffer
            self.work_in[0][..frames_needed].copy_from_slice(&self.input_buffer[..frames_needed]);

            // Remove processed samples from input buffer
            self.input_buffer.drain(..frames_needed);

            // Create adapters for rubato
            let input_adapter = SequentialSliceOfVecs::new(&self.work_in[..], MONO, frames_needed)
                .map_err(|e| format!("{ERR_INPUT_ADAPTER}: {e}"))?;

            let output_frames = self.resampler.output_frames_next();
            let mut output_adapter =
                SequentialSliceOfVecs::new_mut(&mut self.work_out[..], MONO, output_frames)
                    .map_err(|e| format!("{ERR_OUTPUT_ADAPTER}: {e}"))?;

            // Process through resampler
            let (_, frames_written) = self
                .resampler
                .process_into_buffer(&input_adapter, &mut output_adapter, None)
                .map_err(|e| format!("{ERR_RESAMPLER_PROCESS}: {e}"))?;

            // Accumulate output
            output.extend_from_slice(&self.work_out[0][..frames_written]);
        }

        Ok(())
    }
}

// =============================================================================
// Output Resampler (48kHz -> device rate)
// =============================================================================

/// Resamples audio from 48kHz to device sample rate for playback
///
/// Used for speaker output when the device doesn't support 48kHz natively.
pub struct OutputResampler {
    /// The rubato resampler instance
    resampler: Fft<f32>,
    /// Number of output channels
    channels: usize,
    /// Input buffer for accumulating 48kHz samples
    input_buffer: Vec<f32>,
    /// Output buffer for accumulating resampled samples
    output_buffer: Vec<f32>,
    /// Working buffer for resampler input (single channel - we process mono)
    work_in: Vec<Vec<f32>>,
    /// Working buffer for resampler output (single channel)
    work_out: Vec<Vec<f32>>,
}

impl OutputResampler {
    /// Create a new output resampler
    ///
    /// # Arguments
    /// * `device_rate` - The device's native sample rate
    /// * `channels` - Number of output channels (1 for mono, 2 for stereo)
    ///
    /// # Returns
    /// * `Ok(OutputResampler)` - Ready to resample
    /// * `Err(String)` - If resampler creation failed
    pub fn new(device_rate: u32, channels: usize) -> Result<Self, String> {
        // Create resampler: 48kHz -> device_rate (mono internally, we upmix to stereo if needed)
        // Using FixedSync::Input means fixed input size
        let resampler = Fft::<f32>::new(
            TARGET_SAMPLE_RATE,
            device_rate as usize,
            CHUNK_SIZE,
            RESAMPLER_SUB_CHUNKS,
            MONO, // we handle stereo upmix separately
            FixedSync::Input,
        )
        .map_err(|e| format!("{ERR_OUTPUT_RESAMPLER_CREATE}: {e}"))?;

        let input_frames_max = resampler.input_frames_max();
        let output_frames_max = resampler.output_frames_max();

        Ok(Self {
            resampler,
            channels,
            input_buffer: Vec::new(),
            output_buffer: Vec::new(),
            work_in: vec![vec![0.0; input_frames_max]],
            work_out: vec![vec![0.0; output_frames_max]],
        })
    }

    /// Process 48kHz mono samples and return samples at device rate
    ///
    /// Accumulates samples internally and returns resampled output when enough
    /// samples are available. Output is interleaved if stereo.
    ///
    /// # Arguments
    /// * `samples` - Mono samples at 48kHz
    ///
    /// # Returns
    /// * `Ok(Vec<f32>)` - Samples at device rate (mono or interleaved stereo based on channels)
    /// * `Err(String)` - If resampling failed
    pub fn process(&mut self, samples: &[f32]) -> Result<Vec<f32>, String> {
        // Add samples to input buffer
        self.input_buffer.extend_from_slice(samples);

        // Process chunks while we have enough input
        while self.input_buffer.len() >= self.resampler.input_frames_next() {
            let frames_needed = self.resampler.input_frames_next();

            // Copy input to working buffer
            self.work_in[0][..frames_needed].copy_from_slice(&self.input_buffer[..frames_needed]);

            // Remove processed samples from input buffer
            self.input_buffer.drain(..frames_needed);

            // Create adapters for rubato
            let input_adapter = SequentialSliceOfVecs::new(&self.work_in[..], MONO, frames_needed)
                .map_err(|e| format!("{ERR_INPUT_ADAPTER}: {e}"))?;

            let output_frames = self.resampler.output_frames_next();
            let mut output_adapter =
                SequentialSliceOfVecs::new_mut(&mut self.work_out[..], MONO, output_frames)
                    .map_err(|e| format!("{ERR_OUTPUT_ADAPTER}: {e}"))?;

            // Process through resampler
            let (_, frames_written) = self
                .resampler
                .process_into_buffer(&input_adapter, &mut output_adapter, None)
                .map_err(|e| format!("{ERR_RESAMPLER_PROCESS}: {e}"))?;

            // Accumulate output, converting to stereo if needed
            if self.channels == MONO {
                self.output_buffer
                    .extend_from_slice(&self.work_out[0][..frames_written]);
            } else {
                // Stereo output - duplicate mono to both channels
                for &sample in &self.work_out[0][..frames_written] {
                    self.output_buffer.push(sample);
                    self.output_buffer.push(sample);
                }
            }
        }

        // Return accumulated output and clear buffer
        Ok(std::mem::take(&mut self.output_buffer))
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_common::voice::STEREO_CHANNELS;

    const STEREO: usize = STEREO_CHANNELS as usize;

    #[test]
    fn test_needs_resampling() {
        assert!(!needs_resampling(48000));
        assert!(needs_resampling(44100));
        assert!(needs_resampling(96000));
        assert!(needs_resampling(16000));
    }

    #[test]
    fn test_input_resampler_creation() {
        // 44.1kHz should work
        let resampler = InputResampler::new(44100);
        assert!(resampler.is_ok());

        // 96kHz should work
        let resampler = InputResampler::new(96000);
        assert!(resampler.is_ok());

        // 16kHz should work
        let resampler = InputResampler::new(16000);
        assert!(resampler.is_ok());
    }

    #[test]
    fn test_output_resampler_creation() {
        // 44.1kHz mono should work
        let resampler = OutputResampler::new(44100, MONO);
        assert!(resampler.is_ok());

        // 44.1kHz stereo should work
        let resampler = OutputResampler::new(44100, STEREO);
        assert!(resampler.is_ok());

        // 96kHz stereo should work
        let resampler = OutputResampler::new(96000, STEREO);
        assert!(resampler.is_ok());
    }

    #[test]
    fn test_input_resampler_processing() {
        let mut resampler = InputResampler::new(44100).unwrap();

        // Generate 10ms of 44.1kHz mono samples (441 samples)
        let input: Vec<f32> = (0..441).map(|i| (i as f32 * 0.01).sin()).collect();

        let output = resampler.process(&input);
        assert!(output.is_ok());

        // Feed more data to get output
        let mut total_output = output.unwrap().len();

        for _ in 0..10 {
            let more_input: Vec<f32> = (0..441).map(|i| (i as f32 * 0.01).sin()).collect();
            let more_output = resampler.process(&more_input);
            assert!(more_output.is_ok());
            total_output += more_output.unwrap().len();
            if total_output > 0 {
                break;
            }
        }

        assert!(
            total_output > 0,
            "Should produce output after feeding enough samples"
        );
    }

    #[test]
    fn test_input_resampler_process_into_matches_process() {
        let mut process_resampler = InputResampler::new(44100).unwrap();
        let mut into_resampler = InputResampler::new(44100).unwrap();
        let input: Vec<f32> = (0..4410).map(|i| (i as f32 * 0.01).sin()).collect();

        let process_output = process_resampler.process(&input).unwrap();
        let mut into_output = Vec::new();
        into_resampler
            .process_into(&input, &mut into_output)
            .unwrap();

        assert_eq!(into_output, process_output);
    }

    #[test]
    fn test_output_resampler_processing() {
        let mut resampler = OutputResampler::new(44100, MONO).unwrap();

        // Generate 10ms of 48kHz samples (480 samples)
        let input: Vec<f32> = (0..480).map(|i| (i as f32 * 0.01).sin()).collect();

        let output = resampler.process(&input);
        assert!(output.is_ok());

        // Feed more data to get output
        let mut total_output = output.unwrap().len();

        for _ in 0..10 {
            let more_input: Vec<f32> = (0..480).map(|i| (i as f32 * 0.01).sin()).collect();
            let more_output = resampler.process(&more_input);
            assert!(more_output.is_ok());
            total_output += more_output.unwrap().len();
            if total_output > 0 {
                break;
            }
        }

        assert!(
            total_output > 0,
            "Should produce output after feeding enough samples"
        );
    }

    #[test]
    fn test_output_resampler_stereo() {
        let mut resampler = OutputResampler::new(44100, STEREO).unwrap();

        // Generate 10ms of 48kHz mono samples
        let input: Vec<f32> = (0..480).map(|i| (i as f32 * 0.01).sin()).collect();

        // Feed enough data to get output
        let mut total_output = Vec::new();
        for _ in 0..10 {
            let output = resampler.process(&input);
            assert!(output.is_ok());
            total_output.extend(output.unwrap());
            if total_output.len() > 100 {
                break;
            }
        }

        // Stereo output should have even number of samples (L, R pairs)
        assert_eq!(
            total_output.len() % STEREO,
            0,
            "Stereo output should have even number of samples"
        );
    }

    // =========================================================================
    // Additional targeted tests
    // =========================================================================

    #[test]
    fn test_input_resampler_empty_input() {
        let mut resampler = InputResampler::new(44100).unwrap();

        // Empty input should return Ok with empty output
        let output = resampler.process(&[]);
        assert!(output.is_ok());
        assert!(output.unwrap().is_empty());
    }

    #[test]
    fn test_output_resampler_empty_input() {
        let mut resampler = OutputResampler::new(44100, MONO).unwrap();

        // Empty input should return Ok with empty output
        let output = resampler.process(&[]);
        assert!(output.is_ok());
        assert!(output.unwrap().is_empty());
    }

    #[test]
    fn test_output_resampler_stereo_upmix_correctness() {
        let mut resampler = OutputResampler::new(44100, STEREO).unwrap();

        // Feed enough 48kHz mono data to get stereo output
        let mut stereo_output = Vec::new();
        for i in 0..20 {
            // Use distinct values so we can verify L == R
            let input: Vec<f32> = (0..480).map(|j| (i * 480 + j) as f32 / 10000.0).collect();
            let output = resampler.process(&input).unwrap();
            stereo_output.extend(output);
        }

        // Verify we got output
        assert!(
            stereo_output.len() >= 100,
            "Should have at least 100 samples, got {}",
            stereo_output.len()
        );

        // Verify stereo: each pair should have L == R
        for (i, chunk) in stereo_output.as_chunks::<STEREO>().0.iter().enumerate() {
            let left = chunk[0];
            let right = chunk[1];
            assert_eq!(
                left, right,
                "Stereo pair {} should have L == R, got L={}, R={}",
                i, left, right
            );
        }
    }
}
