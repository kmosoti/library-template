---
id: O-0001
kind: observation
status: verified
date: 2026-11-23
added_by: agent
human_ratified: false
links:
  - D-0001
  - R-0001
---
# O-0001: Bare-Metal `thumbv7em-none-eabi` Compilation & Division Lowering

## Observation

Compiling `quota-core` with `cargo check --locked -p quota-core --target thumbv7em-none-eabi` under Rust `1.100.0` (`edition = "2024"`) succeeds with zero external crates and confirms that `core::error::Error` and `core::fmt::Display` are available natively in `#![no_std]` without `std` feature flags.

## Details

- Cortex-M4/M7 (`thumbv7em-none-eabi`) lacks native `alloc` and OS syscalls; enforcing this target inside `cargo xtask verify` immediately catches accidental `std::` imports, `Vec`/`String` allocations, or `SystemTime` usage in `quota-core`.
- Checked `u128` division by constant `1_000_000_000` (`NANOS_PER_SEC`) and `rate_u128` lowers cleanly through `core` compiler intrinsics (`compiler_builtins`) without requiring floating-point ABI flags (`-mfloat-abi=hard`).
