//! Downstream `#![no_std]` SemVer & trait-bound consumer contract test (`H5` / `H9`).
//!
//! Fails at compile time if any promised trait implementation (`Copy`, `Clone`,
//! `Eq`, `PartialEq`, `Ord`, `PartialOrd`, `Hash`, `Debug`, `Display`, `Error`),
//! exhaustive enum variant set (`Decision`, `QuotaError`), or public method/const-fn
//! signature on `quota-core` is removed or altered.

#![no_std]
#![forbid(unsafe_code)]

use quota_core::{
    Capacity, Decision, QuotaError, RatePerSec, Remaining, TokenBucket, TokenBucketQuota,
};

const fn assert_value_contract<T>()
where
    T: Copy + Clone + Eq + PartialEq + Ord + PartialOrd + core::hash::Hash + core::fmt::Debug,
{
}

const fn assert_error_contract<T>()
where
    T: core::fmt::Display + core::error::Error,
{
}

const _: () = {
    assert_value_contract::<Capacity>();
    assert_value_contract::<RatePerSec>();
    assert_value_contract::<Remaining>();
    assert_value_contract::<Decision>();
    assert_value_contract::<QuotaError>();
    assert_value_contract::<TokenBucket>();
    assert_value_contract::<TokenBucketQuota>();
    assert_error_contract::<QuotaError>();
};

const fn exhaustive_decision_score(decision: Decision) -> u64 {
    match decision {
        Decision::Allowed { remaining } => remaining.get(),
        Decision::Denied {
            available,
            retry_at_ns,
        } => available.get().saturating_add(retry_at_ns),
    }
}

const fn exhaustive_error_code(err: QuotaError) -> u8 {
    match err {
        QuotaError::ZeroCapacity => 1,
        QuotaError::ZeroRate => 2,
        QuotaError::CostExceedsCapacity {
            requested,
            capacity,
        } => (requested ^ capacity) as u8,
        QuotaError::ClockRegression { last_ns, now_ns } => (last_ns ^ now_ns) as u8,
        QuotaError::ArithmeticOverflow => 5,
    }
}

#[test]
fn locks_const_constructors_exhaustive_enums_and_method_signatures() {
    const CAP: Result<Capacity, QuotaError> = Capacity::new(8);
    const RATE: Result<RatePerSec, QuotaError> = RatePerSec::new(4);
    const REM: Remaining = Remaining::new(3);
    assert_eq!(REM.get(), 3);

    let cap_new_fn: fn(u64) -> Result<Capacity, QuotaError> = Capacity::new;
    let cap_get_fn: fn(Capacity) -> u64 = Capacity::get;
    let rate_new_fn: fn(u64) -> Result<RatePerSec, QuotaError> = RatePerSec::new;
    let rate_get_fn: fn(RatePerSec) -> u64 = RatePerSec::get;
    let rem_new_fn: fn(u64) -> Remaining = Remaining::new;
    let rem_get_fn: fn(Remaining) -> u64 = Remaining::get;

    let bucket_new_fn: fn(Capacity, RatePerSec, u64) -> TokenBucket = TokenBucket::new;
    let bucket_cap_fn: fn(&TokenBucket) -> Capacity = TokenBucket::capacity;
    let bucket_rate_fn: fn(&TokenBucket) -> RatePerSec = TokenBucket::rate;
    let bucket_last_fn: fn(&TokenBucket) -> u64 = TokenBucket::last_ns;
    let bucket_rem_fn: fn(&TokenBucket, u64) -> Result<Remaining, QuotaError> =
        TokenBucket::available_remaining;
    let bucket_tok_fn: fn(&TokenBucket, u64) -> Result<u64, QuotaError> =
        TokenBucket::available_tokens;
    let bucket_acq_fn: fn(&mut TokenBucket, u64, u64) -> Result<Decision, QuotaError> =
        TokenBucket::try_acquire;
    let bucket_at_fn: fn(&TokenBucket, u64, u64) -> Result<u64, QuotaError> =
        TokenBucket::available_at;

    let cap = cap_new_fn(CAP.unwrap().get()).unwrap();
    let rate = rate_new_fn(RATE.unwrap().get()).unwrap();
    assert_eq!(cap_get_fn(cap), 8);
    assert_eq!(rate_get_fn(rate), 4);
    assert_eq!(rem_get_fn(rem_new_fn(5)), 5);

    let mut bucket: TokenBucketQuota = bucket_new_fn(cap, rate, 100);
    assert_eq!(bucket_cap_fn(&bucket), cap);
    assert_eq!(bucket_rate_fn(&bucket), rate);
    assert_eq!(bucket_last_fn(&bucket), 100);
    assert_eq!(bucket_rem_fn(&bucket, 100), Ok(Remaining::new(8)));
    assert_eq!(bucket_tok_fn(&bucket, 100), Ok(8));

    let allowed = bucket_acq_fn(&mut bucket, 3, 100).unwrap();
    assert_eq!(exhaustive_decision_score(allowed), 5);
    assert_eq!(exhaustive_error_code(QuotaError::ZeroCapacity), 1);
    assert_eq!(bucket_at_fn(&bucket, 5, 100), Ok(100));
}
