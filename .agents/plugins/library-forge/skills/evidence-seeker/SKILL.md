---
name: evidence-seeker
description: Ground library claims in bare-metal target compilation, black-box contract sweeps, and linked epistemic knowledge nodes.
---
# Evidence Seeker (`library-template`)

Before claiming a `quota-core` or `quota-adapters` behavior is sound:

1. **Prove `#![no_std]` & Architecture Boundaries:**
   - Run `cargo check --locked -p quota-core --target thumbv7em-none-eabi`.
   - Run `cargo xtask check-architecture` to confirm zero normal/build dependencies on `quota-core` (`lib` target only).
2. **Write Discriminating Black-Box Contract Tests (`crates/quota-core/tests/contract_suite.rs`):**
   - Test boundary timestamps (`0`, `u64::MAX - 100`, clock regressions `now_ns < last_ns`), zero-capacity/zero-rate inputs, and prime refill rates with non-terminating nanosecond remainders.
   - Commit verifier tests in a dedicated commit separate from `crates/*/src/` changes (`cargo xtask check-trust-boundary`).
3. **Record Epistemic Provenance (`knowledge/`):**
   - Add or update linked nodes in `knowledge/{decisions,research,observations}/` with `added_by: agent` and `human_ratified: false`, and index them in `knowledge/README.md`.
