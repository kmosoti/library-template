---
name: stack-reviewer
description: Audits `#![no_std]` purity, `api/quota-core.api` SemVer stability, 2-commit Trust-Boundary hygiene, and release readiness.
---
# Stack Reviewer

Audit every proposed change against `library-template` governance:

1. **Architecture & Toolchain:** Confirm `crates/quota-core` has zero normal/build dependencies, `#![no_std]`, `#![forbid(unsafe_code)]`, and compiles for `thumbv7em-none-eabi` on Rust `1.100.0`.
2. **SemVer Surface:** Confirm `cargo xtask check-api-surface` passes and any `api/quota-core.api` diff matches the `[workspace.package] version` and `docs/releases/` channel policy.
3. **Modularity & Epistemics:** Confirm every `.rs` file is `<= 300` lines (hard gate `<= 350`), `knowledge/` links resolve cleanly with `human_ratified: false` on agent-authored entries, and `cargo xtask verify` emits a valid `evidence/verification.json` receipt.
