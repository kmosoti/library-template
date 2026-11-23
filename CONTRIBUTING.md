# Contributing to `library-template`

## Core Invariants

1. **Pure-Rust Automation Only:** Do not introduce shell or scripting files anywhere in the repository. Add automation to `xtask/` (`#![forbid(unsafe_code)]`).
2. **Small, Focused Modules:** Keep every `.rs` source file `<= 300` lines (hard gate `<= 350` lines enforced by `cargo xtask check-line-caps`).
3. **`#![no_std]` & Dependency Discipline:** `crates/quota-core` must remain `#![no_std]` with zero normal/build dependencies and compile cleanly for `thumbv7em-none-eabi`. `crates/quota-adapters` may depend only on `quota-core`, `serde`, and `serde_json`.
4. **Strict Two-Commit `Trust-Boundary` Workflow (Zero Mixed Commits):**
   - **Commit 1 (Verifier / Governance):** Update black-box tests (`crates/*/tests/**`), public API surface (`api/quota-core.api`), `xtask/**`, or `.github/**` with bottom Git trailer `Trust-Boundary: verifier`.
   - **Commit 2 (Implementation):** Update `crates/*/src/**` to satisfy the contract without touching verifier paths in the same commit. Every commit carrying `Trust-Boundary: verifier` must touch at least one verifier path.
5. **Promotion, Ancestry & Release Discipline:**
   - Merge feature PRs into `develop-agent` and `release/*` strictly via **`--rebase`** (`allowed_merge_methods: ["rebase"]`, preserving 2-commit `Trust-Boundary` separation and linear history).
   - Promote `develop-agent` -> `main` via `--no-ff` merge commit (`allowed_merge_methods: ["merge"]`, gated by `main-release`), keeping `develop-agent` a linear true ancestor of `main` (`cargo xtask sync-branches --check`).
   - Cut `v*` release tags as a Repository Admin either locally with a signed tag (`git tag -s vX.Y.Z <sha> && git push origin vX.Y.Z`) or via `.github/workflows/cut-release-tag.yml` (`tag-release` Environment). Stable releases require approval in the `stable-publish` Environment.

## Pre-Submission Check

Always run `cargo xtask verify` on a clean working tree before opening a pull request or cutting a release tag:

```bash
cargo xtask verify
```
