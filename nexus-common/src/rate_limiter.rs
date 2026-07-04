//! Per-IP token-bucket rate limiter.
//!
//! Buckets hold `burst` tokens and refill at `refill_per_minute / 60`
//! tokens/sec: burst `burst`, sustained rate `refill_per_minute`/minute.
//! `burst == 0` disables the limiter (always allows, never inserts
//! buckets).
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
//! [`key_ipv6_by_prefix`](RateLimiter::key_ipv6_by_prefix) buckets IPv6
//! addresses by their /64 prefix — attackers trivially hold an entire
//! /64, so per-address buckets are no obstacle there.
//!
//! [`gc`](RateLimiter::gc) bounds the bucket map under a disposable-IP
//! attack; call it from a periodic background task.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const ERR_RATE_LIMITER_MUTEX_POISONED: &str = "rate limiter mutex poisoned";

use crate::address::ipv6_slash_64_bucket_key;

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
/// [`per_minute`](RateLimiter::per_minute) (burst == sustained rate) or
/// [`with_burst_and_refill`](RateLimiter::with_burst_and_refill).
pub struct RateLimiter {
    buckets: Mutex<HashMap<IpAddr, Bucket>>,
    capacity: f64,
    refill_per_sec: f64,
    ipv6_prefix_keying: bool,
}

impl RateLimiter {
    /// Limiter with `capacity` burst, refilling `capacity / 60` tokens
    /// per second (`capacity`/minute). `capacity == 0` disables it.
    #[must_use]
    pub fn per_minute(capacity: u32) -> Self {
        Self::with_burst_and_refill(capacity, capacity)
    }

    /// Limiter with `burst` tokens, refilling `refill_per_minute / 60`
    /// tokens per second. `burst == 0` disables it.
    #[must_use]
    pub fn with_burst_and_refill(burst: u32, refill_per_minute: u32) -> Self {
        Self {
            buckets: Mutex::new(HashMap::new()),
            capacity: f64::from(burst),
            refill_per_sec: f64::from(refill_per_minute) / 60.0,
            ipv6_prefix_keying: false,
        }
    }

    /// Bucket IPv6 addresses by their /64 prefix instead of per-address.
    /// IPv4 addresses are unaffected.
    #[must_use]
    pub fn key_ipv6_by_prefix(mut self) -> Self {
        self.ipv6_prefix_keying = true;
        self
    }

    #[inline]
    fn disabled(&self) -> bool {
        self.capacity == 0.0
    }

    /// Map an address to its bucket key (IPv6 /64 prefix when enabled).
    fn key(&self, ip: IpAddr) -> IpAddr {
        if self.ipv6_prefix_keying {
            ipv6_slash_64_bucket_key(ip)
        } else {
            ip
        }
    }

    /// Refill, then consume one token: `Allowed` if a token was taken,
    /// `Limited` if the bucket was empty.
    pub fn try_consume(&self, ip: IpAddr) -> RateCheck {
        if self.disabled() {
            return RateCheck::Allowed;
        }
        let key = self.key(ip);
        let now = Instant::now();
        let mut buckets = self.buckets.lock().expect(ERR_RATE_LIMITER_MUTEX_POISONED);
        let bucket = buckets.entry(key).or_insert_with(|| Bucket {
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
        let key = self.key(ip);
        let now = Instant::now();
        let mut buckets = self.buckets.lock().expect(ERR_RATE_LIMITER_MUTEX_POISONED);
        let bucket = buckets.entry(key).or_insert_with(|| Bucket {
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
        let key = self.key(ip);
        let now = Instant::now();
        let mut buckets = self.buckets.lock().expect(ERR_RATE_LIMITER_MUTEX_POISONED);
        let bucket = buckets.entry(key).or_insert_with(|| Bucket {
            tokens: self.capacity,
            last_refill: now,
        });
        Self::refill(bucket, now, self.capacity, self.refill_per_sec);
        bucket.tokens = (bucket.tokens - 1.0).max(0.0);
    }

    /// Drop entries that have (or would have) refilled to capacity AND
    /// haven't been touched in `idle_ttl`. Bounds memory under a
    /// disposable-IP attack.
    ///
    /// Tokens are only materialized on access, so an abandoned bucket's
    /// stored count understates it; the full-and-idle decision uses the
    /// elapsed-time projection instead. A zero-refill limiter
    /// (`with_burst_and_refill(n, 0)`) never projects to full, so its
    /// drained buckets are deliberately retained — a never-refilling
    /// penalty is meaningful state, not garbage.
    pub fn gc(&self, idle_ttl: Duration) {
        if self.disabled() {
            return;
        }
        let now = Instant::now();
        let mut buckets = self.buckets.lock().expect(ERR_RATE_LIMITER_MUTEX_POISONED);
        buckets.retain(|_ip, b| {
            let idle_secs = now.saturating_duration_since(b.last_refill).as_secs_f64();
            let idle = idle_secs >= idle_ttl.as_secs_f64();
            let projected = (b.tokens + idle_secs * self.refill_per_sec).min(self.capacity);
            // Drop only full-and-idle; keep still-draining or recently active.
            !(projected >= self.capacity && idle)
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
    fn burst_and_refill_are_independent() {
        // burst=2 with a 60/min refill (1 token/sec): two quick failures
        // exhaust the bucket, and ~1.1s restores roughly one token, not
        // the full burst.
        let rl = RateLimiter::with_burst_and_refill(2, 60);
        rl.record_failure(ip(1));
        rl.record_failure(ip(1));
        assert_eq!(rl.check_only(ip(1)), RateCheck::Limited);
        let before_sleep = std::time::Instant::now();
        thread::sleep(Duration::from_millis(1100));
        let slept = before_sleep.elapsed();
        assert_eq!(rl.try_consume(ip(1)), RateCheck::Allowed);
        // Proves refill restored ~one token rather than the full burst.
        // Oversleep on a loaded host legitimately refills a second token
        // (1 token/sec), so only assert inside the single-token window.
        if slept < Duration::from_millis(1900) {
            assert_eq!(rl.try_consume(ip(1)), RateCheck::Limited);
        }
    }

    #[test]
    fn burst_zero_disables_with_burst_and_refill() {
        let rl = RateLimiter::with_burst_and_refill(0, 60);
        for _ in 0..10 {
            assert_eq!(rl.try_consume(ip(1)), RateCheck::Allowed);
        }
        assert_eq!(rl.bucket_count(), 0);
    }

    #[test]
    fn ipv6_prefix_keying_shares_a_slash_64() {
        let rl = RateLimiter::per_minute(1).key_ipv6_by_prefix();
        let a: IpAddr = "2001:db8:1:1::1".parse().expect("valid IPv6");
        let b: IpAddr = "2001:db8:1:1:ffff:ffff:ffff:ffff"
            .parse()
            .expect("valid IPv6");
        let other_prefix: IpAddr = "2001:db8:1:2::1".parse().expect("valid IPv6");

        assert_eq!(rl.try_consume(a), RateCheck::Allowed);
        // Same /64 → same bucket → limited.
        assert_eq!(rl.try_consume(b), RateCheck::Limited);
        // Different /64 → independent bucket.
        assert_eq!(rl.try_consume(other_prefix), RateCheck::Allowed);
        // IPv4 stays per-address.
        assert_eq!(rl.try_consume(ip(1)), RateCheck::Allowed);
        assert_eq!(rl.try_consume(ip(2)), RateCheck::Allowed);
    }

    #[test]
    fn ipv6_per_address_without_prefix_keying() {
        let rl = RateLimiter::per_minute(1);
        let a: IpAddr = "2001:db8:1:1::1".parse().expect("valid IPv6");
        let b: IpAddr = "2001:db8:1:1::2".parse().expect("valid IPv6");
        assert_eq!(rl.try_consume(a), RateCheck::Allowed);
        // Same /64 but prefix keying is off → independent buckets.
        assert_eq!(rl.try_consume(b), RateCheck::Allowed);
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
    fn gc_removes_abandoned_drained_buckets() {
        // The disposable-IP attack shape: one failure, never touched again.
        // The stored token count stays below capacity (refill only happens
        // on access), so gc must project the refill from elapsed time.
        let rl = RateLimiter::per_minute(60);
        rl.record_failure(ip(1));
        {
            let mut buckets = rl.buckets.lock().expect("mutex");
            let bucket = buckets.get_mut(&ip(1)).expect("present");
            // 120s idle at 1 token/sec projects well past full capacity.
            bucket.last_refill = Instant::now() - Duration::from_secs(120);
        }
        rl.gc(Duration::from_secs(60));
        assert_eq!(rl.bucket_count(), 0);
    }

    #[test]
    fn gc_keeps_idle_buckets_that_have_not_refilled() {
        // burst 10, refill 1/min: after 9 failures, 120s only restores ~2
        // tokens — the projection is below capacity, so the penalty state
        // must survive gc even though the bucket is idle.
        let rl = RateLimiter::with_burst_and_refill(10, 1);
        for _ in 0..9 {
            rl.record_failure(ip(1));
        }
        {
            let mut buckets = rl.buckets.lock().expect("mutex");
            let bucket = buckets.get_mut(&ip(1)).expect("present");
            bucket.last_refill = Instant::now() - Duration::from_secs(120);
        }
        rl.gc(Duration::from_secs(60));
        assert_eq!(rl.bucket_count(), 1);
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
