# Epistemic Knowledge Graph (`knowledge/`)

This directory records linked engineering decisions, upstream research graduations, and empirical target observations governing `library-template`.

Every node carries machine-validated YAML frontmatter (`id`, `kind`, `status`, `added_by`, `human_ratified`, `links`) checked by `cargo xtask check-knowledge`. Nodes authored by autonomous agents start with `added_by: agent` and `human_ratified: false` until reviewed during `main-release` ratification.

## Active Nodes

| ID | Kind | Title | Ratified | Links |
| --- | --- | --- | --- | --- |
| [`D-0001`](decisions/D-0001-no-std-integer-arithmetic.md) | `decision` | `#![no_std]` Checked Integer Nanosecond Arithmetic | `false` | `R-0001`, `O-0001` |
| [`R-0001`](research/R-0001-semver-surface-ratchet.md) | `research` | Mechanical SemVer Public API Surface Ratchet | `false` | `D-0001`, `O-0001` |
| [`O-0001`](observations/O-0001-bare-metal-thumbv7em-target.md) | `observation` | Bare-Metal `thumbv7em-none-eabi` Compilation & Division Lowering | `false` | `D-0001`, `R-0001` |
