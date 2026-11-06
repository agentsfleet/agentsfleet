//! Schedule display facts copied before subsequent changes can replace them.

use std::borrow::Cow;

use serde::Serialize;

/// What changed in the stored schedule's intent.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleChange {
    /// A new schedule was stored.
    Created,
    /// Fields changed or firing resumed.
    Changed,
    /// Firing was paused; registration may still need removal.
    Paused,
    /// Removal was requested; upstream confirmation is still pending.
    RemovalRequested,
}

/// One bounded stored schedule's display state at a confirmed history mutation.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Serialize)]
pub struct ScheduleSnapshot<'a> {
    /// Stable native schedule identity, retained after deletion.
    pub schedule_id: Cow<'a, str>,
    /// Human description including its timezone, from the native cron parser.
    pub description: Cow<'a, str>,
    /// The saved customer message within its existing fleet boundary.
    pub message: Cow<'a, str>,
    /// Stored intent, or the confirmed terminal fact `removed` / `expired`.
    pub status: Cow<'a, str>,
    /// Registration state at commit; terminal removal is `synced`.
    pub sync: Cow<'a, str>,
}

/// Display provenance resolved for one accepted fire, independent of its run state.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Serialize)]
pub struct ScheduleFireDescription<'a> {
    /// The native schedule identity selected by the existing ingress.
    pub schedule_id: Cow<'a, str>,
    /// The native human description at target resolution.
    pub description: Cow<'a, str>,
}

/// The existing runnable cron event, retaining its message at the usual field.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Serialize)]
pub struct ScheduleFire<'a> {
    /// The saved customer request, read by the normal event message parser.
    pub message: Cow<'a, str>,
    /// Display facts from the resolved database target, preserved across replay.
    pub schedule: ScheduleFireDescription<'a>,
}
