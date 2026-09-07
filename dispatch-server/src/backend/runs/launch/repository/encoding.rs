use crate::backend::runs::launch::{
    model::*,
    policy::{constant_time_eq, domain_sha256, validate_sha256},
};
use rootcause::{Result, prelude::*};
use serde::{Serialize, de::DeserializeOwned};
const MAX_CANONICAL_BYTES: usize = 16384;
impl AgentLaunchTargetV1 {
    pub(crate) fn encode(&self) -> Result<CanonicalRecord> {
        self.validate()?;
        canonical_record("dispatch.agent-launch-target.v1", self)
    }
}
impl AgentLaunchTargetV1 {
    pub(crate) fn decode(bytes: &str, digest: &str) -> Result<Self> {
        decode_canonical(
            "dispatch.agent-launch-target.v1",
            bytes,
            digest,
            Self::validate,
        )
    }
}
impl AgentLaunchResolutionV1 {
    pub(crate) fn encode(&self) -> Result<CanonicalRecord> {
        self.validate()?;
        canonical_record("dispatch.agent-launch-resolution.v1", self)
    }
}
impl AgentLaunchResolutionV1 {
    pub(crate) fn decode(bytes: &str, digest: &str) -> Result<Self> {
        decode_canonical(
            "dispatch.agent-launch-resolution.v1",
            bytes,
            digest,
            Self::validate,
        )
    }
}
impl AgentCapabilitySetV1 {
    pub(crate) fn encode(&self) -> Result<CanonicalRecord> {
        self.validate()?;
        canonical_record("dispatch.agent-capability-set.v1", self)
    }
}
impl AgentCapabilitySetV1 {
    pub(crate) fn decode(bytes: &str, digest: &str) -> Result<Self> {
        decode_canonical(
            "dispatch.agent-capability-set.v1",
            bytes,
            digest,
            Self::validate,
        )
    }
}
#[derive(Clone, Debug)]
pub(crate) struct CanonicalRecord {
    pub(crate) json: String,
    pub(crate) sha256: String,
}

fn canonical_record<T: Serialize>(domain: &str, value: &T) -> Result<CanonicalRecord> {
    let json =
        serde_json::to_string(value).context("failed to encode canonical agent run record")?;
    if json.len() > MAX_CANONICAL_BYTES {
        bail!("canonical agent run record is too large");
    }
    Ok(CanonicalRecord {
        sha256: domain_sha256(domain, json.as_bytes()),
        json,
    })
}

fn decode_canonical<T: DeserializeOwned + Serialize>(
    domain: &str,
    bytes: &str,
    expected_sha256: &str,
    validate: impl FnOnce(&T) -> Result<()>,
) -> Result<T> {
    if bytes.len() > MAX_CANONICAL_BYTES {
        bail!("canonical agent run record is too large");
    }
    validate_sha256("canonical record digest", expected_sha256)?;
    let actual = domain_sha256(domain, bytes.as_bytes());
    if !constant_time_eq(actual.as_bytes(), expected_sha256.as_bytes()) {
        bail!("canonical agent run record digest mismatch");
    }
    let value: T = serde_json::from_str(bytes).context("invalid canonical agent run record")?;
    validate(&value)?;
    let encoded = serde_json::to_string(&value)?;
    if encoded.as_bytes() != bytes.as_bytes() {
        bail!("agent run record is not canonical JSON");
    }
    Ok(value)
}
