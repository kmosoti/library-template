---
id: R-0001
kind: research
status: active
date: 2026-11-23
added_by: agent
human_ratified: false
links:
  - D-0001
  - O-0001
---
# R-0001: Mechanical SemVer Public API Surface Ratchet

## Summary

Downstream SDK and service consumers depend on compile-time and behavioral stability across patch and minor upgrades. Relying solely on human code review to notice subtle signature or trait-derive mutations—such as removing `Copy` or `Ord`, dropping `const` from a constructor, adding an enum variant to an exhaustive error type, or altering a parameter type—leads to silent SemVer regressions.

## Findings

1. **Derive-Aware Surface Manifest (`api/quota-core.api`):** Recording sorted canonical signatures plus `#[derive(...)]` and `#[repr(...)]` attributes across `crates/quota-core/src/**/*.rs`—paired with `#![no_std]` trait-bound assertions in `crates/quota-core/tests/semver_contract.rs`—turns public API review into a deterministic diff checked against `--base` SemVer bumps.
2. **Strict Two-Commit Trust Boundary (`Trust-Boundary: verifier`):** Prohibiting mixed commits touching both `crates/*/src/**` and verifier/governance paths (`crates/*/tests/**`, `api/**`, `xtask/**`, `.github/**`)—and requiring verifier commits carrying trailer `Trust-Boundary: verifier` to strictly precede implementation commits—prevents agents from weakening tests alongside implementation edits.
3. **Channel Graduation & Admin Tagging (`alpha` -> `beta` -> `rc` -> `stable`):** Admin-restricted `v*` pre-release tags exercise consumer-verified packaging and SLSA provenance (`prerelease: true`) before publishing stable SemVer contracts through the human-reviewed `stable-publish` Environment.
