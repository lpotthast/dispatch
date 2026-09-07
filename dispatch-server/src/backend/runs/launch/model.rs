use super::policy::{canonical_value_sha256, constant_time_eq, validate_sha256, validate_text};
use crudkit_core::condition::Condition;
use dispatch_types::{AgentRunLaunchResolutionView, AgentRunLaunchTargetView, AgentRunPurposeV1};
use rootcause::{Result, prelude::*};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum AgentLaunchTargetV1 {
    None {
        schema_version: u32,
    },
    NextOpen {
        schema_version: u32,
        state: String,
    },
    Selector {
        schema_version: u32,
        selector_sha256: String,
    },
    Specific {
        schema_version: u32,
        work_item_id: i64,
        expected_version: i64,
    },
}

impl AgentLaunchTargetV1 {
    pub(crate) const fn none() -> Self {
        Self::None { schema_version: 1 }
    }

    #[cfg(test)]
    pub(crate) fn next_open(state: impl Into<String>) -> Result<Self> {
        let value = Self::NextOpen {
            schema_version: 1,
            state: state.into(),
        };
        value.validate()?;
        Ok(value)
    }

    pub(crate) fn selector(condition: &Condition) -> Result<Self> {
        Ok(Self::Selector {
            schema_version: 1,
            selector_sha256: canonical_value_sha256(
                "dispatch.agent-launch-selector.v1",
                condition,
            )?,
        })
    }

    pub(crate) fn specific(work_item_id: i64, expected_version: i64) -> Result<Self> {
        let value = Self::Specific {
            schema_version: 1,
            work_item_id,
            expected_version,
        };
        value.validate()?;
        Ok(value)
    }

    pub(crate) fn validate_selector(&self, condition: &Condition) -> Result<()> {
        let Self::Selector {
            selector_sha256, ..
        } = self
        else {
            bail!("launch target is not a selector");
        };
        let actual = canonical_value_sha256("dispatch.agent-launch-selector.v1", condition)?;
        if !constant_time_eq(selector_sha256.as_bytes(), actual.as_bytes()) {
            bail!("automation selector does not match the persisted launch target");
        }
        Ok(())
    }

    pub(crate) fn validate(&self) -> Result<()> {
        let schema_version = match self {
            Self::None { schema_version }
            | Self::NextOpen { schema_version, .. }
            | Self::Selector { schema_version, .. }
            | Self::Specific { schema_version, .. } => *schema_version,
        };
        if schema_version != 1 {
            bail!("agent launch target schema_version must be 1");
        }
        match self {
            Self::None { .. } => {}
            Self::NextOpen { state, .. } => validate_text("state", state)?,
            Self::Selector {
                selector_sha256, ..
            } => validate_sha256("selector_sha256", selector_sha256)?,
            Self::Specific {
                work_item_id,
                expected_version,
                ..
            } => {
                if *work_item_id <= 0 || *expected_version <= 0 {
                    bail!("specific launch target item id and expected version must be positive");
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum AgentLaunchResolutionV1 {
    Pending {
        schema_version: u32,
    },
    None {
        schema_version: u32,
    },
    Claimed {
        schema_version: u32,
        work_item_id: i64,
        claimed_version: i64,
    },
    Unavailable {
        schema_version: u32,
        reason: String,
    },
}

impl AgentLaunchResolutionV1 {
    pub(crate) const fn pending() -> Self {
        Self::Pending { schema_version: 1 }
    }

    pub(crate) const fn none() -> Self {
        Self::None { schema_version: 1 }
    }

    pub(crate) fn claimed(work_item_id: i64, claimed_version: i64) -> Self {
        Self::Claimed {
            schema_version: 1,
            work_item_id,
            claimed_version,
        }
    }

    pub(crate) fn unavailable(reason: impl Into<String>) -> Self {
        Self::Unavailable {
            schema_version: 1,
            reason: reason.into(),
        }
    }

    pub(crate) fn validate(&self) -> Result<()> {
        let schema_version = match self {
            Self::Pending { schema_version }
            | Self::None { schema_version }
            | Self::Claimed { schema_version, .. }
            | Self::Unavailable { schema_version, .. } => *schema_version,
        };
        if schema_version != 1 {
            bail!("agent launch resolution schema_version must be 1");
        }
        match self {
            Self::Claimed {
                work_item_id,
                claimed_version,
                ..
            } if *work_item_id <= 0 || *claimed_version <= 0 => {
                bail!("claimed launch resolution values must be positive")
            }
            Self::Unavailable { reason, .. } => validate_text("reason", reason)?,
            _ => {}
        }
        Ok(())
    }
}

impl AgentLaunchTargetV1 {
    pub(crate) fn view(&self) -> AgentRunLaunchTargetView {
        match self {
            Self::None { schema_version } => AgentRunLaunchTargetView::None {
                schema_version: *schema_version,
            },
            Self::NextOpen {
                schema_version,
                state,
            } => AgentRunLaunchTargetView::NextOpen {
                schema_version: *schema_version,
                state: state.clone(),
            },
            Self::Selector {
                schema_version,
                selector_sha256,
            } => AgentRunLaunchTargetView::Selector {
                schema_version: *schema_version,
                selector_sha256: selector_sha256.clone(),
            },
            Self::Specific {
                schema_version,
                work_item_id,
                expected_version,
            } => AgentRunLaunchTargetView::Specific {
                schema_version: *schema_version,
                work_item_id: *work_item_id,
                expected_version: *expected_version,
            },
        }
    }
}

impl AgentLaunchResolutionV1 {
    pub(crate) fn view(&self) -> AgentRunLaunchResolutionView {
        match self {
            Self::Pending { schema_version } => AgentRunLaunchResolutionView::Pending {
                schema_version: *schema_version,
            },
            Self::None { schema_version } => AgentRunLaunchResolutionView::None {
                schema_version: *schema_version,
            },
            Self::Claimed {
                schema_version,
                work_item_id,
                claimed_version,
            } => AgentRunLaunchResolutionView::Claimed {
                schema_version: *schema_version,
                work_item_id: *work_item_id,
                claimed_version: *claimed_version,
            },
            Self::Unavailable {
                schema_version,
                reason,
            } => AgentRunLaunchResolutionView::Unavailable {
                schema_version: *schema_version,
                reason: reason.clone(),
            },
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AgentCapabilitySetV1 {
    pub schema_version: u32,
    pub capabilities: Vec<String>,
}

impl AgentCapabilitySetV1 {
    pub(crate) fn ordinary() -> Self {
        Self {
            schema_version: 1,
            capabilities: Vec::new(),
        }
    }

    pub(crate) fn validate(&self) -> Result<()> {
        if self.schema_version != 1 {
            bail!("agent capability set schema_version must be 1");
        }
        if self.capabilities.windows(2).any(|pair| pair[0] >= pair[1]) {
            bail!("agent capabilities must be strictly ordered and unique");
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AgentRunLaunchState {
    Allocated,
    Prepared,
    TargetResolved,
    Spawned,
    Terminal,
}

impl AgentRunLaunchState {
    pub(crate) const fn as_storage(self) -> &'static str {
        match self {
            Self::Allocated => "allocated",
            Self::Prepared => "prepared",
            Self::TargetResolved => "target_resolved",
            Self::Spawned => "spawned",
            Self::Terminal => "terminal",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PersistedLaunchContract {
    pub project_id: i64,
    pub run_id: i64,
    pub purpose: AgentRunPurposeV1,
    pub state: AgentRunLaunchState,
    pub target: AgentLaunchTargetV1,
    pub resolution: AgentLaunchResolutionV1,
    pub capabilities: AgentCapabilitySetV1,
}
