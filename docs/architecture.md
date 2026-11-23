# Architecture — `library-template`

## Position in the 4-Template Lifecycle Suite

```text
research-template     -->  library-template         -->  service-template       ==>  platform-template
 (standalone conceptual     (standalone #![no_std]       (standalone quota           (concrete byte/digest
  token_bucket_ewma)         TokenBucket + SemVer API)    daemon + manifests)         deployment handoff)
```

`research-template`, `library-template`, and `service-template` share the quota domain conceptually as standalone repositories, while `service-template` -> `platform-template` forms the concrete byte/digest deployment handoff.

## Workspace Crate Topology

```text
crates/quota-core       (#![no_std], #![forbid(unsafe_code)], 0 dependencies, lib only)
       ^
       |
crates/quota-adapters   (#![forbid(unsafe_code)], depends ONLY on quota-core + serde/serde_json)

xtask                   (pure-Rust workflow, governance, hooks, ancestry & packaging engine)
```

- **`crates/quota-core`:** Implements `Capacity`, `RatePerSec`, `Remaining`, `Decision`, `QuotaError`, and `TokenBucket` (`TokenBucketQuota`) using checked integer nanosecond math with zero heap allocations and zero external crates.
- **`api/quota-core.api` & `semver_contract.rs`:** `check-api-surface [--base <ref>]` captures module-qualified signatures, `#[derive]`/`#[repr]`/`#[non_exhaustive]` attributes, trait items, and associated types/consts, classifying exhaustive enum variant additions and required trait item additions as SemVer-breaking.

## Trust-Boundary, Atomic Receipts & Reproducible Packaging

- **Scoped 2-Commit Separation (`trust_boundary.rs`):** Rejects mixed `crates/*/src/**` + verifier (`crates/*/tests/**`, `api/**`, `xtask/**`, `.github/**`) commits, requires `Trust-Boundary: verifier` trailers to touch verifier paths and precede implementation commits, and scopes default `verify` checks to `HEAD^1..HEAD^2` on merge promotions (plus merge conflict-resolution delta inspection) or `HEAD~1..HEAD` on linear commits.
- **Full-Fingerprint Atomic Receipt (`verify.rs`):** Fingerprints `HEAD`, `HEAD^{tree}`, sorted refs digest, and dirty-file SHA-256s before and after all 12 gates and writes `evidence/verification.json` atomically (`fsync` + `rename`).
- **Consumer-Verified Pure-Rust Archive (`package.rs`):** Builds a deterministic `tar` + `flate2` archive over `git ls-files` and runs offline `cargo metadata`, `cargo test --workspace`, and `cargo check -p quota-core --target thumbv7em-none-eabi` in an unpacked temp directory.

## Five Rulesets & Three Isolated Environments

1. **`library-template-main-protection` (`refs/heads/main`):** `["merge"]`, `["gates", "main-release-gate"]`, `required_deployments: ["main-release"]`.
2. **`library-template-agent-branch-protection` (`refs/heads/develop-agent`):** Linear `["rebase"]` only (never squashing 2-commit PRs), `["gates"]`; ancestry verified read-only via `cargo xtask sync-branches --check` (`ExactMatch`, `PromotedAncestor`, `AgentAheadOfPromotedBase`).
3. **`library-template-maintenance-protection` (`refs/heads/release/*`):** Linear `["rebase"]`, `["gates", "main-release-gate"]`, `required_deployments: ["main-release"]`.
4. **`library-template-tag-immutability` (`refs/tags/v*`):** `["update", "deletion", "non_fast_forward"]` with **`bypass_actors: []`**.
5. **`library-template-tag-creation` (`refs/tags/v*`):** `["creation"]` restricted to Repository Admin (`actor_id: 5`), paired with unprivileged `verify-candidate` + checkout-free API-only `create-release-tag` (`tag-release` Environment) and draft-first immutable `gh release` publishing (`stable-publish` Environment for stable tags).
