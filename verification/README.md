# Verification Architecture (`verification/`)

`library-template` treats its **compile-time & behavioral API contract** as the repository's unit of truth. Four independent mechanical layers verify every change via `cargo xtask verify`:

1. **Black-Box Behavioral & SemVer Consumer Contracts (`crates/quota-core/tests/`):**
   - `contract_suite.rs`: exercises zero-capacity/zero-rate rejection, idle window-edge burst clamping, deterministic PRNG sweeps proving `available_at` is the exact minimal ready nanosecond, path-independent sub-token refill, clock regression rejection, and `u64::MAX` overflow safety.
   - `semver_contract.rs`: `#![no_std]` downstream consumer asserting `Copy + Clone + Eq + PartialEq + Ord + PartialOrd + Hash + Debug` and exact `const fn` / method pointers on `Capacity`, `RatePerSec`, `Remaining`, `Decision`, `QuotaError`, and `TokenBucket`.
2. **Derive-Aware Public API Surface Ratchet (`api/quota-core.api`):**
   `cargo xtask check-api-surface [--base <ref>]` recursively parses `crates/quota-core/src/**/*.rs` (including `#[derive]` and `#[repr]`) and fails closed on unratified surface mutations or missing SemVer version bumps.
3. **Bare-Metal & Dependency Boundary Proof (`thumbv7em-none-eabi` + `check-architecture`):**
   `cargo check --locked -p quota-core --target thumbv7em-none-eabi` proves `#![no_std]` purity, while `cargo metadata --no-deps --locked` proves `quota-core` has zero normal/build dependencies (`lib` target only) and `quota-adapters` depends on no workspace crate other than `quota-core`.
4. **Strict Two-Commit Trust-Boundary & Honest Receipt (`evidence/verification.json`):**
   `cargo xtask check-trust-boundary` rejects any commit mixing `crates/*/src/**` with verifier/governance paths (`crates/*/tests/**`, `api/**`, `xtask/**`, `.github/**`) and requires `Trust-Boundary: verifier` commits to strictly precede implementation commits. Passing all 12 gates writes `evidence/verification.json` with explicit `worktree_dirty` / `tree_hash` honesty.
