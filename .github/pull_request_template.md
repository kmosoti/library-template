## Summary

- **Scope:**
- **Epistemic Nodes (`knowledge/`):**

## Library Contract Checklist

- [ ] `cargo xtask verify --force` passes locally (including `--target thumbv7em-none-eabi`)
- [ ] `crates/quota-core` remains `#![no_std]`, `#![forbid(unsafe_code)]`, and zero-dependency
- [ ] Public API & derive changes (if any) are reflected in `api/quota-core.api` (`check-api-surface --base`) with appropriate SemVer bump
- [ ] Strict 2-commit `Trust-Boundary` separation obeyed: zero mixed `src/` + verifier commits, and any verifier commit (`crates/*/tests/`, `api/`, `xtask/`, `.github/`) carries trailer `Trust-Boundary: verifier` strictly preceding `crates/*/src/` commits
- [ ] New `knowledge/` nodes created by agents leave `human_ratified: false` until human review
