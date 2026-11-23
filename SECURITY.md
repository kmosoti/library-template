# Security Policy & Governance Trust Boundaries

## Client-Side Agent Hooks vs. Server-Side GitHub Rulesets

`library-template` deploys defense-in-depth across two distinct layers:

1. **Client-Side `PreToolUse` Hooks (Advisory UX Guardrail):**
   - Configured across Antigravity (`.agents/hooks.json`), Claude Code (`.claude/settings.json`), OpenAI Codex (`.codex/hooks.json`), and GitHub Copilot (`.github/hooks/forge-guard.json`).
   - Invoke `cargo xtask hook pre-tool-use --format <host>` to block accidental direct pushes to `main`/`release/*`, unverified `v*` tag pushes, non-rebase PR merges, or attempts to approve `main-release` / `stable-publish` / `tag-release` deployments, failing closed with structured JSON deny responses on malformed inputs.
   - Client hooks execute inside the caller's local environment and are **never** relied upon as the sole security boundary.

2. **Server-Side GitHub Rulesets & Isolated Environments (Authoritative Enforcement):**
   - Configured idempotently with policy pruning and typed readback verification via `cargo xtask github-setup <OWNER/REPO>`:
     - **`library-template-main-protection` (`refs/heads/main`):** Blocks deletion and non-fast-forward pushes (`bypass_actors: []`), requires PR review, status checks `["gates", "main-release-gate"]`, `allowed_merge_methods: ["merge"]`, and deployment to **`main-release`**.
     - **`library-template-agent-branch-protection` (`refs/heads/develop-agent`):** Enforces linear history (`allowed_merge_methods: ["rebase"]` — squash disabled so 2-commit `Trust-Boundary` PRs cannot be collapsed into mixed commits) and `["gates"]`.
     - **`library-template-maintenance-protection` (`refs/heads/release/*`):** Enforces linear history (`["rebase"]`), status checks `["gates", "main-release-gate"]`, and deployment to **`main-release`**.
     - **`library-template-tag-immutability` (`refs/tags/v*`):** Blocks `update`, `deletion`, and `non_fast_forward` with **`bypass_actors: []`** so published `v*` tags are permanently immutable even to admins.
     - **`library-template-tag-creation` (`refs/tags/v*`):** Restricts tag `creation` strictly to Repository Admins (`actor_id: 5`).
     - **Privilege-Separated Tag & Release Environments:**
       - `cut-release-tag.yml` verifies untrusted candidate code in an unprivileged `verify-candidate` job (`contents: read`, `persist-credentials: false`, zero secrets) and creates the Git ref in a checkout-free API-only `create-release-tag` job gated by the `main`-only **`tag-release`** Environment (`TAG_CREATOR_TOKEN`). Admins may alternatively push signed tags locally (`git tag -s vX.Y.Z <sha> && git push origin vX.Y.Z`) with zero stored CI secrets.
       - `release.yml` publishes pre-releases and **`stable-publish`**-gated stable releases via a draft-first (`gh release create --draft` -> verify 3 assets -> `gh release edit --draft=false`) flow compatible with GitHub Immutable Releases and idempotent digest retries.

## Code & Supply-Chain Posture

- `#![forbid(unsafe_code)]` across the entire workspace (`quota-core`, `quota-adapters`, `xtask`).
- `crates/quota-core` is `#![no_std]` with **zero** normal or build dependencies.
- All GitHub Actions are pinned to exact 40-character lowercase commit SHAs, and release bundles are attested via SLSA build provenance (`actions/attest-build-provenance`).

## Reporting a Vulnerability

Please report security issues privately through GitHub Security Advisories on your repository rather than opening a public issue.
