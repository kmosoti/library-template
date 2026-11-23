//! Deterministic `#![no_std]` token-bucket quota primitives.
//!
//! Graduated from `research-template` into a zero-dependency, SemVer-ratcheted
//! library contract verified on both host targets and bare-metal `thumbv7em-none-eabi`.

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod bucket;
mod error;
mod types;

pub use bucket::{TokenBucket, TokenBucketQuota};
pub use error::QuotaError;
pub use types::{Capacity, Decision, RatePerSec, Remaining};
