# Agent Operating Contract — `library-template`

This repository is Archetype 2 (`library-template`) of the 4-Template Lifecycle Suite (`research-template` -> `library-template` -> `service-template` -> `platform-template`). Its unit of truth is a **compile-time and behavioral API contract**.

## Non-Negotiable Invariants

1. **Zero Scripting Files:** Never add shell or scripting files anywhere in the repository. All verification, API checks, hooks, branch ancestry checks, packaging, and GitHub setup live in pure Rust inside `xtask/` (`#![forbid(unsafe_code)]`).
2. **Core Purity (`crates/quota-core`):**
   - Must stay `#![no_std]`, `#![forbid(unsafe_code)]`, `#![deny(missing_docs)]`, with **zero** normal/build dependencies and a single `lib` target.
   - Must compile cleanly under `cargo check --locked -p quota-core --target thumbv7em-none-eabi`.
   - Keep external wire serialization (`serde`, `serde_json`) strictly inside `crates/quota-adapters`.
3. **SemVer API, Derive & Impl-Member Ratchet (`api/quota-core.api`):**
   - `cargo xtask check-api-surface [--base <ref>]` recursively inspects `crates/quota-core/src/**/*.rs` (qualifying modules and capturing `#[derive]`, `#[repr]`, `#[non_exhaustive]`, public fields, variants, trait items, and associated types/consts). Exhaustive enum variant additions, required trait item additions, and associated type changes are treated as **breaking** SemVer changes (`0.Y.0 -> 0.(Y+1).0` pre-1.0, major bump `>= 1.0.0`).
4. **Strict Two-Commit `Trust-Boundary` Separation (Zero Mixed Commits):**
   - **Verifier/governance paths:** `crates/*/tests/**`, `api/**`, `xtask/**`, `.github/**`.
   - **Implementation paths:** `crates/*/src/**`.
   - **Never** modify implementation paths and verifier/governance paths in the same commit. Every commit carrying bottom Git trailer `Trust-Boundary: verifier` must touch at least one verifier path and strictly precede its dependent implementation commit (`--rebase` merge only on `develop-agent`).
5. **Epistemic Knowledge Graph (`knowledge/`):**
   - Record decisions (`decisions/D-NNNN-*.md`), research graduations (`research/R-NNNN-*.md`), and empirical target observations (`observations/O-NNNN-*.md`) with `added_by: agent` and `human_ratified: false`. Never set `human_ratified: true` as an agent.
6. **Agent Config Scope & Server Governance (`SECURITY.md`):**
   - **`.agents/` vs. `.agents/plugins/library-forge/`:** `.agents/` is the active workspace-scoped Antigravity configuration (owning `.agents/hooks.json`, `.agents/rules/`, and `.agents/skills/`), whereas `.agents/plugins/library-forge/` is the standalone distributable Antigravity plugin package (intentionally omitting duplicate `hooks.json` while keeping synchronized `rules/`, `skills/`, and `agents/`).
   - `.claude/settings.json`, `.codex/hooks.json` (workspace-discoverable from any subdirectory such as `docs/`), and `.github/hooks/forge-guard.json` (`bash` + `powershell`) provide advisory local `PreToolUse` guardrails; GitHub Rulesets and `main-release` / `stable-publish` / `tag-release` Environments enforce authoritative server-side policy.
