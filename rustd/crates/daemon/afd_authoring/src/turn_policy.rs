//! Admission limits are captured with each authoring attempt, never reloaded mid-turn.

use garde::Validate;
use serde::Deserialize;
use std::time::Duration;

/// Private setup policy within the runner protocol's compiled compatibility ceilings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub struct TurnSettings {
    /// Includes every distinct physical model dispatch in one accepted attempt.
    #[garde(range(min = 1, max = af_wire::authoring::MODEL_CALLS_MAX))]
    pub max_model_calls: u32,
    /// Wall time across the attempt, including renewals.
    #[garde(range(min = 1, max = af_wire::authoring::WALL_SECONDS_MAX))]
    pub max_wall_seconds: u32,
    /// Additional proposals after a refused proposal; zero disables correction.
    #[garde(range(max = af_wire::authoring::PROPOSAL_CORRECTIONS_MAX))]
    pub max_proposal_corrections: u32,
    /// One provider request cannot outlive the attempt's accepted wall allowance.
    #[garde(range(min = 1, max = af_wire::authoring::WALL_SECONDS_MAX))]
    pub provider_timeout_seconds: u32,
}

impl TurnSettings {
    const DEFAULT: Self = Self {
        max_model_calls: af_wire::authoring::MODEL_CALLS_MAX,
        max_wall_seconds: af_wire::authoring::WALL_SECONDS_MAX,
        max_proposal_corrections: af_wire::authoring::PROPOSAL_CORRECTIONS_MAX,
        provider_timeout_seconds: af_wire::authoring::WALL_SECONDS_MAX,
    };
}

impl Default for TurnSettings {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Checked admission policy, copied into the attempt's durable snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TurnPolicy {
    settings: TurnSettings,
}

impl Default for TurnPolicy {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl TurnPolicy {
    pub(crate) const DEFAULT: Self = Self {
        settings: TurnSettings::DEFAULT,
    };

    /// Accepts only settings already checked against the protocol ceilings.
    #[must_use]
    pub fn new(settings: &garde::Valid<TurnSettings>) -> Self {
        Self {
            settings: **settings,
        }
    }

    /// Non-secret accepted settings, suitable for effective-configuration diagnostics.
    #[must_use]
    pub const fn settings(self) -> TurnSettings {
        self.settings
    }

    pub(crate) fn calls(self) -> i64 {
        i64::from(self.settings.max_model_calls)
    }

    pub(crate) fn wall(self) -> Duration {
        Duration::from_secs(u64::from(self.settings.max_wall_seconds))
    }

    pub(crate) fn provider_timeout(self) -> Duration {
        Duration::from_secs(u64::from(self.settings.provider_timeout_seconds))
    }

    pub(crate) fn admit_call(self, value: u32) -> crate::Result<()> {
        let _checked = garde::Unvalidated::new(CallIndex {
            value,
            maximum: self.settings.max_model_calls,
        })
        .validate()?;
        Ok(())
    }
}

#[derive(Validate)]
struct CallIndex {
    #[garde(range(min = 1, max = self.maximum))]
    value: u32,
    #[garde(skip)]
    maximum: u32,
}
