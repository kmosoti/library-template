# Claude Code Instructions — `library-template`

Follow all invariants defined in [`AGENTS.md`](AGENTS.md), [`SECURITY.md`](SECURITY.md), and [`docs/architecture.md`](docs/architecture.md).

## Mandatory Commands

- Verify workspace idempotently (reuses `evidence/verification.json` only on a clean worktree matching `HEAD`, `Cargo.lock`, `rust-toolchain.toml`, and `api/quota-core.api`):
  ```bash
  cargo xtask verify
  ```
- Force full 12-gate verification:
  ```bash
  cargo xtask verify --force
  ```
- Check SemVer tag & clean-tree release preflight:
  ```bash
  cargo xtask preflight-tag v0.1.0
  ```

## Safety Rails Enforced by `.claude/settings.json`

`PreToolUse` runs `cargo run --quiet --manifest-path "${CLAUDE_PROJECT_DIR:-.}/Cargo.toml" --package xtask -- hook pre-tool-use --format claude`, emitting `{}` on neutral commands (never bypassing user permission prompts) and denying:
- `gh pr merge` missing `--rebase` or targeting `main` / `release/*`
- Agent approval of `main-release` or `stable-publish` human gate deployments
- Direct `git push` to `main` / `release/*`, unverified `v*` tag pushes, or malformed hook JSON payloads
