---
name: idempotent-workflow
description: Use receipt-cached verification (`evidence/verification.json`) and deterministic packaging without mutating tracked git state.
---
# Idempotent Workflow (`library-template`)

1. **Receipt-Backed Verification (`cargo xtask verify`):**
   - `cargo xtask verify` computes `(HEAD^{tree}, sha256(Cargo.lock), rustc --version)` and short-circuits with `[idempotent cache hit]` when `evidence/verification.json` already proves a clean passing tree.
   - Pass `--force` (`cargo xtask verify --force`) when validating toolchain upgrades or auditing every gate from scratch.
2. **Isolated Build Targets:**
   - Keep Cargo build outputs isolated via `CARGO_TARGET_DIR` when running multi-workspace experiments, and never commit `/target`, `/evidence`, or `/dist`.
3. **Deterministic Packaging & Tag Preflight:**
   - Run `cargo xtask preflight-tag <tag>` and `cargo xtask package --out dist` only after `evidence/verification.json` matches `HEAD^{tree}`.
