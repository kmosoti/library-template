# `library-template`

Archetype **2 of 4** in the 4-Template Lifecycle Suite (`research-template` -> **`library-template`** -> `service-template` -> `platform-template`).

`library-template` is a standalone Rust workspace template for **reusable libraries and SDKs** whose unit of truth is a **compile-time and behavioral API contract**: `#![no_std]` purity, pinned MSRV/toolchain discipline (`1.100.0`, `edition = "2024"`), mechanical module-qualified public API, derive, and impl-member ratcheting (`api/quota-core.api`), strict 2-commit `Trust-Boundary` separation, and `alpha` -> `beta` -> `rc` -> `stable` + `release/X.Y` maintenance releases with SLSA build provenance.

## Lifecycle & Domain Thread

`research-template`, `library-template`, and `service-template` share the quota rate-limiting domain **conceptually as standalone independent repositories** (graduating mathematical models and contracts rather than coupling via monorepo crates), whereas **`service-template` -> `platform-template`** exercises a **concrete byte/digest deployment contract handoff**:

| Archetype | Standalone Repo Role | Domain Artifact & Contract Boundary |
| --- | --- | --- |
| 1. `research-template` | Conceptual upstream | `token_bucket_ewma` reproducible simulation & statistical burst evaluation |
| 2. **`library-template`** | Standalone SDK contract | `#![no_std]` checked-integer `TokenBucket` (`quota-core`) & SemVer API surface (`api/quota-core.api`) |
| 3. `service-template` | Standalone service + artifact producer | `admission-engine` stateful windowed quota daemon emitting digest-pinned release manifests |
| 4. `platform-template` | Concrete handoff consumer | Consumes `service-template` byte/digest artifacts for multi-environment promotion & policy governance |

## Workspace Layout

| Path | Role |
| --- | --- |
| `crates/quota-core` | `#![no_std]`, `#![forbid(unsafe_code)]`, **zero-dependency** token-bucket quota core (`Capacity`, `RatePerSec`, `Remaining`, `Decision`, `TokenBucket`, `QuotaError`) verified on `thumbv7em-none-eabi` |
| `crates/quota-adapters` | Thin `serde` / `serde_json` wire boundary translating external JSON configs and requests into `quota-core` domain calls |
| `api/quota-core.api` | Module-qualified public signature, `#[derive]`, `#[repr]`, `#[non_exhaustive]`, and impl-member manifest enforced by `cargo xtask check-api-surface [--base <ref>]` |
| `xtask/` | Pure-Rust verification, SemVer ratchet, strict 2-commit trust-boundary, ancestry check, deterministic `tar`+`flate2` packaging + unpacked consumer gate, 4-host agent hooks, and idempotent GitHub governance CLI |
| `knowledge/` | Linked epistemic graph (`decisions/`, `research/`, `observations/`) tracking `added_by` and `human_ratified` |
| `docs/` & `SECURITY.md` | Architecture reference, release notes (`docs/releases/v0.1.0.md`), and client-hook vs server-ruleset security model |

## Strict 2-Commit `Trust-Boundary` & Isolated Admin Tag Creation

- **Zero Mixed Commits (`rebase`-only on `develop-agent`):** Non-root commits must **never** modify implementation paths (`crates/*/src/**`) and verifier/governance paths (`crates/*/tests/**`, `api/**`, `xtask/**`, `.github/**`) in the same commit. Commit verifier/API changes **first** with bottom Git trailer `Trust-Boundary: verifier`, then commit implementation changes (`develop-agent` enforces `allowed_merge_methods: ["rebase"]` so 2-commit PRs are never squashed into 1 mixed commit).
- **Split Tag Immutability & Isolated Tag Creation:**
  - `library-template-tag-immutability` blocks tag `update`, `deletion`, and `non_fast_forward` with **`bypass_actors: []`** (even admins cannot rewrite published `v*` tags).
  - `library-template-tag-creation` restricts `refs/tags/v*` creation strictly to Repository Admins (`actor_id: 5`).
  - **Zero-Token Local Admin Tagging (Recommended):** After `cargo xtask verify` and `cargo xtask preflight-tag v0.1.0`, a Repository Admin can push a signed tag directly (`git tag -s v0.1.0 <sha> && git push origin v0.1.0`) with zero stored CI tokens.
  - **Workflow Dispatch Tagging (`cut-release-tag.yml`):** Runs an unprivileged `verify-candidate` job (`contents: read`, zero secrets) followed by a checkout-free API-only `create-release-tag` job gated by the `main`-only `tag-release` Environment (`TAG_CREATOR_TOKEN`).

## Quickstart

```bash
# Run all 12 verification gates and atomically write evidence/verification.json
cargo xtask verify --force

# Re-run idempotently (instant cache hit when clean HEAD, Cargo.lock, toolchain, and API match)
cargo xtask verify

# Validate release tag parity, channel classification, and clean-tree receipt
cargo xtask preflight-tag v0.1.0

# Build pure-Rust deterministic archive and run offline unpacked consumer build/test/no_std gates
cargo xtask package --out dist

# Preview 3 GitHub environments (main-release, stable-publish, tag-release) and 5 rulesets
cargo xtask github-setup example-org/library-template --dry-run
```

## License

Dual-licensed under either of:

- Apache License, Version 2.0 ([`LICENSE-APACHE`](LICENSE-APACHE))
- MIT License ([`LICENSE-MIT`](LICENSE-MIT))

at your option. Copyright (c) 2026 Kennedy Mosoti.
