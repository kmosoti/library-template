#![forbid(unsafe_code)]

use quota_core::{Capacity, Decision, QuotaError, RatePerSec, Remaining, TokenBucket};

#[test]
fn rejects_zero_capacity_and_zero_rate() {
    assert_eq!(Capacity::new(0), Err(QuotaError::ZeroCapacity));
    assert_eq!(RatePerSec::new(0), Err(QuotaError::ZeroRate));
    assert_eq!(Capacity::new(1).unwrap().get(), 1);
    assert_eq!(RatePerSec::new(1).unwrap().get(), 1);
}

#[test]
fn window_edge_burst_never_exceeds_capacity() {
    let cap = Capacity::new(5).unwrap();
    let rate = RatePerSec::new(100).unwrap();
    let mut quota = TokenBucket::new(cap, rate, 1_000);

    // Idle for 10 full seconds: burst must stay capped at 5 with zero hidden sub-token credit.
    let idle_ns = 10_000_001_000;
    assert_eq!(quota.available_tokens(idle_ns).unwrap(), 5);
    assert_eq!(
        quota.try_acquire(5, idle_ns).unwrap(),
        Decision::Allowed {
            remaining: Remaining::new(0)
        }
    );

    // Immediately 1ns after draining burst, no residual credit from idle period can leak through.
    assert_eq!(
        quota.try_acquire(1, idle_ns + 1).unwrap(),
        Decision::Denied {
            available: Remaining::new(0),
            retry_at_ns: idle_ns + 10_000_000,
        }
    );
}

#[test]
fn available_at_is_exact_minimal_ready_nanosecond() {
    let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next_u64 = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };

    for _ in 0..256 {
        let cap_val = (next_u64() % 64) + 1;
        let rate_val = (next_u64() % 10_000) + 1;
        let t0 = next_u64() % 1_000_000_000;
        let drain = (next_u64() % cap_val) + 1;
        let cost = (next_u64() % cap_val) + 1;
        let step_ns = next_u64() % 500_000_000;

        let mut quota = TokenBucket::new(
            Capacity::new(cap_val).unwrap(),
            RatePerSec::new(rate_val).unwrap(),
            t0,
        );
        assert!(matches!(
            quota.try_acquire(drain, t0).unwrap(),
            Decision::Allowed { .. }
        ));

        let query_ns = t0 + step_ns;
        let ready_ns = quota.available_at(cost, query_ns).unwrap();
        assert!(ready_ns >= query_ns);

        if ready_ns > query_ns {
            let mut before = quota;
            assert!(matches!(
                before.try_acquire(cost, ready_ns - 1).unwrap(),
                Decision::Denied { retry_at_ns, .. } if retry_at_ns == ready_ns
            ));
        }

        let mut at_ready = quota;
        assert!(matches!(
            at_ready.try_acquire(cost, ready_ns).unwrap(),
            Decision::Allowed { .. }
        ));
    }
}

#[test]
fn monotonic_refill_is_path_independent_below_cap() {
    let cap = Capacity::new(1_000).unwrap();
    let rate = RatePerSec::new(37).unwrap();

    let mut stepped = TokenBucket::new(cap, rate, 0);
    let mut direct = TokenBucket::new(cap, rate, 0);
    assert_eq!(
        stepped.try_acquire(1_000, 0).unwrap(),
        Decision::Allowed {
            remaining: Remaining::new(0)
        }
    );
    assert_eq!(
        direct.try_acquire(1_000, 0).unwrap(),
        Decision::Allowed {
            remaining: Remaining::new(0)
        }
    );

    let mut t = 0_u64;
    for delta in [13_u64, 999_991, 42_123_457, 1, 88_765_432, 333_333_333] {
        t += delta;
        let _ = stepped.try_acquire(999, t).unwrap();
    }

    assert_eq!(
        stepped.available_remaining(t).unwrap(),
        direct.available_remaining(t).unwrap()
    );
    assert_eq!(
        stepped.available_at(50, t).unwrap(),
        direct.available_at(50, t).unwrap()
    );
}

#[test]
fn enforces_clock_monotonicity_cost_bounds_and_overflow_safety() {
    let cap = Capacity::new(10).unwrap();
    let rate = RatePerSec::new(1).unwrap();
    let mut quota = TokenBucket::new(cap, rate, 5_000);

    assert_eq!(
        quota.try_acquire(11, 5_000),
        Err(QuotaError::CostExceedsCapacity {
            requested: 11,
            capacity: 10
        })
    );
    assert_eq!(quota.available_tokens(5_000).unwrap(), 10);

    assert_eq!(
        quota.try_acquire(1, 4_999),
        Err(QuotaError::ClockRegression {
            last_ns: 5_000,
            now_ns: 4_999
        })
    );

    let near_max = u64::MAX - 100;
    let mut high_clock = TokenBucket::new(cap, rate, near_max);
    assert_eq!(
        high_clock.try_acquire(10, near_max).unwrap(),
        Decision::Allowed {
            remaining: Remaining::new(0)
        }
    );
    assert_eq!(
        high_clock.available_at(1, near_max),
        Err(QuotaError::ArithmeticOverflow)
    );
    assert_eq!(
        high_clock.try_acquire(1, near_max),
        Err(QuotaError::ArithmeticOverflow)
    );
}
