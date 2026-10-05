//! The runner's schedules verb: a fleet keeping its own follow-ups on the
//! daemon's schedule plane, through the lease it runs under.
//!
//! The fleet is never named here. It is the lease's, proved by the daemon from
//! the path's `lease_id`, so a body cannot reach another fleet's schedules. A
//! read and a delete carry the fence in the query; a create, an edit and a run
//! carry it in the body. The views a reply carries are the tenant surface's
//! own [`crate::schedule::View`], so a person and a fleet read one shape.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

/// `POST /v1/runners/me/leases/{lease_id}/schedules` request.
//
// The three text fields are bounded by `afd_cron::validate::Fields` at the
// daemon, the one place a schedule's grammar is decided, as the tenant create
// is.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScheduleCreateRequest<'a> {
    /// The lease's fencing token; a holder the fleet has superseded is refused.
    pub fencing_token: u64,
    /// A five-field cron expression.
    #[serde(borrow)]
    pub cron: Cow<'a, str>,
    /// The zone the expression is read in; UTC when absent.
    #[serde(borrow, default)]
    pub timezone: Option<Cow<'a, str>>,
    /// What the fleet is asked to do when it fires.
    #[serde(borrow)]
    pub message: Cow<'a, str>,
    /// Whether it retires after its first fire.
    #[serde(default)]
    pub once: bool,
}

/// `PATCH /v1/runners/me/leases/{lease_id}/schedules/{schedule_id}` request.
///
/// Every field but the fence is optional, and an absent one is left alone.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchedulePatchRequest<'a> {
    /// The lease's fencing token; a holder the fleet has superseded is refused.
    pub fencing_token: u64,
    /// A new expression.
    #[serde(borrow, default)]
    pub cron: Option<Cow<'a, str>>,
    /// A new zone.
    #[serde(borrow, default)]
    pub timezone: Option<Cow<'a, str>>,
    /// A new message.
    #[serde(borrow, default)]
    pub message: Option<Cow<'a, str>>,
    /// Whether it should stop firing.
    #[serde(default)]
    pub paused: Option<bool>,
}

/// `POST /v1/runners/me/leases/{lease_id}/schedules/{schedule_id}/runs`
/// request: run the schedule now.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScheduleRunRequest {
    /// The lease's fencing token; a holder the fleet has superseded is refused.
    pub fencing_token: u64,
}

/// What running a schedule now admitted.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduleRun<'a> {
    /// The event the run will execute as.
    #[serde(borrow)]
    pub event_id: Cow<'a, str>,
}

/// The query parameter that carries the lease's fencing token.
///
/// A schedules read, a delete and a runs read carry no body, so the fence
/// rides the query; the page parameters beside it on a runs read are the
/// keyset pair every list takes (`afd_core::paging`).
pub const QUERY_FENCING_TOKEN: &str = "fencing_token";

#[cfg(test)]
#[path = "schedule_verb/tests.rs"]
mod tests;
