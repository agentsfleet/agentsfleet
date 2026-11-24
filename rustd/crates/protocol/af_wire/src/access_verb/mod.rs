//! The runner's access verb: a fleet asking for one secret or integration,
//! which a person approves once on a card in its chat.
//!
//! The fleet is the run's, proved by the daemon from the path's `run_id`. A
//! request the fleet's approved grant already covers answers at once; any
//! other sets the grant pending and raises a card carrying the asking turn,
//! and the run ends waiting for the person's answer.

use std::borrow::Cow;

use af_validate::nul_free;
use garde::Validate;
use serde::{Deserialize, Serialize};

/// `POST /v1/runners/me/runs/{run_id}/access` request.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct AccessRequest<'a> {
    /// The run's fencing token; a holder the fleet has superseded is refused.
    #[garde(skip)]
    pub fencing_token: u64,
    /// The workspace vault key or connected integration, such as `sentry` or
    /// `github`.
    #[serde(borrow)]
    #[cfg_attr(feature = "openapi", schema(min_length = 1))]
    #[garde(length(bytes, min = 1), custom(nul_free))]
    pub name: Cow<'a, str>,
    /// The GitHub scope, `{repositories, access, base?}`; required for
    /// `github` and refused for any other name. The daemon reads it through
    /// `af_fleet_runtime::config::RepositoryBinding::parse`.
    #[cfg_attr(feature = "openapi", schema(value_type = Option<Object>))]
    #[serde(default)]
    #[garde(skip)]
    pub scope: Option<serde_json::Map<String, serde_json::Value>>,
}

/// What an access request did.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessRequested<'a> {
    /// The fleet's grant for the name.
    #[serde(borrow)]
    pub grant_id: Cow<'a, str>,
    /// The card a person answers; absent when the approved grant already
    /// covers the request, so the run continues holding it.
    #[serde(borrow)]
    pub gate_id: Option<Cow<'a, str>>,
}

#[cfg(test)]
mod tests;
