use crate::{Capacity, Decision, QuotaError, RatePerSec, Remaining};

const NANOS_PER_SEC: u128 = 1_000_000_000;

/// Alias preserving downstream ergonomics for [`TokenBucket`].
pub type TokenBucketQuota = TokenBucket;

/// Deterministic integer-nanosecond token-bucket rate limiter.
///
/// Tracks sub-token refill credit in exact nano-tokens (`< 1_000_000_000`) so
/// arbitrary nanosecond polling intervals neither lose credit to rounding nor
/// exceed the configured [`Capacity`] burst cap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TokenBucket {
    capacity: Capacity,
    rate: RatePerSec,
    tokens: u64,
    sub_token_nanos: u64,
    last_ns: u64,
}

impl TokenBucket {
    /// Creates a full [`TokenBucket`] initialized at `initial_ns`.
    #[must_use]
    pub const fn new(capacity: Capacity, rate: RatePerSec, initial_ns: u64) -> Self {
        Self {
            capacity,
            rate,
            tokens: capacity.get(),
            sub_token_nanos: 0,
            last_ns: initial_ns,
        }
    }

    /// Returns the configured maximum burst [`Capacity`].
    #[must_use]
    pub const fn capacity(&self) -> Capacity {
        self.capacity
    }

    /// Returns the configured [`RatePerSec`].
    #[must_use]
    pub const fn rate(&self) -> RatePerSec {
        self.rate
    }

    /// Returns the most recent monotonic timestamp observed by this bucket.
    #[must_use]
    pub const fn last_ns(&self) -> u64 {
        self.last_ns
    }

    /// Computes whole tokens available at `now_ns` as a [`Remaining`] value without mutating state.
    ///
    /// # Errors
    ///
    /// Returns [`QuotaError::ClockRegression`] if `now_ns < self.last_ns()`, or
    /// [`QuotaError::ArithmeticOverflow`] if refill math overflows.
    pub fn available_remaining(&self, now_ns: u64) -> Result<Remaining, QuotaError> {
        let (tokens, _) = self.project(now_ns)?;
        Ok(Remaining::new(tokens))
    }

    /// Computes raw whole tokens available at `now_ns` without mutating state.
    ///
    /// # Errors
    ///
    /// Returns [`QuotaError::ClockRegression`] if `now_ns < self.last_ns()`, or
    /// [`QuotaError::ArithmeticOverflow`] if refill math overflows.
    pub fn available_tokens(&self, now_ns: u64) -> Result<u64, QuotaError> {
        Ok(self.available_remaining(now_ns)?.get())
    }

    /// Attempts to acquire `cost` tokens at `now_ns`, advancing bucket refill state.
    ///
    /// If insufficient tokens are available, no tokens are deducted and
    /// [`Decision::Denied`] reports the exact `retry_at_ns` timestamp.
    ///
    /// # Errors
    ///
    /// Returns [`QuotaError::CostExceedsCapacity`] if `cost > self.capacity().get()`,
    /// [`QuotaError::ClockRegression`] if `now_ns < self.last_ns()`, or
    /// [`QuotaError::ArithmeticOverflow`] if `retry_at_ns` overflows `u64`.
    pub fn try_acquire(&mut self, cost: u64, now_ns: u64) -> Result<Decision, QuotaError> {
        if cost > self.capacity.get() {
            return Err(QuotaError::CostExceedsCapacity {
                requested: cost,
                capacity: self.capacity.get(),
            });
        }
        let (tokens, sub_nanos) = self.project(now_ns)?;
        self.tokens = tokens;
        self.sub_token_nanos = sub_nanos;
        self.last_ns = now_ns;

        if self.tokens >= cost {
            self.tokens -= cost;
            Ok(Decision::Allowed {
                remaining: Remaining::new(self.tokens),
            })
        } else {
            let retry_at_ns =
                Self::compute_ready_ns(self.tokens, self.sub_token_nanos, self.rate, cost, now_ns)?;
            Ok(Decision::Denied {
                available: Remaining::new(self.tokens),
                retry_at_ns,
            })
        }
    }

    /// Returns the earliest monotonic timestamp `>= now_ns` at which `cost` tokens
    /// will be available, without mutating bucket state.
    ///
    /// # Errors
    ///
    /// Returns [`QuotaError::CostExceedsCapacity`] if `cost > self.capacity().get()`,
    /// [`QuotaError::ClockRegression`] if `now_ns < self.last_ns()`, or
    /// [`QuotaError::ArithmeticOverflow`] if the resulting timestamp exceeds `u64::MAX`.
    pub fn available_at(&self, cost: u64, now_ns: u64) -> Result<u64, QuotaError> {
        if cost > self.capacity.get() {
            return Err(QuotaError::CostExceedsCapacity {
                requested: cost,
                capacity: self.capacity.get(),
            });
        }
        let (tokens, sub_nanos) = self.project(now_ns)?;
        if tokens >= cost {
            return Ok(now_ns);
        }
        Self::compute_ready_ns(tokens, sub_nanos, self.rate, cost, now_ns)
    }

    fn project(&self, now_ns: u64) -> Result<(u64, u64), QuotaError> {
        let elapsed_ns = now_ns
            .checked_sub(self.last_ns)
            .ok_or(QuotaError::ClockRegression {
                last_ns: self.last_ns,
                now_ns,
            })?;
        if elapsed_ns == 0 || self.tokens == self.capacity.get() {
            let clamped_sub = if self.tokens == self.capacity.get() {
                0
            } else {
                self.sub_token_nanos
            };
            return Ok((self.tokens, clamped_sub));
        }

        let produced_nano_tokens = u128::from(elapsed_ns)
            .checked_mul(u128::from(self.rate.get()))
            .and_then(|v| v.checked_add(u128::from(self.sub_token_nanos)))
            .ok_or(QuotaError::ArithmeticOverflow)?;

        let whole_added = produced_nano_tokens / NANOS_PER_SEC;
        let rem_nanos = (produced_nano_tokens % NANOS_PER_SEC) as u64;

        let cap = u128::from(self.capacity.get());
        let updated = u128::from(self.tokens)
            .checked_add(whole_added)
            .ok_or(QuotaError::ArithmeticOverflow)?;

        if updated >= cap {
            Ok((self.capacity.get(), 0))
        } else {
            Ok((updated as u64, rem_nanos))
        }
    }

    fn compute_ready_ns(
        tokens: u64,
        sub_nanos: u64,
        rate: RatePerSec,
        cost: u64,
        now_ns: u64,
    ) -> Result<u64, QuotaError> {
        let deficit_tokens = cost.saturating_sub(tokens);
        if deficit_tokens == 0 {
            return Ok(now_ns);
        }
        let needed_nano_tokens = u128::from(deficit_tokens)
            .checked_mul(NANOS_PER_SEC)
            .and_then(|v| v.checked_sub(u128::from(sub_nanos)))
            .ok_or(QuotaError::ArithmeticOverflow)?;

        let rate_u128 = u128::from(rate.get());
        let wait_ns_u128 = needed_nano_tokens
            .checked_add(rate_u128 - 1)
            .map(|num| num / rate_u128)
            .ok_or(QuotaError::ArithmeticOverflow)?;

        let wait_ns = u64::try_from(wait_ns_u128).map_err(|_| QuotaError::ArithmeticOverflow)?;
        now_ns
            .checked_add(wait_ns)
            .ok_or(QuotaError::ArithmeticOverflow)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sub_nanosecond_credit_accumulates_without_loss() {
        let cap = Capacity::new(10).unwrap();
        let rate = RatePerSec::new(10).unwrap();
        let mut bucket = TokenBucket::new(cap, rate, 0);
        assert_eq!(
            bucket.try_acquire(10, 0).unwrap(),
            Decision::Allowed {
                remaining: Remaining::new(0)
            }
        );
        assert_eq!(
            bucket.try_acquire(1, 60_000_000).unwrap(),
            Decision::Denied {
                available: Remaining::new(0),
                retry_at_ns: 100_000_000
            }
        );
        assert_eq!(
            bucket.try_acquire(1, 120_000_000).unwrap(),
            Decision::Allowed {
                remaining: Remaining::new(0)
            }
        );
        assert_eq!(bucket.available_at(1, 120_000_000).unwrap(), 200_000_000);
    }
}
