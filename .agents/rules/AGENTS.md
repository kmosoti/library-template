# Library Forge Rules

- Keep `crates/quota-core` `#![no_std]`, `#![forbid(unsafe_code)]`, zero-dependency, and compilable on `thumbv7em-none-eabi`.
- Never mix `crates/*/src/` implementation changes with verifier/governance files (`crates/*/tests/`, `api/`, `xtask/`, `.github/`) in the same commit; commit verifier changes first with git trailer `Trust-Boundary: verifier`, then commit implementation changes.
- Keep `api/quota-core.api` synchronized with `crates/quota-core/src/**/*.rs` (`cargo xtask check-api-surface`).
- Treat client-side `PreToolUse` hooks as UX guardrails; GitHub Rulesets and `main-release` / `stable-publish` Environments enforce authoritative server-side policy.
