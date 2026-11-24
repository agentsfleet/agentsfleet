//! What may wake a fleet, as its `fleet.triggers` rows hold it.

use std::borrow::Cow;

use af_wire::trigger_verb::TriggerKind;
use serde::Serialize;

/// One trigger: a webhook source, the GitHub App subscription, or one Slack
/// channel. The fleet adds it by tool; a person lists and removes it.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Trigger<'a> {
    /// The trigger's identity: what a remove names.
    pub id: Cow<'a, str>,
    /// What wakes the fleet.
    pub kind: TriggerKind,
    /// A webhook's source slug, `github` for an App event, `slack` for a mention.
    pub source: Cow<'a, str>,
    /// The event names that wake the fleet, or `null` for every event.
    pub events: Option<Vec<Cow<'a, str>>>,
    /// The repositories whose App events wake the fleet, or `null`.
    pub repositories: Option<Vec<Cow<'a, str>>>,
    /// A mention's channel identifier, or `null`.
    pub channel: Option<Cow<'a, str>>,
    /// A webhook's delivery address, `/v1/webhooks/{fleet_id}/{source}` on this
    /// deployment, or `null` for another kind.
    pub url: Option<Cow<'a, str>>,
    /// When the fleet added it, such as `2026-10-24T05:00:00.000Z`.
    #[serde(with = "af_wire::instant")]
    pub created_at: jiff::Timestamp,
}

/// `GET …/fleets/{fleet_id}/triggers`, and the run verb's list.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TriggersResponse<'a> {
    /// The fleet's triggers, oldest first.
    pub items: Vec<Trigger<'a>>,
}
