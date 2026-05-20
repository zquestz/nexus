//! Per-IP token-bucket rate limiter.
//!
//! Buckets refill at `capacity / 60` tokens/sec: sustained rate
//! `capacity`/minute, burst `capacity`. `capacity == 0` disables the
//! limiter (always allows, never inserts buckets).
//!
//! Two usage modes:
//! - **Connection rate** — one-shot [`try_consume`](RateLimiter::try_consume)
//!   per TCP accept; over-limit connections are dropped at the framing
//!   layer with no response, per the protocol spec.
//! - **Failed-auth rate** — two-phase: [`check_only`](RateLimiter::check_only)
//!   before verifying (reject if `Limited`), then
//!   [`record_failure`](RateLimiter::record_failure) only on a *failed*
//!   verify. Successes never debit, so an attacker can't guess past the
//!   limit.
//!
//! [`gc`](RateLimiter::gc) bounds the bucket map under a disposable-IP
//! attack; call it from a periodic background task.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::constants::ERR_RATE_LIMITER_MUTEX_POISONED;

/// Outcome of a rate-limit check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RateCheck {
    Allowed,
    Limited,
}

/// Per-IP bucket. `last_refill` is bumped on every access (consumed or
/// not) so `gc` can identify idle entries.
struct Bucket {
    tokens: f64,
    last_refill: Instant,
}

/// Per-IP token-bucket rate limiter. Construct via
/// [`per_minute`](RateLimiter::per_minute).
pub struct RateLimiter {
    buckets: Mutex<HashMap<IpAddr, Bucket>>,
    capacity: f64,
    refill_per_sec: f64,
}

impl RateLimiter {
    /// Limiter with `capacity` burst, refilling `capacity / 60` tokens
    /// per second (`capacity`/minute). `capacity == 0` disables it.
    #[must_use]
    pub fn per_minute(capacity: u32) -> Self {
        let cap = f64::from(capacity);
        Self {
            buckets: Mutex::new(HashMap::new()),
            capacity: cap,
            refill_per_sec: cap / 60.0,
        }
    }

    #[inline]
    fn disabled(&self) -> bool {
        self.capacity == 0.0
    }

    /// Refill, then consume one token: `Allowed` if a token was taken,
    /// `Limited` if the bucket was empty.
    pub fn try_consume(&self, ip: IpAddr) -> RateCheck {
        if self.disabled() {
            return RateCheck::Allowed;
        }
        let now = Instant::now();
        let mut buckets = self.buckets.lock().expect(ERR_RATE_LIMITER_MUTEX_POISONED);
        let bucket = buckets.entry(ip).or_insert_with(|| Bucket {
            tokens: self.capacity,
            last_refill: now,
        });
        Self::refill(bucket, now, self.capacity, self.refill_per_sec);
        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            RateCheck::Allowed
        } else {
            RateCheck::Limited
        }
    }

    /// Refill, then check capacity *without* consuming. Pair with
    /// [`record_failure`](Self::record_failure) for failure-only debits.
    pub fn check_only(&self, ip: IpAddr) -> RateCheck {
        if self.disabled() {
            return RateCheck::Allowed;
        }
        let now = Instant::now();
        let mut buckets = self.buckets.lock().expect(ERR_RATE_LIMITER_MUTEX_POISONED);
        let bucket = buckets.entry(ip).or_insert_with(|| Bucket {
            tokens: self.capacity,
            last_refill: now,
        });
        Self::refill(bucket, now, self.capacity, self.refill_per_sec);
        if bucket.tokens >= 1.0 {
            RateCheck::Allowed
        } else {
            RateCheck::Limited
        }
    }

    /// Debit one token (saturating at zero) after a failed check, e.g.
    /// a `check_only`-allowed password verify that didn't verify.
    pub fn record_failure(&self, ip: IpAddr) {
        if self.disabled() {
            return;
        }
        let now = Instant::now();
        let mut buckets = self.buckets.lock().expect(ERR_RATE_LIMITER_MUTEX_POISONED);
        let bucket = buckets.entry(ip).or_insert_with(|| Bucket {
            tokens: self.capacity,
            last_refill: now,
        });
        Self::refill(bucket, now, self.capacity, self.refill_per_sec);
        bucket.tokens = (bucket.tokens - 1.0).max(0.0);
    }

    /// Drop entries that have refilled to capacity AND haven't been
    /// touched in `idle_ttl`. Bounds memory under a disposable-IP attack.
    pub fn gc(&self, idle_ttl: Duration) {
        if self.disabled() {
            return;
        }
        let now = Instant::now();
        let mut buckets = self.buckets.lock().expect(ERR_RATE_LIMITER_MUTEX_POISONED);
        buckets.retain(|_ip, b| {
            let idle = now.saturating_duration_since(b.last_refill) >= idle_ttl;
            // Drop only full-and-idle; keep still-draining or recently active.
            !(b.tokens >= self.capacity && idle)
        });
    }

    /// Apply elapsed-time refill to one bucket (caller holds the lock).
    fn refill(bucket: &mut Bucket, now: Instant, capacity: f64, refill_per_sec: f64) {
        let elapsed = now
            .saturating_duration_since(bucket.last_refill)
            .as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * refill_per_sec).min(capacity);
        bucket.last_refill = now;
    }

    #[cfg(test)]
    fn bucket_count(&self) -> usize {
        self.buckets
            .lock()
            .expect(ERR_RATE_LIMITER_MUTEX_POISONED)
            .len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;
    use std::thread;

    fn ip(n: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(192, 0, 2, n))
    }

    #[test]
    fn try_consume_allows_up_to_capacity() {
        let rl = RateLimiter::per_minute(3);
        assert_eq!(rl.try_consume(ip(1)), RateCheck::Allowed);
        assert_eq!(rl.try_consume(ip(1)), RateCheck::Allowed);
        assert_eq!(rl.try_consume(ip(1)), RateCheck::Allowed);
        assert_eq!(rl.try_consume(ip(1)), RateCheck::Limited);
    }

    #[test]
    fn separate_ips_have_independent_buckets() {
        let rl = RateLimiter::per_minute(1);
        assert_eq!(rl.try_consume(ip(1)), RateCheck::Allowed);
        assert_eq!(rl.try_consume(ip(1)), RateCheck::Limited);
        // ip(2) still has its own full bucket.
        assert_eq!(rl.try_consume(ip(2)), RateCheck::Allowed);
    }

    #[test]
    fn capacity_zero_means_unlimited() {
        let rl = RateLimiter::per_minute(0);
        for _ in 0..100 {
            assert_eq!(rl.try_consume(ip(1)), RateCheck::Allowed);
        }
        // Disabled limiter never inserts buckets.
        assert_eq!(rl.bucket_count(), 0);
    }

    #[test]
    fn check_only_does_not_consume() {
        let rl = RateLimiter::per_minute(1);
        assert_eq!(rl.check_only(ip(1)), RateCheck::Allowed);
        assert_eq!(rl.check_only(ip(1)), RateCheck::Allowed);
        // Token is still there for try_consume.
        assert_eq!(rl.try_consume(ip(1)), RateCheck::Allowed);
        assert_eq!(rl.try_consume(ip(1)), RateCheck::Limited);
    }

    #[test]
    fn record_failure_decrements_bucket() {
        let rl = RateLimiter::per_minute(2);
        // Two failures empty the bucket.
        rl.record_failure(ip(1));
        rl.record_failure(ip(1));
        assert_eq!(rl.check_only(ip(1)), RateCheck::Limited);
    }

    #[test]
    fn record_failure_saturates_at_zero() {
        let rl = RateLimiter::per_minute(1);
        // Many failures don't push the bucket negative.
        for _ in 0..10 {
            rl.record_failure(ip(1));
        }
        assert_eq!(rl.check_only(ip(1)), RateCheck::Limited);
    }

    #[test]
    fn refill_replenishes_over_time() {
        // capacity=60 → 1 token per second. Sleeping 1.2s adds at least
        // one whole token (loosely — wallclock granularity varies).
        let rl = RateLimiter::per_minute(60);
        for _ in 0..60 {
            assert_eq!(rl.try_consume(ip(1)), RateCheck::Allowed);
        }
        assert_eq!(rl.try_consume(ip(1)), RateCheck::Limited);
        thread::sleep(Duration::from_millis(1100));
        // Bucket should have refilled at least one token.
        assert_eq!(rl.try_consume(ip(1)), RateCheck::Allowed);
    }

    #[test]
    fn gc_removes_idle_full_buckets() {
        let rl = RateLimiter::per_minute(1);
        assert_eq!(rl.try_consume(ip(1)), RateCheck::Allowed);
        // At 0 tokens (not full) → gc keeps it even with zero TTL.
        rl.gc(Duration::ZERO);
        assert_eq!(rl.bucket_count(), 1);

        // Force the bucket full and stale (simulates a long quiet period).
        {
            let mut buckets = rl.buckets.lock().expect("mutex");
            let bucket = buckets.get_mut(&ip(1)).expect("present");
            bucket.tokens = rl.capacity;
            bucket.last_refill = Instant::now() - Duration::from_secs(120);
        }
        rl.gc(Duration::from_secs(60));
        assert_eq!(rl.bucket_count(), 0);
    }

    #[test]
    fn gc_keeps_recently_active_buckets() {
        let rl = RateLimiter::per_minute(1);
        rl.try_consume(ip(1)); // last_refill = now
        rl.gc(Duration::from_secs(60));
        // Recently active → kept.
        assert_eq!(rl.bucket_count(), 1);
    }

    #[test]
    fn gc_is_no_op_when_disabled() {
        let rl = RateLimiter::per_minute(0);
        rl.try_consume(ip(1));
        rl.gc(Duration::ZERO);
        assert_eq!(rl.bucket_count(), 0);
    }
}
