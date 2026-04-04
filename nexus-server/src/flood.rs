//! Chat flood protection using a token bucket algorithm.
//!
//! `FloodConfig` is shared server-wide (via `Arc`) and configurable at runtime.
//! `FloodTracker` is per-connection state that tracks the token bucket and violation count.

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;

/// Maximum consecutive flood violations before disconnect.
const MAX_FLOOD_VIOLATIONS: u32 = 3;

/// Shared server-wide flood configuration, safe to mutate at runtime.
pub struct FloodConfig {
    burst: AtomicU32,
    rate: AtomicU32,
}

impl FloodConfig {
    pub fn new(burst: u32, rate: u32) -> Self {
        Self {
            burst: AtomicU32::new(burst),
            rate: AtomicU32::new(rate),
        }
    }

    pub fn burst(&self) -> u32 {
        self.burst.load(Ordering::Relaxed)
    }

    pub fn rate(&self) -> u32 {
        self.rate.load(Ordering::Relaxed)
    }

    pub fn set_burst(&self, burst: u32) {
        self.burst.store(burst, Ordering::Relaxed);
    }

    pub fn set_rate(&self, rate: u32) {
        self.rate.store(rate, Ordering::Relaxed);
    }
}

/// Result of a flood check.
pub enum FloodCheck {
    /// Message allowed, proceed.
    Allowed,
    /// Message blocked, wait N seconds (ceil to 1 minimum).
    Limited {
        wait_seconds: u32,
        violation: u32,
        max_violations: u32,
    },
    /// Too many consecutive violations, disconnect.
    Disconnect,
}

/// Per-connection flood tracking state using a token bucket.
pub struct FloodTracker {
    /// Current tokens in the bucket.
    tokens: f64,
    /// Last time tokens were refilled.
    last_check: Instant,
    /// Consecutive flood violations (resets on allowed message).
    consecutive_violations: u32,
}

impl Default for FloodTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl FloodTracker {
    pub fn new() -> Self {
        Self {
            tokens: f64::MAX,
            last_check: Instant::now(),
            consecutive_violations: 0,
        }
    }

    /// Reset consecutive violations to 0. Called by handlers when flood check
    /// is skipped (rate=0 or chat_unlimited).
    pub fn reset_violations(&mut self) {
        self.consecutive_violations = 0;
    }

    /// Check if there are any outstanding consecutive violations.
    pub fn has_violations(&self) -> bool {
        self.consecutive_violations > 0
    }

    /// Run the token bucket check, consuming one token if available.
    pub fn check(&mut self, burst: u32, rate: u32) -> FloodCheck {
        let now = Instant::now();
        let elapsed_secs = now.duration_since(self.last_check).as_secs_f64();

        let refill_rate = rate as f64 / 60.0;
        let refill = elapsed_secs * refill_rate;
        self.tokens += refill;

        let capacity = if burst == 0 { 1.0 } else { burst as f64 };
        if self.tokens > capacity {
            self.tokens = capacity;
        }

        self.last_check = now;

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            self.consecutive_violations = 0;
            FloodCheck::Allowed
        } else {
            self.consecutive_violations += 1;
            if self.consecutive_violations >= MAX_FLOOD_VIOLATIONS {
                FloodCheck::Disconnect
            } else {
                let wait = if refill_rate > 0.0 {
                    ((1.0 - self.tokens) / refill_rate).ceil() as u32
                } else {
                    1
                };
                let wait_seconds = wait.max(1);
                FloodCheck::Limited {
                    wait_seconds,
                    violation: self.consecutive_violations,
                    max_violations: MAX_FLOOD_VIOLATIONS,
                }
            }
        }
    }

    #[cfg(test)]
    fn set_last_check_seconds_ago(&mut self, seconds: u64) {
        self.last_check = Instant::now() - std::time::Duration::from_secs(seconds);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flood_config_new_and_getters() {
        let config = FloodConfig::new(10, 30);
        assert_eq!(config.burst(), 10);
        assert_eq!(config.rate(), 30);
    }

    #[test]
    fn test_flood_config_set() {
        let config = FloodConfig::new(5, 20);
        config.set_burst(15);
        config.set_rate(60);
        assert_eq!(config.burst(), 15);
        assert_eq!(config.rate(), 60);
    }

    #[test]
    fn test_allowed_within_burst() {
        let mut tracker = FloodTracker::new();
        for _ in 0..5 {
            assert!(matches!(tracker.check(5, 20), FloodCheck::Allowed));
        }
    }

    #[test]
    fn test_limited_after_burst() {
        let mut tracker = FloodTracker::new();
        // Consume all 5 burst tokens.
        for _ in 0..5 {
            assert!(matches!(tracker.check(5, 20), FloodCheck::Allowed));
        }
        // 6th message should be limited.
        assert!(matches!(tracker.check(5, 20), FloodCheck::Limited { .. }));
    }

    #[test]
    fn test_refill_over_time() {
        let mut tracker = FloodTracker::new();
        // Consume all 5 burst tokens.
        for _ in 0..5 {
            assert!(matches!(tracker.check(5, 20), FloodCheck::Allowed));
        }
        // Should be limited now.
        assert!(matches!(tracker.check(5, 20), FloodCheck::Limited { .. }));

        // Simulate 10 seconds passing (rate=20/min => ~3.33 tokens refilled).
        tracker.set_last_check_seconds_ago(10);
        // Should be allowed again after refill.
        assert!(matches!(tracker.check(5, 20), FloodCheck::Allowed));
    }

    #[test]
    fn test_disconnect_after_max_violations() {
        let mut tracker = FloodTracker::new();
        // Consume the single burst token (burst=1 effectively via capacity).
        assert!(matches!(tracker.check(1, 20), FloodCheck::Allowed));

        // Next 2 should be Limited (violations 1 and 2).
        assert!(matches!(tracker.check(1, 20), FloodCheck::Limited { .. }));
        assert!(matches!(tracker.check(1, 20), FloodCheck::Limited { .. }));

        // 3rd violation should be Disconnect.
        assert!(matches!(tracker.check(1, 20), FloodCheck::Disconnect));
    }

    #[test]
    fn test_violations_reset_on_allowed() {
        let mut tracker = FloodTracker::new();
        // Use burst=1 so we hit limits quickly.
        assert!(matches!(tracker.check(1, 60), FloodCheck::Allowed));

        // Get 2 violations.
        assert!(matches!(tracker.check(1, 60), FloodCheck::Limited { .. }));
        assert!(matches!(tracker.check(1, 60), FloodCheck::Limited { .. }));
        assert_eq!(tracker.consecutive_violations, 2);

        // Wait for refill (10 seconds at 60/min = 10 tokens).
        tracker.set_last_check_seconds_ago(10);
        assert!(matches!(tracker.check(1, 60), FloodCheck::Allowed));
        assert_eq!(tracker.consecutive_violations, 0);

        // Get 2 more violations — should NOT disconnect because counter was reset.
        assert!(matches!(tracker.check(1, 60), FloodCheck::Limited { .. }));
        assert!(matches!(tracker.check(1, 60), FloodCheck::Limited { .. }));
        assert_eq!(tracker.consecutive_violations, 2);

        // 3rd consecutive violation after reset → Disconnect
        assert!(matches!(tracker.check(1, 60), FloodCheck::Disconnect));
    }

    #[test]
    fn test_burst_zero_capacity_one() {
        let mut tracker = FloodTracker::new();
        // burst=0 means capacity=1, so only 1 message allowed.
        assert!(matches!(tracker.check(0, 20), FloodCheck::Allowed));
        assert!(matches!(tracker.check(0, 20), FloodCheck::Limited { .. }));
    }

    #[test]
    fn test_wait_seconds_minimum_one() {
        let mut tracker = FloodTracker::new();
        // Exhaust tokens.
        assert!(matches!(tracker.check(1, 120), FloodCheck::Allowed));
        // With high rate, wait would be very short, but should be >= 1.
        match tracker.check(1, 120) {
            FloodCheck::Limited { wait_seconds, .. } => {
                assert!(wait_seconds >= 1, "wait_seconds was {wait_seconds}");
            }
            other => panic!(
                "Expected Limited, got {}",
                match other {
                    FloodCheck::Allowed => "Allowed",
                    FloodCheck::Disconnect => "Disconnect",
                    FloodCheck::Limited { .. } => unreachable!(),
                }
            ),
        }
    }

    #[test]
    fn test_tokens_capped_to_burst() {
        let mut tracker = FloodTracker::new();
        // First check caps tokens from f64::MAX to burst capacity (5).
        assert!(matches!(tracker.check(5, 20), FloodCheck::Allowed));
        // After first check: tokens = 5.0 - 1.0 = 4.0.

        // Wait a very long time.
        tracker.set_last_check_seconds_ago(3600);

        // Tokens should refill but cap at 5 (burst).
        assert!(matches!(tracker.check(5, 20), FloodCheck::Allowed));
        // After: tokens capped to 5.0, then -1.0 = 4.0.

        // We should get exactly 4 more allowed (total 5 from burst).
        for _ in 0..4 {
            assert!(matches!(tracker.check(5, 20), FloodCheck::Allowed));
        }
        // 6th consecutive (without time passing) should be limited.
        assert!(matches!(tracker.check(5, 20), FloodCheck::Limited { .. }));
    }

    #[test]
    fn test_reset_violations() {
        let mut tracker = FloodTracker::new();
        assert!(matches!(tracker.check(1, 20), FloodCheck::Allowed));
        assert!(matches!(tracker.check(1, 20), FloodCheck::Limited { .. }));
        assert!(tracker.has_violations());

        tracker.reset_violations();
        assert!(!tracker.has_violations());
        assert_eq!(tracker.consecutive_violations, 0);
    }

    #[test]
    fn test_has_violations() {
        let mut tracker = FloodTracker::new();
        assert!(!tracker.has_violations());

        // Consume burst token.
        assert!(matches!(tracker.check(1, 20), FloodCheck::Allowed));
        assert!(!tracker.has_violations());

        // Trigger a violation.
        assert!(matches!(tracker.check(1, 20), FloodCheck::Limited { .. }));
        assert!(tracker.has_violations());

        // Reset and verify.
        tracker.reset_violations();
        assert!(!tracker.has_violations());
    }

    #[test]
    fn test_burst_capacity_decrease_mid_session() {
        let mut tracker = FloodTracker::new();
        // First check with burst=5 caps tokens to 5, consume one → 4 remaining.
        assert!(matches!(tracker.check(5, 20), FloodCheck::Allowed));

        // Now check with burst=2 — tokens (4) should be capped to 2, consume one → 1 remaining.
        assert!(matches!(tracker.check(2, 20), FloodCheck::Allowed));

        // 1 token left, consume it.
        assert!(matches!(tracker.check(2, 20), FloodCheck::Allowed));

        // 0 tokens, should be limited.
        assert!(matches!(tracker.check(2, 20), FloodCheck::Limited { .. }));
    }

    #[test]
    fn test_wait_seconds_accuracy() {
        let mut tracker = FloodTracker::new();
        // burst=5, rate=20/min (refill = 0.333/sec)
        // Consume all 5 tokens.
        for _ in 0..5 {
            assert!(matches!(tracker.check(5, 20), FloodCheck::Allowed));
        }
        // tokens ≈ 0, need 1.0 token. At 0.333/sec → 3.0 seconds → ceil = 3.
        match tracker.check(5, 20) {
            FloodCheck::Limited {
                wait_seconds,
                violation,
                max_violations,
            } => {
                assert_eq!(
                    wait_seconds, 3,
                    "At rate=20/min, wait for 1 token should be 3 seconds"
                );
                assert_eq!(violation, 1, "Should be first violation");
                assert_eq!(max_violations, 3, "Max violations should be 3");
            }
            other => panic!(
                "Expected Limited, got {}",
                match other {
                    FloodCheck::Allowed => "Allowed",
                    FloodCheck::Disconnect => "Disconnect",
                    FloodCheck::Limited { .. } => unreachable!(),
                }
            ),
        }
    }
}
