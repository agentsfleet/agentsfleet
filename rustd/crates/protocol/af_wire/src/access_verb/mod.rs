//! The runner's access verb: a fleet asking for one secret or integration,
//! which a person approves once on a card in its chat.
//!
//! The fleet is the run's, proved by the daemon from the path's `run_id`. A
//! request the fleet's approved grant already covers answers at once; any
//! other sets the grant pending and raises a card carrying the asking turn,
//! and the run ends waiting for the person's answer.

mod host;
pub use host::{Hostname, RedirectEvidence, RefusedTool};

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
    #[serde(default, borrow, skip_serializing_if = "Option::is_none")]
    #[garde(custom(valid_target(self.valid_target())), inner(length(bytes, min = 1), custom(nul_free)))]
    pub name: Option<Cow<'a, str>>,
    /// A host grant, mutually exclusive with a credential name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[garde(skip)]
    pub host: Option<Hostname>,
    /// Redirect evidence recorded by the runner, present only for a matching host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[garde(skip)]
    pub evidence: Option<RedirectEvidence>,
    /// The GitHub scope, `{repositories, access, base?}`; required for
    /// `github` and refused for any other name. The daemon reads it through
    /// `af_fleet_runtime::config::RepositoryBinding::parse`.
    #[cfg_attr(feature = "openapi", schema(value_type = Option<Object>))]
    #[serde(default)]
    #[garde(skip)]
    pub scope: Option<serde_json::Map<String, serde_json::Value>>,
}

impl AccessRequest<'_> {
    fn valid_target(&self) -> bool {
        match (&self.name, &self.host) {
            (Some(_), None) => self.evidence.is_none(),
            (None, Some(host)) => {
                self.scope.is_none()
                    && self
                        .evidence
                        .as_ref()
                        .is_none_or(|redirect| redirect.to_host == *host)
            }
            _ => false,
        }
    }
}

fn valid_target(valid: bool) -> impl Fn(&Option<Cow<'_, str>>, &()) -> garde::Result {
    move |_, ()| {
        if valid {
            Ok(())
        } else {
            Err(garde::Error::new(
                "provide a credential name or one host with matching evidence",
            ))
        }
    }
}

/// Which run owns the decision card, or whether the grant already covers this request.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessOutcome {
    /// This turn raised a new card and waits.
    Raised,
    /// Another turn raised the pending card.
    Pending,
    /// An approved grant already covers this request.
    Covered,
}

/// What an access request did.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessRequested<'a> {
    /// What took place independent of whether the transaction saw the card ID.
    pub state: AccessOutcome,
    /// The fleet's grant for the name.
    #[serde(borrow)]
    pub grant_id: Cow<'a, str>,
    /// The card when observed. A concurrent pending request may not yet see its ID.
    /// Runners use `state` to decide whether this turn waits or continues.
    #[serde(borrow)]
    pub gate_id: Option<Cow<'a, str>>,
}

#[cfg(test)]
mod tests;
