# GitHub Copilot Instructions — `library-template`

Obey [`AGENTS.md`](../AGENTS.md) at all times:

- Keep `crates/quota-core` `#![no_std]`, `#![forbid(unsafe_code)]`, zero-dependency, and compatible with `thumbv7em-none-eabi`.
- Keep every `.rs` file `<= 300` lines (hard gate `<= 350` lines).
- Preserve `api/quota-core.api` exact public signature parity; run `cargo xtask check-api-surface` after touching `crates/quota-core/src/`.
- Separate `crates/*/tests/` and `api/*.api` commits from `crates/*/src/` commits (`cargo xtask check-trust-boundary`).
- Run `cargo xtask verify` before proposing commits, pull requests, or release tags.
