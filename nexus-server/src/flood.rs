//! Chat flood protection using a token bucket algorithm.
//!
//! `FloodConfig` is shared server-wide (via `Arc`) and configurable at runtime.
//! `FloodState` is shared across live sessions and keys buckets by visible user
//! identity, so multiple connections cannot multiply the configured limit.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::users::user::UserSession;

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

/// Visible-identity key for chat flood buckets.
///
/// Regular accounts share one bucket by immutable account id. Shared-account
/// sessions are visible as distinct nicknames, so they use folded nickname keys.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum FloodKey {
    UserId(i64),
    Nickname(String),
}

impl FloodKey {
    pub(crate) fn for_session(session: &UserSession) -> Self {
        if session.is_shared {
            Self::Nickname(session.nickname_folded.clone())
        } else {
            Self::UserId(session.user_id)
        }
    }
}

/// Shared flood tracking state using one token bucket per visible identity.
#[derive(Debug, Clone, Default)]
pub struct FloodState {
    trackers: Arc<Mutex<HashMap<FloodKey, FloodTracker>>>,
}

impl FloodState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Run the flood check for `key`.
    ///
    /// If flood protection is disabled or the user has `chat_unlimited`, any
    /// outstanding violation count for the key is reset without creating a new
    /// tracker.
    pub(crate) fn check(
        &self,
        key: FloodKey,
        config: &FloodConfig,
        has_unlimited: bool,
        now: Instant,
    ) -> FloodCheck {
        let rate = config.rate();
        let mut trackers = self
            .trackers
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        if rate == 0 || has_unlimited {
            if let Some(tracker) = trackers.get_mut(&key)
                && tracker.has_violations()
            {
                tracker.reset_violations();
            }
            return FloodCheck::Allowed;
        }

        let burst = config.burst();
        trackers.entry(key).or_default().check(burst, rate, now)
    }

    /// Prune buckets for removed sessions whose visible identity no longer has
    /// any live session. Caller holds the users write lock and must not call this
    /// while holding the flood lock.
    pub(crate) fn prune_removed_sessions<'a>(
        &self,
        removed: impl IntoIterator<Item = &'a UserSession>,
        live_sessions: impl IntoIterator<Item = &'a UserSession>,
    ) {
        let removed_keys: HashSet<FloodKey> =
            removed.into_iter().map(FloodKey::for_session).collect();
        if removed_keys.is_empty() {
            return;
        }

        let live_keys: HashSet<FloodKey> = live_sessions
            .into_iter()
            .map(FloodKey::for_session)
            .collect();
        let mut trackers = self
            .trackers
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        for key in removed_keys {
            if !live_keys.contains(&key) {
                trackers.remove(&key);
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn contains_key_for_test(&self, key: &FloodKey) -> bool {
        self.trackers
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .contains_key(key)
    }

    #[cfg(test)]
    pub(crate) fn tracker_count_for_test(&self) -> usize {
        self.trackers
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .len()
    }
}

/// Per-identity flood tracking state using a token bucket.
#[derive(Debug)]
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
    ///
    /// `now` is threaded in (rather than read internally) so tests can drive
    /// time forward without subtracting from `Instant::now()`, which
    /// underflows on Windows early in process life.
    pub fn check(&mut self, burst: u32, rate: u32, now: Instant) -> FloodCheck {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

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
        let now = Instant::now();
        for _ in 0..5 {
            assert!(matches!(tracker.check(5, 20, now), FloodCheck::Allowed));
        }
    }

    #[test]
    fn test_limited_after_burst() {
        let mut tracker = FloodTracker::new();
        let now = Instant::now();
        for _ in 0..5 {
            assert!(matches!(tracker.check(5, 20, now), FloodCheck::Allowed));
        }
        assert!(matches!(
            tracker.check(5, 20, now),
            FloodCheck::Limited { .. }
        ));
    }

    #[test]
    fn test_refill_over_time() {
        let mut tracker = FloodTracker::new();
        let base = Instant::now();
        for _ in 0..5 {
            assert!(matches!(tracker.check(5, 20, base), FloodCheck::Allowed));
        }
        assert!(matches!(
            tracker.check(5, 20, base),
            FloodCheck::Limited { .. }
        ));

        // 10s at rate=20/min refills ~3.33 tokens, enough for one more.
        let later = base + Duration::from_secs(10);
        assert!(matches!(tracker.check(5, 20, later), FloodCheck::Allowed));
    }

    #[test]
    fn test_disconnect_after_max_violations() {
        let mut tracker = FloodTracker::new();
        let now = Instant::now();
        assert!(matches!(tracker.check(1, 20, now), FloodCheck::Allowed));

        // Violations 1 and 2 are Limited; the 3rd hits MAX → Disconnect.
        assert!(matches!(
            tracker.check(1, 20, now),
            FloodCheck::Limited { .. }
        ));
        assert!(matches!(
            tracker.check(1, 20, now),
            FloodCheck::Limited { .. }
        ));

        assert!(matches!(tracker.check(1, 20, now), FloodCheck::Disconnect));
    }

    #[test]
    fn test_violations_reset_on_allowed() {
        let mut tracker = FloodTracker::new();
        let base = Instant::now();
        // burst=1 to hit limits quickly.
        assert!(matches!(tracker.check(1, 60, base), FloodCheck::Allowed));

        assert!(matches!(
            tracker.check(1, 60, base),
            FloodCheck::Limited { .. }
        ));
        assert!(matches!(
            tracker.check(1, 60, base),
            FloodCheck::Limited { .. }
        ));
        assert_eq!(tracker.consecutive_violations, 2);

        // 10s at 60/min refills 10 tokens; an Allowed message resets the counter.
        let later = base + Duration::from_secs(10);
        assert!(matches!(tracker.check(1, 60, later), FloodCheck::Allowed));
        assert_eq!(tracker.consecutive_violations, 0);

        // 2 more violations must NOT disconnect — counter was reset.
        assert!(matches!(
            tracker.check(1, 60, later),
            FloodCheck::Limited { .. }
        ));
        assert!(matches!(
            tracker.check(1, 60, later),
            FloodCheck::Limited { .. }
        ));
        assert_eq!(tracker.consecutive_violations, 2);

        assert!(matches!(
            tracker.check(1, 60, later),
            FloodCheck::Disconnect
        ));
    }

    #[test]
    fn test_burst_zero_capacity_one() {
        let mut tracker = FloodTracker::new();
        let now = Instant::now();
        // burst=0 means capacity=1, so only 1 message allowed.
        assert!(matches!(tracker.check(0, 20, now), FloodCheck::Allowed));
        assert!(matches!(
            tracker.check(0, 20, now),
            FloodCheck::Limited { .. }
        ));
    }

    #[test]
    fn test_wait_seconds_minimum_one() {
        let mut tracker = FloodTracker::new();
        let now = Instant::now();
        assert!(matches!(tracker.check(1, 120, now), FloodCheck::Allowed));
        // High rate → sub-second wait, but the result is floored at 1.
        match tracker.check(1, 120, now) {
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
        let base = Instant::now();
        // First check caps tokens from f64::MAX to burst (5), then consumes 1 → 4.
        assert!(matches!(tracker.check(5, 20, base), FloodCheck::Allowed));

        // After an hour, refill is still capped at burst (5), not unbounded.
        let later = base + Duration::from_secs(3600);
        assert!(matches!(tracker.check(5, 20, later), FloodCheck::Allowed));

        // Exactly 4 more (5 total) before the bucket empties.
        for _ in 0..4 {
            assert!(matches!(tracker.check(5, 20, later), FloodCheck::Allowed));
        }
        assert!(matches!(
            tracker.check(5, 20, later),
            FloodCheck::Limited { .. }
        ));
    }

    #[test]
    fn test_reset_violations() {
        let mut tracker = FloodTracker::new();
        let now = Instant::now();
        assert!(matches!(tracker.check(1, 20, now), FloodCheck::Allowed));
        assert!(matches!(
            tracker.check(1, 20, now),
            FloodCheck::Limited { .. }
        ));
        assert!(tracker.has_violations());

        tracker.reset_violations();
        assert!(!tracker.has_violations());
        assert_eq!(tracker.consecutive_violations, 0);
    }

    #[test]
    fn test_has_violations() {
        let mut tracker = FloodTracker::new();
        let now = Instant::now();
        assert!(!tracker.has_violations());

        assert!(matches!(tracker.check(1, 20, now), FloodCheck::Allowed));
        assert!(!tracker.has_violations());

        assert!(matches!(
            tracker.check(1, 20, now),
            FloodCheck::Limited { .. }
        ));
        assert!(tracker.has_violations());

        tracker.reset_violations();
        assert!(!tracker.has_violations());
    }

    #[test]
    fn test_burst_capacity_decrease_mid_session() {
        let mut tracker = FloodTracker::new();
        let now = Instant::now();
        // First check with burst=5 caps tokens to 5, consume one → 4 remaining.
        assert!(matches!(tracker.check(5, 20, now), FloodCheck::Allowed));

        // Now check with burst=2 — tokens (4) should be capped to 2, consume one → 1 remaining.
        assert!(matches!(tracker.check(2, 20, now), FloodCheck::Allowed));

        // 1 token left, consume it.
        assert!(matches!(tracker.check(2, 20, now), FloodCheck::Allowed));

        // 0 tokens, should be limited.
        assert!(matches!(
            tracker.check(2, 20, now),
            FloodCheck::Limited { .. }
        ));
    }

    #[test]
    fn test_wait_seconds_accuracy() {
        let mut tracker = FloodTracker::new();
        let now = Instant::now();
        // burst=5, rate=20/min (refill = 0.333/sec)
        // Consume all 5 tokens.
        for _ in 0..5 {
            assert!(matches!(tracker.check(5, 20, now), FloodCheck::Allowed));
        }
        // tokens ≈ 0, need 1.0 token. At 0.333/sec → 3.0 seconds → ceil = 3.
        match tracker.check(5, 20, now) {
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
