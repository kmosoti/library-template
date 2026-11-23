use core::fmt;
use quota_core::{Capacity, Decision, QuotaError, RatePerSec, TokenBucketQuota};
use serde::{Deserialize, Serialize};

/// Errors produced while decoding wire payloads or evaluating [`quota_core`] contracts.
#[derive(Debug)]
pub enum AdapterError {
    /// JSON syntax or schema validation failure at the wire boundary.
    WireJson(serde_json::Error),
    /// Domain invariant violation rejected by [`quota_core`].
    Domain(QuotaError),
}

impl fmt::Display for AdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WireJson(err) => write!(f, "wire json error: {err}"),
            Self::Domain(err) => write!(f, "quota domain error: {err}"),
        }
    }
}

impl std::error::Error for AdapterError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::WireJson(err) => Some(err),
            Self::Domain(err) => Some(err),
        }
    }
}

impl From<serde_json::Error> for AdapterError {
    fn from(value: serde_json::Error) -> Self {
        Self::WireJson(value)
    }
}

impl From<QuotaError> for AdapterError {
    fn from(value: QuotaError) -> Self {
        Self::Domain(value)
    }
}

/// External configuration DTO translated into a validated [`TokenBucketQuota`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyConfigWire {
    /// Maximum burst capacity in whole tokens (`>= 1`).
    pub capacity: u64,
    /// Steady-state refill rate in tokens per second (`>= 1`).
    pub rate_per_sec: u64,
    /// Initial monotonic timestamp in nanoseconds.
    pub initial_ns: u64,
}

impl PolicyConfigWire {
    /// Validates wire primitives into a [`TokenBucketQuota`].
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError::Domain`] if `capacity == 0` or `rate_per_sec == 0`.
    pub fn into_quota(self) -> Result<TokenBucketQuota, AdapterError> {
        let cap = Capacity::new(self.capacity)?;
        let rate = RatePerSec::new(self.rate_per_sec)?;
        Ok(TokenBucketQuota::new(cap, rate, self.initial_ns))
    }
}

/// External token-acquisition request DTO.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcquireRequestWire {
    /// Requested whole tokens.
    pub cost: u64,
    /// Caller monotonic timestamp in nanoseconds.
    pub now_ns: u64,
}

/// External token-acquisition decision DTO.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum AcquireResponseWire {
    /// Request succeeded and tokens were deducted.
    Allowed {
        /// Remaining whole tokens after deduction.
        remaining: u64,
    },
    /// Request was rate-limited without deducting tokens.
    Denied {
        /// Available whole tokens at evaluation time.
        available: u64,
        /// Earliest monotonic timestamp in nanoseconds when `cost` tokens will be ready.
        retry_at_ns: u64,
    },
}

impl From<Decision> for AcquireResponseWire {
    fn from(decision: Decision) -> Self {
        match decision {
            Decision::Allowed { remaining } => Self::Allowed {
                remaining: remaining.get(),
            },
            Decision::Denied {
                available,
                retry_at_ns,
            } => Self::Denied {
                available: available.get(),
                retry_at_ns,
            },
        }
    }
}

/// Stateful JSON boundary adapter wrapping a [`TokenBucketQuota`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotaJsonAdapter {
    quota: TokenBucketQuota,
}

impl QuotaJsonAdapter {
    /// Parses a JSON [`PolicyConfigWire`] document and constructs a [`QuotaJsonAdapter`].
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError`] if JSON decoding fails or domain validation rejects zero values.
    pub fn from_policy_json(raw_json: &str) -> Result<Self, AdapterError> {
        let cfg: PolicyConfigWire = serde_json::from_str(raw_json)?;
        Ok(Self {
            quota: cfg.into_quota()?,
        })
    }

    /// Evaluates a JSON [`AcquireRequestWire`] payload and serializes an [`AcquireResponseWire`].
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError`] if request JSON is malformed or violates [`quota_core`] invariants.
    pub fn handle_acquire_json(&mut self, raw_request: &str) -> Result<String, AdapterError> {
        let req: AcquireRequestWire = serde_json::from_str(raw_request)?;
        let decision = self.quota.try_acquire(req.cost, req.now_ns)?;
        let response = AcquireResponseWire::from(decision);
        Ok(serde_json::to_string(&response)?)
    }

    /// Borrows the underlying [`TokenBucketQuota`] domain state.
    #[must_use]
    pub const fn inner(&self) -> &TokenBucketQuota {
        &self.quota
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_valid_wire_requests_and_decisions() {
        let mut adapter = QuotaJsonAdapter::from_policy_json(
            r#"{"capacity":4,"rate_per_sec":2,"initial_ns":1000}"#,
        )
        .unwrap();

        let ok_json = adapter
            .handle_acquire_json(r#"{"cost":3,"now_ns":1000}"#)
            .unwrap();
        let ok_resp: AcquireResponseWire = serde_json::from_str(&ok_json).unwrap();
        assert_eq!(ok_resp, AcquireResponseWire::Allowed { remaining: 1 });

        let denied_json = adapter
            .handle_acquire_json(r#"{"cost":2,"now_ns":1000}"#)
            .unwrap();
        let denied_resp: AcquireResponseWire = serde_json::from_str(&denied_json).unwrap();
        assert_eq!(
            denied_resp,
            AcquireResponseWire::Denied {
                available: 1,
                retry_at_ns: 500_001_000
            }
        );
    }

    #[test]
    fn rejects_unknown_wire_fields_and_invalid_domain_bounds() {
        let unknown = QuotaJsonAdapter::from_policy_json(
            r#"{"capacity":4,"rate_per_sec":2,"initial_ns":0,"extra":true}"#,
        );
        assert!(matches!(unknown, Err(AdapterError::WireJson(_))));

        let zero_cap =
            QuotaJsonAdapter::from_policy_json(r#"{"capacity":0,"rate_per_sec":2,"initial_ns":0}"#);
        assert!(matches!(
            zero_cap,
            Err(AdapterError::Domain(QuotaError::ZeroCapacity))
        ));
    }
}
