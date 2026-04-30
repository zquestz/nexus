//! Per-IP token-bucket rate limiter
//!
//! Bounds two classes of abuse independently:
//!
//! - **Connection rate** — every fresh TCP accept costs one token from
//!   the connecting peer's bucket. When the bucket is empty the daemon
//!   drops the connection at the framing layer (no response sent), per
//!   the protocol spec's recommendation.
//! - **Failed-auth rate** — only *failed* password verifications cost a
//!   token. Successful authentications never debit. Once the bucket is
//!   empty further attempts (correct or not) are rejected with a
//!   typed `error_kind: "rate_limited"` response, so an attacker who
//!   triggers the limit can't sneak through with a guess.
//!
//! Each limiter keeps its own per-IP `HashMap<IpAddr, Bucket>`. Buckets
//! refill at `capacity / 60` tokens per second so the sustained rate
//! is `capacity` events per minute and the burst is `capacity`.
//!
//! `capacity == 0` disables the limiter (always allows; never inserts
//! buckets) so operators can opt out by setting the corresponding
//! `--rate-*` flag to 0.
//!
//! ## Two-phase auth check
//!
//! For the auth-failure flow, callers use:
//!
//! 1. [`RateLimiter::check_only`] before verifying the password — if
//!    `Limited`, reject the request as rate-limited.
//! 2. If `Allowed`, verify the password.
//! 3. If verification *failed*, call [`RateLimiter::record_failure`] to
//!    debit one token. Successes don't debit.
//!
//! For the connection-rate flow there is just one phase:
//! [`RateLimiter::try_consume`] refills, checks, and debits in one shot.
//!
//! ## Memory bound
//!
//! Without cleanup the bucket map would grow without bound under a
//! disposable-IP attack. [`RateLimiter::gc`] sweeps idle entries whose
//! buckets have refilled to capacity and which haven't been touched in
//! `idle_ttl`. Call it from a periodic background task.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::constants::ERR_RATE_LIMITER_MUTEX_POISONED;

/// Outcome of a rate-limit check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RateCheck {
    /// The bucket had capacity; the action is allowed.
    Allowed,
    /// The bucket is empty; the action should be rate-limited.
    Limited,
}

/// Per-IP bucket state. `last_refill` is updated on every access (whether
/// or not a token was consumed) so `gc` can identify idle entries.
struct Bucket {
    tokens: f64,
    last_refill: Instant,
}

/// Per-IP token-bucket rate limiter.
///
/// Construct via [`RateLimiter::per_minute`]. `capacity == 0` makes
/// every check return [`RateCheck::Allowed`] without touching the map.
pub struct RateLimiter {
    buckets: Mutex<HashMap<IpAddr, Bucket>>,
    capacity: f64,
    refill_per_sec: f64,
}

impl RateLimiter {
    /// Build a limiter with `capacity` tokens of burst, refilling at
    /// `capacity` tokens per minute (`capacity / 60` per second).
    /// `capacity == 0` disables the limiter.
    #[must_use]
    pub fn per_minute(capacity: u32) -> Self {
        let cap = f64::from(capacity);
        Self {
            buckets: Mutex::new(HashMap::new()),
            capacity: cap,
            refill_per_sec: cap / 60.0,
        }
    }

    /// `true` when this limiter is disabled (capacity == 0).
    #[inline]
    fn disabled(&self) -> bool {
        self.capacity == 0.0
    }

    /// Refill the bucket and atomically consume one token.
    ///
    /// Returns `Allowed` if a token was consumed, `Limited` if the
    /// bucket was empty.
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

    /// Refill the bucket and check capacity *without* consuming.
    ///
    /// Pair with [`record_failure`](Self::record_failure) for two-phase
    /// flows where only failures should debit (e.g., auth attempts —
    /// successes don't burn tokens, failures do).
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

    /// Debit one token to record a failure event.
    ///
    /// Use after [`check_only`](Self::check_only) returned `Allowed`
    /// and the underlying check (e.g., password verification) actually
    /// failed. Saturates at zero.
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
            // Keep when not at full capacity (still draining) OR when
            // recently active (within `idle_ttl`).
            !(b.tokens >= self.capacity && idle)
        });
    }

    /// Apply elapsed-time refill to a single bucket. Caller already
    /// holds the map lock.
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
        // Touch ip(1) but don't drain (bucket stays at capacity-1 then refills).
        // Easier: drain it then let it refill.
        assert_eq!(rl.try_consume(ip(1)), RateCheck::Allowed);
        // Full refill at 1/min would take 60s; rather than sleep, force-set the
        // bucket to "full but very stale" by calling gc with zero TTL — the
        // bucket isn't full yet so it's preserved.
        rl.gc(Duration::ZERO);
        // After consume the bucket is at 0 tokens, so it's NOT at capacity →
        // gc should keep it (still draining/refilling).
        assert_eq!(rl.bucket_count(), 1);

        // Manually mark the bucket full and stale (simulates a long quiet
        // period after refill).
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
