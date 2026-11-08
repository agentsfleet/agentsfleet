//! A null override restores the recipe; omitted fields retain current intent.

use crate::authoring::present;
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

/// Instance fields overlaid on a fleet's pinned recipe.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Override {
    /// Replacement five-field expression.
    #[serde(deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    pub cron: Option<String>,
    /// Replacement timezone name.
    #[serde(deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    /// Replacement scheduled message.
    #[serde(deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Replacement firing intent.
    #[serde(deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    pub status: Option<Status>,
}

/// Inherited schedules accept only `override`; independent schedules use fields.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(as = SchedulePatch))]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Patch {
    /// Present null restores inherited setup; an object replaces the overlay.
    #[serde(default, rename = "override", deserialize_with = "present")]
    pub overrides: Option<Option<Override>>,
    /// Independent-schedule expression.
    #[serde(default, deserialize_with = "present")]
    pub cron: Option<String>,
    /// Independent-schedule timezone.
    #[serde(default, deserialize_with = "present")]
    pub timezone: Option<String>,
    /// Independent-schedule message.
    #[serde(default, deserialize_with = "present")]
    pub message: Option<String>,
    /// Independent-schedule firing intent.
    #[serde(default, deserialize_with = "present")]
    pub status: Option<Status>,
}
