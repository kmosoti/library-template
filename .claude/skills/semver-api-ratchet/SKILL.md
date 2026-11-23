---
name: semver-api-ratchet
description: Guard `api/quota-core.api`, classify SemVer impact, and manage alpha/beta/rc pre-releases vs stable and `release/X.Y` lineages.
---
# SemVer API Ratchet (`library-template`)

Use this skill whenever touching exported items in `crates/quota-core/src/` or preparing a release tag:

1. **Check Surface Drift Immediately:**
   - Run `cargo xtask check-api-surface` (and `cargo xtask check-api-surface --base origin/main` on pull requests).
   - Additive changes (new `pub fn`, new `pub struct`) require an explicit SemVer bump, release notes in `docs/releases/<tag>.md`, and a preceding `Trust-Boundary: verifier` commit updating `api/quota-core.api`.
   - Signature removals, parameter/return type changes, removing `const` or `#[derive(...)]` traits, or adding variants to exhaustive `pub enum` types are **breaking** changes requiring a SemVer-major (or `0.(Y+1).0`) bump and release notes. Never mix `api/*.api` and `crates/*/src/` edits in the same commit.
2. **Channel Progression:**
   - Preview breaking or additive surface changes on `vX.Y.Z-alpha.N` -> `vX.Y.Z-beta.N` -> `vX.Y.Z-rc.N` (`prerelease: true`) before Repository Admins cut `vX.Y.Z` (`stable`, gated by `stable-publish`).
   - Backport non-breaking fixes for older minor lines onto `release/X.Y` maintenance branches.
