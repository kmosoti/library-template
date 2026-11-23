//! Boundary translation adapters between JSON wire payloads and [`quota_core`].
//!
//! Keeps serialization dependencies (`serde`, `serde_json`) strictly outside the
//! `#![no_std]` zero-dependency [`quota_core`] crate.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod wire;

pub use wire::{
    AcquireRequestWire, AcquireResponseWire, AdapterError, PolicyConfigWire, QuotaJsonAdapter,
};
