use core::fmt;

/// Domain errors returned by [`crate::Capacity`], [`crate::RatePerSec`],
/// and [`crate::TokenBucket`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum QuotaError {
    /// Bucket capacity must be at least `1` token.
    ZeroCapacity,
    /// Refill rate must be at least `1` token per second.
    ZeroRate,
    /// Requested acquisition cost exceeds the bucket burst capacity and can never succeed.
    CostExceedsCapacity {
        /// Tokens requested by the caller.
        requested: u64,
        /// Configured maximum burst capacity of the bucket.
        capacity: u64,
    },
    /// Monotonic clock regressed below the bucket's most recent observation timestamp.
    ClockRegression {
        /// Last timestamp observed by the bucket in nanoseconds.
        last_ns: u64,
        /// Caller-supplied timestamp in nanoseconds.
        now_ns: u64,
    },
    /// Checked integer arithmetic overflowed `u64` or `u128` bounds.
    ArithmeticOverflow,
}

impl fmt::Display for QuotaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroCapacity => f.write_str("bucket capacity must be greater than zero"),
            Self::ZeroRate => f.write_str("refill rate must be greater than zero"),
            Self::CostExceedsCapacity {
                requested,
                capacity,
            } => write!(
                f,
                "requested cost ({requested}) exceeds bucket capacity ({capacity})"
            ),
            Self::ClockRegression { last_ns, now_ns } => write!(
                f,
                "monotonic clock regressed from {last_ns}ns to {now_ns}ns"
            ),
            Self::ArithmeticOverflow => f.write_str("checked quota arithmetic overflowed"),
        }
    }
}

impl core::error::Error for QuotaError {}
