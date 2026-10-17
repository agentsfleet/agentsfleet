//! How a fleet's schedules are rendered to the people who own them.
//!
//! The dashboard and the CLI both read this surface, which is why the shape
//! lives here rather than beside the handler that fills it.

use std::borrow::Cow;

use serde::Serialize;

/// One schedule, as the tenant surface renders it.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize)]
pub struct View<'s> {
    /// Its identity.
    #[serde(borrow)]
    pub schedule_id: Cow<'s, str>,
    /// The fleet it wakes.
    #[serde(borrow)]
    pub fleet_id: Cow<'s, str>,
    /// The expression it fires on.
    #[serde(borrow)]
    pub cron: Cow<'s, str>,
    /// The zone that expression is read in.
    #[serde(borrow)]
    pub timezone: Cow<'s, str>,
    /// What the fleet is asked to do.
    #[serde(borrow)]
    pub message: Cow<'s, str>,
    /// Who made it: `api` for a person, `trigger` for the fleet's own
    /// document, `fleet` for the fleet while it ran. Only a `fleet` schedule
    /// can be changed by the fleet.
    #[serde(borrow)]
    pub source: Cow<'s, str>,
    /// Whether it retires after its first fire.
    pub once: bool,
    /// What the operator wants it to be doing.
    #[serde(borrow)]
    pub status: Cow<'s, str>,
    // Rendered rather than hidden: a schedule that saved and did not register
    // is the one state a person needs to see, and a view that showed only the
    // intent would report a schedule as live when it fires nowhere.
    /// How far agentsfleet has brought the external scheduler in line.
    ///
    /// A schedule can save without registering. This field shows you that
    /// state.
    #[serde(borrow)]
    pub sync: Cow<'s, str>,
    /// Strong tag binding the pinned recipe, setup and instance override.
    pub etag: String,
    /// Explicit instance overlay, omitted when the pinned recipe is inherited.
    #[serde(rename = "override", skip_serializing_if = "Option::is_none")]
    pub overrides: Option<super::Override>,
    /// Why the last push failed, when one did.
    #[serde(borrow)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<Cow<'s, str>>,
    /// When it was created.
    #[serde(with = "afd_wire::instant")]
    pub created_at: jiff::Timestamp,
    /// When it was last changed.
    #[serde(with = "afd_wire::instant")]
    pub updated_at: jiff::Timestamp,
}

/// A page of schedules.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize)]
pub struct Page<'s> {
    /// The fleet's schedules, oldest first.
    #[serde(borrow)]
    pub schedules: Vec<View<'s>>,
}
