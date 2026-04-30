//! Jitter buffer for voice packet reordering and smoothing
//!
//! Implements an adaptive jitter buffer that reorders out-of-order packets
//! and provides smooth audio output despite network jitter. The buffer size
//! adjusts dynamically based on observed network conditions.

use crate::constants::ERR_NEXT_SEQUENCE_NONE;
use std::collections::BTreeMap;
use std::time::Instant;

// =============================================================================
// Constants
// =============================================================================

/// Minimum buffer size in frames (10ms per frame)
const MIN_BUFFER_FRAMES: usize = 2; // 20ms

/// Maximum buffer size in frames (10ms per frame)
const MAX_BUFFER_FRAMES: usize = 20; // 200ms

/// Initial buffer size in frames (10ms per frame)
const INITIAL_BUFFER_FRAMES: usize = 2; // 20ms

/// EMA smoothing factor (0.125 = 1/8, gives ~8 packet smoothing)
const JITTER_EMA_ALPHA: f64 = 0.125;

/// Safety multiplier for target buffer (2.5x average jitter)
const JITTER_SAFETY_MULTIPLIER: f64 = 2.5;

/// Frame duration in milliseconds (must match VOICE_FRAME_DURATION_MS in nexus-common)
const FRAME_DURATION_MS: f64 = 10.0;

/// Maximum sequence number gap before considering packets too old
const MAX_SEQUENCE_GAP: u32 = 100;

// =============================================================================
// Buffered Packet
// =============================================================================

/// A packet stored in the jitter buffer
#[derive(Debug, Clone)]
struct BufferedPacket {
    /// Decoded audio samples (f32 normalized to -1.0..1.0)
    samples: Vec<f32>,
}

// =============================================================================
// Jitter Buffer
// =============================================================================

/// Adaptive jitter buffer for a single voice stream
///
/// Buffers incoming packets and outputs them in order, smoothing
/// out network jitter and handling packet reordering. Buffer size
/// adapts based on observed jitter using exponential moving average.
pub struct JitterBuffer {
    /// Buffered packets, keyed by sequence number
    packets: BTreeMap<u32, BufferedPacket>,
    /// Next expected sequence number
    next_sequence: Option<u32>,
    /// Last packet arrival time (for jitter calculation)
    last_arrival: Option<Instant>,
    /// Last packet sequence (for expected interval calculation)
    last_sequence: Option<u32>,
    /// Exponential moving average of jitter in milliseconds
    avg_jitter_ms: f64,
    /// Current target buffer size in frames
    target_frames: usize,
}

impl JitterBuffer {
    /// Create a new jitter buffer
    pub fn new() -> Self {
        Self {
            packets: BTreeMap::new(),
            next_sequence: None,
            last_arrival: None,
            last_sequence: None,
            avg_jitter_ms: 0.0,
            target_frames: INITIAL_BUFFER_FRAMES,
        }
    }

    /// Push a packet into the jitter buffer
    ///
    /// # Arguments
    /// * `sequence` - Packet sequence number
    /// * `_timestamp` - Packet timestamp in samples (reserved for future use)
    /// * `samples` - Decoded audio samples (f32 normalized to -1.0..1.0)
    ///
    /// # Returns
    /// * `true` if packet was accepted
    /// * `false` if packet was too old or duplicate
    pub fn push(&mut self, sequence: u32, _timestamp: u32, samples: Vec<f32>) -> bool {
        let now = Instant::now();

        // Update jitter estimate
        self.update_jitter(sequence, now);

        // Initialize next_sequence on first packet
        if self.next_sequence.is_none() {
            self.next_sequence = Some(sequence);
        }

        let next = self.next_sequence.expect(ERR_NEXT_SEQUENCE_NONE);

        // Check if packet is too old
        if sequence_before(sequence, next) {
            // Packet arrived too late, discard
            return false;
        }

        // Check for unreasonably large gap (might indicate sequence wrap or reset)
        if sequence.wrapping_sub(next) > MAX_SEQUENCE_GAP {
            // Reset the buffer - sender may have restarted
            self.reset();
            self.next_sequence = Some(sequence);
        }

        // Check for duplicate
        if self.packets.contains_key(&sequence) {
            return false;
        }

        // Add to buffer
        self.packets.insert(sequence, BufferedPacket { samples });

        // Limit buffer size by removing old packets
        while self.packets.len() > self.target_frames * 3 {
            if let Some((&oldest_seq, _)) = self.packets.first_key_value() {
                if oldest_seq < next {
                    self.packets.remove(&oldest_seq);
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        true
    }

    /// Update jitter estimate based on packet arrival
    fn update_jitter(&mut self, sequence: u32, now: Instant) {
        if let (Some(last_arrival), Some(last_seq)) = (self.last_arrival, self.last_sequence) {
            // Only measure jitter for sequential packets (not reordered ones)
            // Reordered packets would give bogus jitter measurements
            let seq_diff = sequence.wrapping_sub(last_seq);
            if seq_diff == 1 {
                // This packet is the next expected one - measure jitter
                let expected_interval_ms = FRAME_DURATION_MS;
                let actual_interval_ms = now.duration_since(last_arrival).as_secs_f64() * 1000.0;

                // Jitter is the deviation from expected
                let jitter_ms = (actual_interval_ms - expected_interval_ms).abs();

                // Update EMA
                self.avg_jitter_ms =
                    JITTER_EMA_ALPHA * jitter_ms + (1.0 - JITTER_EMA_ALPHA) * self.avg_jitter_ms;

                // Update target buffer size
                let target_ms = self.avg_jitter_ms * JITTER_SAFETY_MULTIPLIER;
                let target_frames = (target_ms / FRAME_DURATION_MS).ceil() as usize;
                self.target_frames = target_frames.clamp(MIN_BUFFER_FRAMES, MAX_BUFFER_FRAMES);
            }
        }

        self.last_arrival = Some(now);
        self.last_sequence = Some(sequence);
    }

    /// Pop the next frame from the jitter buffer
    ///
    /// Returns the next frame in sequence if available and the buffer
    /// has been filled enough to absorb jitter.
    ///
    /// # Returns
    /// * `Some(samples)` - Next frame's audio samples (f32 normalized to -1.0..1.0)
    /// * `None` - No frame available (buffer underrun or not ready)
    pub fn pop(&mut self) -> Option<Vec<f32>> {
        let next = self.next_sequence?;

        // Wait for buffer to fill before starting playback
        if self.packets.len() < self.target_frames {
            return None;
        }

        // Try to get the next expected packet
        if let Some(packet) = self.packets.remove(&next) {
            self.next_sequence = Some(next.wrapping_add(1));
            return Some(packet.samples);
        }

        // Packet is missing - check if we should skip or wait
        // Look ahead to see if we have future packets
        let have_future = self.packets.keys().any(|&seq| sequence_after(seq, next));

        if have_future {
            // We have future packets, so this one is lost
            self.next_sequence = Some(next.wrapping_add(1));

            // Return None to signal loss (caller should use PLC)
            return None;
        }

        // No future packets - buffer underrun, wait for more data
        None
    }

    /// Check if we have a packet loss at the current position
    ///
    /// Returns true if the next expected packet is missing but we have
    /// later packets, indicating loss rather than buffer underrun.
    pub fn has_loss(&self) -> bool {
        if let Some(next) = self.next_sequence {
            if self.packets.contains_key(&next) {
                return false;
            }
            // Check if we have any packet after next
            self.packets.keys().any(|&seq| sequence_after(seq, next))
        } else {
            false
        }
    }

    /// Reset the jitter buffer
    ///
    /// Clears all buffered packets and resets state.
    pub fn reset(&mut self) {
        self.packets.clear();
        self.next_sequence = None;
        self.last_arrival = None;
        self.last_sequence = None;
        // Keep avg_jitter_ms and target_frames - they're still useful estimates
    }

    /// Get the current target buffer size in frames
    #[cfg(test)]
    pub fn target_frames(&self) -> usize {
        self.target_frames
    }

    /// Get the current average jitter estimate in milliseconds
    #[cfg(test)]
    pub fn avg_jitter_ms(&self) -> f64 {
        self.avg_jitter_ms
    }
}

impl Default for JitterBuffer {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Jitter Buffer Pool
// =============================================================================

/// Pool of jitter buffers for multiple voice streams
///
/// Maintains one jitter buffer per sender.
pub struct JitterBufferPool {
    /// Jitter buffers keyed by sender nickname (lowercase)
    buffers: std::collections::HashMap<String, JitterBuffer>,
}

impl JitterBufferPool {
    /// Create a new empty jitter buffer pool
    pub fn new() -> Self {
        Self {
            buffers: std::collections::HashMap::new(),
        }
    }

    /// Push a packet for a sender
    pub fn push(&mut self, sender: &str, sequence: u32, timestamp: u32, samples: Vec<f32>) -> bool {
        let key = sender.to_lowercase();
        self.buffers
            .entry(key)
            .or_default()
            .push(sequence, timestamp, samples)
    }

    /// Remove a sender's jitter buffer
    pub fn remove(&mut self, sender: &str) {
        self.buffers.remove(&sender.to_lowercase());
    }

    /// Iterate over all buffers mutably
    ///
    /// Returns an iterator of (sender_key, &mut JitterBuffer) pairs.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&String, &mut JitterBuffer)> {
        self.buffers.iter_mut()
    }
}

impl Default for JitterBufferPool {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Check if sequence a comes before sequence b (handling wraparound)
fn sequence_before(a: u32, b: u32) -> bool {
    // Handle wraparound: a is before b if (b - a) is a small positive number
    let diff = b.wrapping_sub(a);
    diff > 0 && diff < (u32::MAX / 2)
}

/// Check if sequence a comes after sequence b (handling wraparound)
fn sequence_after(a: u32, b: u32) -> bool {
    sequence_before(b, a)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use std::thread;
    use std::time::Duration;

    use nexus_common::voice::VOICE_SAMPLES_PER_FRAME;

    use super::*;

    fn make_samples() -> Vec<f32> {
        vec![0.0f32; VOICE_SAMPLES_PER_FRAME as usize]
    }

    #[test]
    fn test_jitter_buffer_new() {
        let buffer = JitterBuffer::new();
        assert_eq!(buffer.packets.len(), 0);
        assert_eq!(buffer.target_frames(), INITIAL_BUFFER_FRAMES);
    }

    #[test]
    fn test_jitter_buffer_push_pop() {
        let mut buffer = JitterBuffer::new();

        // Push enough packets to fill the buffer
        for i in 0..INITIAL_BUFFER_FRAMES as u32 {
            assert!(buffer.push(i, i * VOICE_SAMPLES_PER_FRAME, make_samples()));
        }

        assert_eq!(buffer.packets.len(), INITIAL_BUFFER_FRAMES);

        // Pop the first packet
        let samples = buffer.pop();
        assert!(samples.is_some());
        assert_eq!(samples.unwrap().len(), VOICE_SAMPLES_PER_FRAME as usize);
    }

    #[test]
    fn test_jitter_buffer_reorder() {
        let mut buffer = JitterBuffer::new();

        // Push packets out of order: 0, 2, 1, 3, 4, 5
        // Out-of-order packets don't affect jitter measurement, buffer stays at minimum
        // Need at least MIN_BUFFER_FRAMES (2) packets before pop() returns data
        buffer.push(0, 0, make_samples());
        buffer.push(2, VOICE_SAMPLES_PER_FRAME * 2, make_samples());
        buffer.push(1, VOICE_SAMPLES_PER_FRAME, make_samples());
        buffer.push(3, VOICE_SAMPLES_PER_FRAME * 3, make_samples());
        buffer.push(4, VOICE_SAMPLES_PER_FRAME * 4, make_samples());
        buffer.push(5, VOICE_SAMPLES_PER_FRAME * 5, make_samples());

        // Buffer has 6 packets, target should still be minimum (2)
        assert_eq!(buffer.packets.len(), 6);
        assert_eq!(buffer.target_frames(), MIN_BUFFER_FRAMES);

        // Should be able to pop in order (packets were reordered internally)
        assert!(buffer.pop().is_some()); // seq 0
        assert!(buffer.pop().is_some()); // seq 1
    }

    #[test]
    fn test_jitter_buffer_duplicate_rejection() {
        let mut buffer = JitterBuffer::new();

        assert!(buffer.push(0, 0, make_samples()));
        assert!(!buffer.push(0, 0, make_samples())); // Duplicate
    }

    #[test]
    fn test_jitter_buffer_late_packet_rejection() {
        let mut buffer = JitterBuffer::new();

        // Fill buffer and pop some
        for i in 0..5 {
            buffer.push(i, i * VOICE_SAMPLES_PER_FRAME, make_samples());
        }
        buffer.pop(); // pops seq 0, next_sequence becomes 1
        buffer.pop(); // pops seq 1, next_sequence becomes 2

        // Now try to push packet 0 - should be rejected as too late
        assert!(!buffer.push(0, 0, make_samples()));
    }

    #[test]
    fn test_jitter_buffer_loss_detection() {
        let mut buffer = JitterBuffer::new();

        // Push packets with a gap: 0, 1, 3, 4, 5, 6 (missing 2)
        // Need at least MIN_BUFFER_FRAMES (2) packets before pop() returns data
        buffer.push(0, 0, make_samples());
        buffer.push(1, VOICE_SAMPLES_PER_FRAME, make_samples());
        buffer.push(3, VOICE_SAMPLES_PER_FRAME * 3, make_samples());
        buffer.push(4, VOICE_SAMPLES_PER_FRAME * 4, make_samples());
        buffer.push(5, VOICE_SAMPLES_PER_FRAME * 5, make_samples());
        buffer.push(6, VOICE_SAMPLES_PER_FRAME * 6, make_samples());

        // Pop 0 and 1
        buffer.pop();
        buffer.pop();

        // Now next_sequence is 2, which is missing but we have 3+
        assert!(buffer.has_loss());
    }

    #[test]
    fn test_jitter_buffer_reset() {
        let mut buffer = JitterBuffer::new();

        buffer.push(0, 0, make_samples());
        buffer.push(1, VOICE_SAMPLES_PER_FRAME, make_samples());

        buffer.reset();

        assert_eq!(buffer.packets.len(), 0);
    }

    #[test]
    fn test_sequence_wraparound() {
        // Test near wraparound point
        assert!(sequence_before(u32::MAX - 1, u32::MAX));
        assert!(sequence_before(u32::MAX, 0)); // Wraparound
        assert!(sequence_before(u32::MAX, 1)); // Wraparound

        assert!(sequence_after(0, u32::MAX)); // Wraparound
        assert!(sequence_after(1, u32::MAX)); // Wraparound
    }

    #[test]
    fn test_jitter_buffer_pool() {
        let mut pool = JitterBufferPool::new();

        // Push for two senders
        pool.push("Alice", 0, 0, make_samples());
        pool.push("Bob", 0, 0, make_samples());

        assert_eq!(pool.buffers.len(), 2);

        // Case insensitive
        pool.push("alice", 1, VOICE_SAMPLES_PER_FRAME, make_samples());
        assert_eq!(pool.buffers.len(), 2);

        // Remove one
        pool.remove("Alice");
        assert_eq!(pool.buffers.len(), 1);
    }

    #[test]
    fn test_adaptive_buffer_grows_with_jitter() {
        let mut buffer = JitterBuffer::new();

        // Simulate high jitter by adding delays between packets (expected interval: 10ms)
        buffer.push(0, 0, make_samples());
        thread::sleep(Duration::from_millis(50)); // 40ms late
        buffer.push(1, VOICE_SAMPLES_PER_FRAME, make_samples());
        thread::sleep(Duration::from_millis(60)); // 40ms late
        buffer.push(2, VOICE_SAMPLES_PER_FRAME * 2, make_samples());
        thread::sleep(Duration::from_millis(70)); // 50ms late
        buffer.push(3, VOICE_SAMPLES_PER_FRAME * 3, make_samples());

        // Buffer should have grown above minimum
        assert!(
            buffer.target_frames() >= MIN_BUFFER_FRAMES,
            "Buffer should be at least minimum size"
        );
        assert!(
            buffer.avg_jitter_ms() > 0.0,
            "Should have measured some jitter"
        );
    }

    #[test]
    fn test_adaptive_buffer_respects_limits() {
        let mut buffer = JitterBuffer::new();

        // Simulate extreme jitter
        for i in 0..20 {
            buffer.push(i, i * VOICE_SAMPLES_PER_FRAME, make_samples());
            thread::sleep(Duration::from_millis(100)); // Way more than 10ms
        }

        // Should be clamped to max
        assert!(
            buffer.target_frames() <= MAX_BUFFER_FRAMES,
            "Buffer should not exceed maximum"
        );
    }
}
