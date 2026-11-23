use crate::QuotaError;

/// Validated non-zero token burst capacity (`>= 1`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Capacity(u64);

impl Capacity {
    /// Constructs a validated non-zero [`Capacity`].
    ///
    /// # Errors
    ///
    /// Returns [`QuotaError::ZeroCapacity`] when `tokens == 0`.
    pub const fn new(tokens: u64) -> Result<Self, QuotaError> {
        if tokens == 0 {
            Err(QuotaError::ZeroCapacity)
        } else {
            Ok(Self(tokens))
        }
    }

    /// Returns the raw non-zero token capacity.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Validated non-zero refill rate in whole tokens per second (`>= 1`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RatePerSec(u64);

impl RatePerSec {
    /// Constructs a validated non-zero [`RatePerSec`].
    ///
    /// # Errors
    ///
    /// Returns [`QuotaError::ZeroRate`] when `tokens_per_sec == 0`.
    pub const fn new(tokens_per_sec: u64) -> Result<Self, QuotaError> {
        if tokens_per_sec == 0 {
            Err(QuotaError::ZeroRate)
        } else {
            Ok(Self(tokens_per_sec))
        }
    }

    /// Returns the configured refill rate in tokens per second.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Whole tokens currently available or remaining in a [`crate::TokenBucket`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Remaining(u64);

impl Remaining {
    /// Wraps a whole-token balance (`0..=capacity`).
    #[must_use]
    pub const fn new(tokens: u64) -> Self {
        Self(tokens)
    }

    /// Returns the raw whole-token count.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Outcome of a valid [`crate::TokenBucket::try_acquire`] evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Decision {
    /// Request was granted and `cost` tokens were deducted.
    Allowed {
        /// Whole tokens remaining in the bucket immediately after deduction.
        remaining: Remaining,
    },
    /// Request was denied without deducting tokens.
    Denied {
        /// Whole tokens currently available at `now_ns`.
        available: Remaining,
        /// Earliest monotonic timestamp (in nanoseconds) at which `cost` tokens will be available.
        retry_at_ns: u64,
    },
}
