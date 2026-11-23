---
id: D-0001
kind: decision
status: active
date: 2026-11-23
added_by: agent
human_ratified: false
links:
  - R-0001
  - O-0001
---
# D-0001: `#![no_std]` Checked Integer Nanosecond Arithmetic

## Context

`quota-core` graduates the token-bucket rate-limiting model from `research-template` into a reusable library consumed by both embedded/bare-metal runtimes and standard host services (`service-template`). Floating-point refill rates introduce IEEE-754 rounding drift across frequent polling intervals, non-deterministic edge behavior near burst boundaries, and soft-float overhead on microcontroller targets without hardware FPUs.

## Decision

1. Keep `crates/quota-core` strictly `#![no_std]`, `#![forbid(unsafe_code)]`, and zero-dependency (`lib` target only).
2. Represent capacity (`Capacity`) and steady-state refill rate (`RatePerSec`) as validated non-zero `u64` domain types.
3. Track sub-token refill progress in exact integer nano-tokens (`sub_token_nanos < 1_000_000_000`) using checked `u128` intermediate products (`elapsed_ns * rate_per_sec + sub_token_nanos`) and clamp `sub_token_nanos = 0` whenever the bucket reaches `Capacity`.
4. Isolate all wire serialization (`serde`, `serde_json`) inside `crates/quota-adapters`.

## Consequences

- Refill state is path-independent across arbitrary sub-nanosecond polling splits whenever `tokens < capacity`.
- `available_at(cost, now_ns)` computes the exact minimal nanosecond at which `try_acquire(cost, ready_ns)` transitions from `Denied` to `Allowed`.
- Zero ambient clock or heap dependencies enter `quota-core`; callers inject `now_ns: u64` explicitly.
