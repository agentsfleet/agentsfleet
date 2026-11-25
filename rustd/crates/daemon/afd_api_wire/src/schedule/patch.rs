//! A schedule edit names the schedule's own fields; omitted fields keep their
//! current value.

use crate::present;
use serde::{Deserialize, Serialize};

/// Human-editable schedule intent; deletion keeps its separate resource verb.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Schedule should fire on its expression.
    Active,
    /// Schedule should remain stored without firing.
    Paused,
}

impl Status {
    /// Stored scheduler intent spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Paused => "paused",
        }
    }
}

/// The fields a schedule edit changes. An unknown field is refused.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(as = SchedulePatch))]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Patch {
    /// Replacement five-field expression.
    #[serde(default, deserialize_with = "present")]
    pub cron: Option<String>,
    /// Replacement timezone name.
    #[serde(default, deserialize_with = "present")]
    pub timezone: Option<String>,
    /// Replacement scheduled message.
    #[serde(default, deserialize_with = "present")]
    pub message: Option<String>,
    /// Replacement firing intent.
    #[serde(default, deserialize_with = "present")]
    pub status: Option<Status>,
}
