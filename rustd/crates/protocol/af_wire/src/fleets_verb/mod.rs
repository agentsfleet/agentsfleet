//! The runner's fleets verb: the fleet a run runs reading which fleets its
//! workspace holds, and what one of them did recently.
//!
//! Both reads are run-proven and scoped to the run's workspace. Another
//! fleet's words are cut to an excerpt here, so the runner's tool and the
//! daemon's route agree on how much of them one read carries.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

/// The most bytes of another fleet's request or answer one event row carries.
///
/// A digest needs a line, not a transcript, and the full answer stays in the
/// dashboard. Kept small because a model reads every row it is handed.
pub const EXCERPT_MAX_BYTES: usize = 1024;

/// `text` cut to at most [`EXCERPT_MAX_BYTES`], never inside a character.
#[must_use]
pub fn excerpt(text: &str) -> &str {
    let (kept, _rest) = text.split_at(text.floor_char_boundary(EXCERPT_MAX_BYTES));
    kept
}

/// One fleet of the run's workspace.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FleetItem<'a> {
    /// The fleet's identifier: what `fleet_steer` and the events read take.
    #[serde(borrow)]
    pub fleet_id: Cow<'a, str>,
    /// Its saved name in this workspace.
    #[serde(borrow)]
    pub name: Cow<'a, str>,
    /// Where it stands: `installing`, `active` or `archived`.
    #[serde(borrow)]
    pub status: Cow<'a, str>,
    /// What it is for, from its saved setup. Omitted while it has none.
    #[serde(borrow, default, skip_serializing_if = "Option::is_none")]
    pub description: Option<Cow<'a, str>>,
    /// How many events it has processed, as a decimal string.
    #[serde(borrow)]
    pub events_processed: Cow<'a, str>,
}

/// One page of the run's workspace's fleets, newest first.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FleetsPage<'a> {
    /// The fleets on this page.
    pub items: Vec<FleetItem<'a>>,
    /// Unknown without a separate count; always null.
    pub total: Option<u64>,
    /// Pass back as `starting_after` for the next page; null on the last one.
    #[serde(borrow)]
    pub next_cursor: Option<Cow<'a, str>>,
}

/// One event of a fleet, as another fleet reads it.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FleetEventItem<'a> {
    /// The event's identifier.
    #[serde(borrow)]
    pub event_id: Cow<'a, str>,
    /// Who or what produced it: `steer:<user>`, `fleet:<fleet_id>`, `cron:<id>`.
    #[serde(borrow)]
    pub actor: Cow<'a, str>,
    /// How it entered: `chat`, `webhook`, `cron` or `continuation`.
    #[serde(borrow)]
    pub event_type: Cow<'a, str>,
    /// Where its run got to; `queued` while it waits for a runner.
    #[serde(borrow)]
    pub status: Cow<'a, str>,
    /// When it was admitted.
    #[serde(with = "crate::instant")]
    pub created_at: jiff::Timestamp,
    /// What it was asked, cut to [`EXCERPT_MAX_BYTES`].
    #[serde(borrow)]
    pub request_excerpt: Cow<'a, str>,
    /// What it answered, cut to [`EXCERPT_MAX_BYTES`]. Omitted until it answers.
    #[serde(borrow, default, skip_serializing_if = "Option::is_none")]
    pub response_excerpt: Option<Cow<'a, str>>,
    /// What refused or failed the run. Omitted on a clean one.
    #[serde(borrow, default, skip_serializing_if = "Option::is_none")]
    pub failure_label: Option<Cow<'a, str>>,
}

/// One page of a fleet's events, newest first.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FleetEventsPage<'a> {
    /// The events on this page.
    pub items: Vec<FleetEventItem<'a>>,
    /// Unknown without a separate count; always null.
    pub total: Option<u64>,
    /// Pass back as `starting_after` for the next page; null on the last one.
    #[serde(borrow)]
    pub next_cursor: Option<Cow<'a, str>>,
}

#[cfg(test)]
mod tests;
