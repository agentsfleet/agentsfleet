//! The runner's triggers verb: a fleet adding, listing and removing what may
//! wake it, through the run it runs under.
//!
//! The fleet is never named here. It is the run's, proved by the daemon from
//! the path's `run_id`, so a body cannot reach another fleet's triggers. A read
//! and a delete carry the fence in the query
//! ([`crate::schedule_verb::QUERY_FENCING_TOKEN`]); an add carries it in the
//! body with its call's item, so a retried add answers the first result. The
//! trigger a reply carries is the tenant surface's own
//! `afd_api_wire::fleet::Trigger`, so a person and a fleet read one shape.
//!
//! The body is primitives: the daemon reads it through
//! `af_fleet_runtime::config::parse_triggers`, the one place a trigger's
//! bounds and coherence are decided.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

/// What wakes the fleet: a signed delivery to its own URL, an event from the
/// workspace's GitHub App, or a mention in one Slack channel.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerKind {
    /// A signed delivery to `/v1/webhooks/{fleet_id}/{source}`.
    Webhook,
    /// An event the workspace's GitHub App delivers.
    AppEvent,
    /// A mention of the bot in one chat channel.
    Mention,
}

impl TriggerKind {
    /// Every kind, in declaration order.
    pub const ALL: [Self; 3] = [Self::Webhook, Self::AppEvent, Self::Mention];

    /// The spelling `fleet.triggers.kind` stores.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Webhook => "webhook",
            Self::AppEvent => "app_event",
            Self::Mention => "mention",
        }
    }
}

/// How a webhook delivery from a source outside the built-in registry proves
/// itself. It names a signing recipe, and the header that carries the signature
/// when the recipe signs the body alone.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerVerifier<'a> {
    /// The recipe, such as `body-hex-sha256` or `svix-v1`.
    #[serde(borrow)]
    pub scheme: Cow<'a, str>,
    /// The signature header, for a recipe that signs the body alone.
    #[serde(borrow, default)]
    pub header: Option<Cow<'a, str>>,
}

/// `POST /v1/runners/me/runs/{run_id}/triggers` request.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerAddRequest<'a> {
    /// The run's fencing token; a holder the fleet has superseded is refused.
    pub fencing_token: u64,
    /// The item this call is: a `UUIDv7` the runner mints once per call.
    /// A retry sends it again and gets the first result.
    #[serde(borrow)]
    pub item_id: Cow<'a, str>,
    /// The call's number within its run, from 1, as its record names it.
    pub call_number: u64,
    /// What wakes the fleet.
    pub kind: TriggerKind,
    /// A webhook's source slug, the last segment of its URL; `github` for an
    /// App event and `slack` for a mention when absent.
    #[serde(borrow, default)]
    pub source: Option<Cow<'a, str>>,
    /// The event names that wake the fleet; absent wakes it on every event.
    #[serde(borrow, default)]
    pub events: Option<Vec<Cow<'a, str>>>,
    /// The repositories whose App events wake the fleet, as `owner/name`.
    #[serde(borrow, default)]
    pub repositories: Option<Vec<Cow<'a, str>>>,
    /// A webhook's signing recipe, for a source outside the built-in registry.
    #[serde(borrow, default)]
    pub verifier: Option<TriggerVerifier<'a>>,
    /// The workspace vault key holding a webhook's signing secret.
    #[serde(borrow, default)]
    pub secret_name: Option<Cow<'a, str>>,
    /// A mention's channel identifier.
    #[serde(borrow, default)]
    pub channel: Option<Cow<'a, str>>,
}

#[cfg(test)]
mod tests;
