//! A fleet asking for one secret or integration, and the card a person
//! answers it on.
//!
//! # Why this crate, beside the revoke
//!
//! [`crate::grant`]'s module note argues it: this crate is the grant table's
//! writer, because `sql::RESOLVE_GATE` moves a grant in the same statement
//! that answers its card. The request is the third direction of that one
//! authority, and putting it on the run path instead would give
//! `connector.integration_grants` a second writer whose writes the resolve
//! would have to trust.
//!
//! # The card carries the asking turn
//!
//! A run asks, then ends waiting, as a run's question does. The card is raised
//! with the asking turn as its `turn_id`, so answering it writes the answer
//! turn a question's answer writes, and the waiting run resumes holding the
//! access or knowing it was refused.
//!
//! # The card carries the key the approve statement matches on
//!
//! `RESOLVE_GATE` joins `g.service = r.evidence->>'service'`. A card written
//! without that key resolves cleanly, moves no grant, and leaves the fleet
//! exactly where it was — a failure with no error. [`Wanted::evidence`] is the
//! only place that key is written.

use std::time::Duration;

use af_core::id::Uuid7;
use af_core::timing::DAY;
use afd_api_wire::grant::grant_type;
use jiff::Timestamp;
use sqlx::Acquire as _;

use crate::grant::IntegrationGrants;
use crate::{Result, error};

/// Statement name, for the context a query failure carries.
const CONTEXT_REQUEST: &str = "grant.request";

/// The gate kind whose answer also moves an integration grant.
///
/// Declared here and imported by [`crate::inbox`], which binds it into
/// `RESOLVE_GATE`'s `granted` arm: the raise and the resolve must spell one
/// word, and a second copy is a card one half writes and the other stops
/// matching (RULE UFS).
pub const KIND_INTEGRATION_GRANT: &str = "integration_grant";

/// The `evidence` keys a card carries: the name `RESOLVE_GATE` joins on, the
/// channel it reaches a run through, and the scope asked for.
const EVIDENCE_SERVICE: &str = "service";
const EVIDENCE_KIND: &str = "kind";
const EVIDENCE_SCOPE: &str = "scope";

/// A grant request raised a card, and a person now owes an answer.
const EVENT_REQUESTED: &str = "grant_requested";

/// A request found a card already open and raised none.
const EVENT_SUPPRESSED: &str = "grant_request_suppressed";

/// How long a grant card stands before the sweeper may take it.
///
/// Thirty days: the card asks for a STANDING authorisation that a person may
/// not read for days, and the run waits on it as it waits on a question. A
/// span the statement adds to the database clock, so the card's deadline is
/// stamped by the same clock the sweeper later reads it against.
///
/// Kept: `fleet.approval_gates.expires_at` is `NOT NULL` and read back as a timestamp.
const GRANT_DECISION_WINDOW: Duration = DAY.saturating_mul(GRANT_DECISION_DAYS);

/// The days in [`GRANT_DECISION_WINDOW`].
const GRANT_DECISION_DAYS: u32 = 30;

/// The provenance a fleet's request records on the grant row.
pub const REASON_ASKED_BY_THE_FLEET: &str = "Asked for by the fleet in its chat";

/// Which channel an approved grant reaches a run through.
///
/// Derived by the caller from the vault entry or the connected integration
/// the name resolves to, never chosen by the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantKind {
    /// A workspace vault entry whose stored value ships to the run.
    Secret,
    /// A connected integration whose token is minted per run.
    Integration,
}

impl GrantKind {
    /// The spelling `connector.integration_grants.grant_type` stores.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Secret => grant_type::SECRET,
            Self::Integration => grant_type::INTEGRATION,
        }
    }

    /// The card's headline and blast radius for `name`.
    fn card(self, name: &str) -> (String, String) {
        match self {
            Self::Secret => (
                format!("use the workspace secret {name}"),
                format!("every run of this fleet holds {name}, until a person revokes the grant"),
            ),
            Self::Integration => (
                format!("mint short-lived credentials for {name}"),
                format!(
                    "every credential this fleet mints for {name}, until a person revokes the grant"
                ),
            ),
        }
    }
}

/// One secret or integration a fleet asks to hold.
#[derive(Debug, Clone, Copy)]
pub struct Wanted<'a> {
    /// The vault key or the integration's name — `sentry`, `github` — which is
    /// the grant row's `service` and the key `RESOLVE_GATE` matches it on.
    pub name: &'a str,
    /// Which channel it reaches a run through.
    pub kind: GrantKind,
    /// The scope asked for, in the JSON shape
    /// `af_fleet_runtime::config::RepositoryBinding::recorded` writes, or
    /// `None` for a name that takes no scope.
    pub scope: Option<&'a str>,
    /// The turn whose run asked: the card carries it, and its answer resumes it.
    pub turn: &'a Uuid7,
}

impl Wanted<'_> {
    /// The `evidence` body, carrying the one key the approve statement joins on.
    ///
    /// Rendered rather than bound as a value: the statement casts `::jsonb`,
    /// which is how every other gate insert in this workspace writes the
    /// column. A scope that is not JSON renders as `null`; the caller parsed it
    /// before asking, so that is a bug's trace on the card, never a refusal.
    fn evidence(&self) -> String {
        let scope = self
            .scope
            .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok());
        serde_json::json!({
            EVIDENCE_SERVICE: self.name,
            EVIDENCE_KIND: self.kind.as_str(),
            EVIDENCE_SCOPE: scope,
        })
        .to_string()
    }
}

/// What a request did, and therefore what the run does next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Requested {
    /// A card was raised; the run ends waiting on its answer.
    Raised {
        /// The fleet's grant for the name.
        grant: Uuid7,
        /// The card a person answers.
        gate: Uuid7,
    },
    /// A card for this grant is already open, so none was raised.
    Pending {
        /// The fleet's grant for the name.
        grant: Uuid7,
        /// The open card, or `None` when a concurrent request raised it after
        /// this one looked.
        gate: Option<Uuid7>,
    },
    /// The approved grant already holds what was asked; the run continues.
    Covered {
        /// The fleet's grant for the name.
        grant: Uuid7,
    },
}

impl Requested {
    /// The fleet's grant for the name, whatever the request did.
    #[must_use]
    pub const fn grant(&self) -> &Uuid7 {
        match self {
            Self::Raised { grant, .. } | Self::Pending { grant, .. } | Self::Covered { grant } => {
                grant
            }
        }
    }

    /// The card a person answers, when one is open.
    #[must_use]
    pub const fn gate(&self) -> Option<&Uuid7> {
        match self {
            Self::Raised { gate, .. } => Some(gate),
            Self::Pending { gate, .. } => gate.as_ref(),
            Self::Covered { .. } => None,
        }
    }
}

impl IntegrationGrants {
    /// Ask a person for one grant, and raise the card they answer it on.
    ///
    /// The grant and the card land in one transaction. A revoked grant is
    /// asked again: it moves back to `pending` with a new card. A pending
    /// grant with an open card raises none, and an approved grant raises one
    /// only when a scope it does not hold is asked for; until that card is
    /// approved the approved scope stays in force. `now` mints the rows'
    /// identifiers and nothing else; every stamp is the database's.
    ///
    /// # Preconditions
    /// `fleet` MUST belong to `workspace` and `wanted.turn` to `fleet`: the
    /// statement re-derives neither, so a caller takes all three from the
    /// fenced run it serves. A scoped request MUST be one the approved scope
    /// does not already cover, which the caller decides with the scope parser;
    /// this store compares the stored and asked scopes only for equality.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and an identifier that could
    /// not be minted.
    pub async fn request(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        wanted: Wanted<'_>,
        now: Timestamp,
    ) -> Result<Requested> {
        let mut connection = self.database().acquire().await?;
        let mut transaction = connection
            .begin()
            .await
            .map_err(error::query(CONTEXT_REQUEST))?;
        let outcome = self
            .request_on(&mut transaction, workspace, fleet, &wanted, now)
            .await?;
        transaction
            .commit()
            .await
            .map_err(error::query(CONTEXT_REQUEST))?;
        report(&outcome, fleet, &wanted);
        Ok(outcome)
    }

    /// Draws one identifier for a row this request writes.
    ///
    /// Three per request — the grant, the card, and the action a person
    /// answers about — all through the workspace's one entropy surface: a
    /// second source is a second failure mode on a path that already has one.
    fn mint(&self, now: Timestamp) -> Result<Uuid7> {
        Ok(self.entropy().uuid7(now)?)
    }
}

/// Says what the request did, in the two lines an operator counts.
fn report(outcome: &Requested, fleet: &Uuid7, wanted: &Wanted<'_>) {
    let fleet_id = fleet.as_str();
    let kind = wanted.kind.as_str();
    match outcome {
        Requested::Raised { .. } => {
            let (event, message) = (
                &EVENT_REQUESTED,
                format_args!("a grant was requested and a person now owes an answer"),
            );
            tracing::info!(message, event, fleet_id, kind);
        }
        Requested::Pending { .. } => {
            let (event, message) = (
                &EVENT_SUPPRESSED,
                format_args!("a card is already open for this grant; none was raised"),
            );
            tracing::debug!(message, event, fleet_id, kind);
        }
        Requested::Covered { .. } => (),
    }
}

mod ask;

#[cfg(test)]
mod tests;
