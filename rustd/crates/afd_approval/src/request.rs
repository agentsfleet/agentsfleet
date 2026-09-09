//! The verb that was missing: asking for a grant a person can give.
//!
//! # Why this crate, beside the revoke
//!
//! [`crate::grant`]'s module note already argues it: this crate was the grant
//! table's writer before either verb existed, because `sql::RESOLVE_GATE` moves
//! a grant in the same statement that answers its gate. The request is the
//! third direction of that one authority, and putting it on the lease path
//! instead would give `core.integration_grants` a second writer whose writes
//! the resolve would have to trust.
//!
//! # What was actually broken
//!
//! The approve half shipped complete. `RESOLVE_GATE`'s `granted` arm moves a
//! grant when a person answers a gate of kind [`KIND_INTEGRATION_GRANT`], and
//! it has been correct and unreachable — every `INSERT INTO
//! core.integration_grants` in the repository was in a test file, so no row
//! ever existed for it to move. A fleet declaring a mintable credential parked
//! its delivery on a grant that could not be created, redelivering every second
//! against a dashboard that reported it healthy.
//!
//! # The card carries the key the approve statement matches on
//!
//! `RESOLVE_GATE` joins `g.service = r.evidence->>'service'`. A card written
//! without that key resolves cleanly, moves no grant, and leaves the fleet
//! exactly where it was — a failure with no error, which is the class this
//! module exists to end. [`Wanted::evidence`] is the only place that key is
//! written, and it is spelled from the same constant the statement is bound
//! with.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_wire::grant::status;
use sqlx::Row as _;

use crate::grant::IntegrationGrants;
use crate::{Result, error, sql};

/// Statement name, for the context a query failure carries.
const CONTEXT_REQUEST: &str = "grant.request";

/// The gate kind whose approval also moves an integration grant.
///
/// Declared here and imported by [`crate::inbox`], which binds it into
/// `RESOLVE_GATE`'s `granted` arm: the raise and the resolve must spell one
/// word, and a second copy is a card one half writes and the other stops
/// matching (RULE UFS).
pub const KIND_INTEGRATION_GRANT: &str = "integration_grant";

/// The `evidence` key naming the third party a card is about.
///
/// The Rust half of a key that is also spelled inside two statements —
/// `RESOLVE_GATE`'s join and `REQUEST_GRANT`'s guard both reach it as
/// `evidence->>'service'`. Those two are SQL text, which this crate keeps
/// verbatim rather than assembled, so the const cannot be bound into them
/// without making the text something a reader has to reconstruct.
///
/// What holds the three in agreement is a test rather than the type system:
/// `tests::evidence_carries_the_key_the_approve_statement_joins_on` reads it out
/// of the rendered object, `a_request_writes_the_grant_and_the_card_together`
/// reads it back out of the column, and `approving_the_card_grants_the_integration`
/// proves the join actually matches the row this writes. A drift in any one of
/// the three fails all three.
///
/// Private: the spellings it must agree with are this crate's own SQL, so
/// nothing outside has a use for the name — only for the behaviour it buys.
const EVIDENCE_SERVICE: &str = "service";

/// A grant request was written, and a person now owes an answer.
const EVENT_REQUESTED: &str = "grant_requested";

/// A request found a card already open and wrote nothing.
const EVENT_SUPPRESSED: &str = "grant_request_suppressed";

/// A grant row carries a status this build has no arm for.
const EVENT_STATUS_UNKNOWN: &str = "grant_status_unrecognised";

/// How long a grant card stands before the sweeper may take it.
///
/// Thirty days, and deliberately not the hour an event gate gets
/// (`afd_fleet_runtime::config::DEFAULT_TIMEOUT_MS`). That hour bounds a
/// question about ONE delivery, where a run is waiting and a stale answer is
/// worse than none. This card asks for a STANDING authorisation, raised at
/// install for a person who may not be at their desk, and an hour would expire
/// it before they read it — leaving the fleet exactly as unable to run as it
/// was, which is the failure this module exists to end.
const GRANT_DECISION_WINDOW_MS: i64 = 30 * 24 * 60 * 60 * 1_000;

/// The card's headline, before the service it names.
const PROPOSED_ACTION_PREFIX: &str = "mint short-lived credentials for ";

/// The card's blast radius, around the service it names.
const RADIUS_PREFIX: &str = "every credential this fleet mints for ";
const RADIUS_SUFFIX: &str = ", until a person revokes the grant";

/// Where a request came from, and the provenance it records.
///
/// A closed pair rather than a reason string and an origin label travelling
/// beside each other: they are one fact read two ways — an operator reads the
/// sentence on the card, a dashboard counts the label — and two fields would
/// let a card say it came from the install while the metric said it came from a
/// park.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// The bundle declared the credential and the fleet was installed.
    Install,
    /// A delivery reached the credential and found no grant.
    Park,
}

impl Origin {
    /// The reason stored on the grant row.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::Install => REASON_DECLARED_AT_INSTALL,
            Self::Park => REASON_WANTED_BY_A_DELIVERY,
        }
    }

    /// The label an operator's metric counts this request under.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Install => "install",
            Self::Park => "park",
        }
    }
}

/// The provenance an install-time request records.
///
/// The vocabulary the codebase already assumed: `afd_wire`'s own wire fixture
/// and this crate's grant suite both carry this sentence, and both were
/// test-only until a production writer existed to mean it.
pub const REASON_DECLARED_AT_INSTALL: &str = "Declared by the fleet bundle at install";

/// The provenance the park-time backstop records.
///
/// Private, unlike [`REASON_DECLARED_AT_INSTALL`]: the install sentence predates
/// this module and three test crates already assert against it by name, while
/// this one is reachable where it matters through [`Origin::reason`].
const REASON_WANTED_BY_A_DELIVERY: &str = "Wanted by a delivery that could not run";

/// One integration a fleet needs standing permission to mint against.
#[derive(Debug, Clone, Copy)]
pub struct Wanted<'a> {
    /// The connector's own name — `github`, `zoho` — as
    /// `afd_credential::secrets::connector::Connector::name` spells it.
    ///
    /// This is the value `RESOLVE_GATE` matches the grant row on, so it is the
    /// service and never the fleet's own name for the credential.
    pub service: &'a str,
    /// The name the fleet declared the credential under.
    ///
    /// Card copy only. `github` for the service and `gh` for the declaration is
    /// ordinary, and a person answering wants to see which of their fleet's
    /// declarations is asking.
    pub credential: &'a str,
    /// Where the request came from.
    pub origin: Origin,
}

impl Wanted<'_> {
    /// The `evidence` body, carrying the one key the approve statement joins on.
    ///
    /// Rendered rather than bound as a value: the statement casts `$14::jsonb`,
    /// which is how every other gate insert in this workspace writes the column
    /// — the driver's JSON binding is not compiled in, and turning it on for
    /// one object would be a feature the rest of the daemon does not use.
    fn evidence(&self) -> String {
        serde_json::json!({ EVIDENCE_SERVICE: self.service }).to_string()
    }
}

/// What a request found, and therefore what its caller does next.
///
/// Four arms rather than a `bool`, because the park path turns on the
/// difference: a pending grant means wait, and a revoked one means a person
/// said no and the event must END rather than ask again every second.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Requested {
    /// The grant, the card, or both were written. A person now owes an answer.
    Raised,
    /// A card was already open for this service. Nothing was written.
    Pending,
    /// The fleet already holds a standing yes; there was nothing to ask.
    Approved,
    /// A person answered no. Asking again would talk over them.
    Denied,
}

impl IntegrationGrants {
    /// Ask for one grant, and raise the card a person answers it on.
    ///
    /// Idempotent at both write sites and by two different mechanisms, which is
    /// the point: the grant row is held to one per `(fleet, service)` by the
    /// table's own unique constraint, and the card by the statement's
    /// `NOT EXISTS`. A delivery re-parking at the one-second redelivery cadence
    /// therefore raises one question, not one per second — and neither guard is
    /// a rate limit, so a card a person answers is replaced by the next real
    /// request rather than swallowed by a window.
    ///
    /// # Preconditions
    /// `fleet` MUST belong to `workspace`. Unlike this crate's two
    /// tenant-facing grant verbs, the statement does not re-derive that —
    /// `sql::SELECT_FLEET_IN_WORKSPACE` says why — so a caller holding an
    /// untrusted pair must check it before asking. Today's two callers each
    /// take both identifiers from a single trusted row, so no endpoint can
    /// route a caller-supplied pair here; the card would otherwise land in a
    /// workspace's inbox that does not own the fleet it names.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and an identifier that could
    /// not be minted.
    pub async fn request(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        wanted: Wanted<'_>,
        now: UnixMillis,
    ) -> Result<Requested> {
        // Hoisted, not drawn inside the chain: three identifiers are minted
        // here and each is fallible, so naming them says which row is which and
        // keeps the failure ahead of the statement rather than inside it.
        let grant_id = self.mint(now)?;
        let gate_id = self.mint(now)?;
        let action_id = self.mint(now)?;
        let mut connection = self.database().acquire().await?;
        let row = sqlx::query(sql::REQUEST_GRANT)
            .bind(grant_id.as_str())
            .bind(fleet.as_str())
            .bind(wanted.service)
            .bind(status::PENDING)
            .bind(wanted.origin.reason())
            .bind(now.as_millis())
            .bind(gate_id.as_str())
            .bind(workspace.as_str())
            .bind(action_id.as_str())
            .bind(wanted.service)
            .bind(wanted.credential)
            .bind(KIND_INTEGRATION_GRANT)
            .bind(format!("{PROPOSED_ACTION_PREFIX}{}", wanted.service))
            .bind(wanted.evidence())
            .bind(format!("{RADIUS_PREFIX}{}{RADIUS_SUFFIX}", wanted.service))
            .bind(
                now.saturating_add_millis(GRANT_DECISION_WINDOW_MS)
                    .as_millis(),
            )
            .bind(afd_wire::approval::status::PENDING)
            .fetch_one(&mut *connection)
            .await
            .map_err(error::query(CONTEXT_REQUEST))?;

        let unreadable = error::query(CONTEXT_REQUEST);
        let raised: i64 = row.try_get(0).map_err(&unreadable)?;
        let found: Option<String> = row.try_get(1).map_err(&unreadable)?;
        let outcome = settle(found.as_deref(), raised > 0, fleet, wanted.service);
        report(outcome, fleet, &wanted);
        Ok(outcome)
    }

    /// Draws one identifier for a row this statement writes.
    ///
    /// Three per request — the grant, the gate, and the action a person answers
    /// about — all through the workspace's one entropy surface, for the reason
    /// [`afd_gate`]'s park mints its pair there: a second source is a second
    /// failure mode on a path that already has one.
    fn mint(&self, now: UnixMillis) -> Result<Uuid7> {
        Ok(Uuid7::encode(now, self.entropy().uuid_randomness()?)?)
    }
}

/// What the statement found, as the answer a caller acts on.
///
/// `found` is the status the row held BEFORE this statement ran — the select
/// and the writes share one snapshot — so a freshly written grant reads as
/// absent here, and that is what makes an absent row and a re-raised card the
/// same [`Requested::Raised`].
///
/// `raised` therefore separates only the two cases an absent status cannot: a
/// still-pending grant whose card the sweeper expired (re-raised, so `Raised`)
/// from one whose card is still open (`Pending`). It is deliberately NOT
/// consulted when the row is absent — the loser of a concurrent request writes
/// no card and sees no grant, and the question it did not raise is standing all
/// the same.
fn settle(found: Option<&str>, raised: bool, fleet: &Uuid7, service: &str) -> Requested {
    match found {
        None => Requested::Raised,
        Some(status::PENDING) if raised => Requested::Raised,
        Some(status::PENDING) => Requested::Pending,
        Some(status::APPROVED) => Requested::Approved,
        Some(status::REVOKED) => Requested::Denied,
        // A spelling this build has no arm for. Waiting is the fail-safe
        // direction — an unknown status must never be read as a person's no,
        // which would end an event nobody answered.
        Some(unknown) => {
            let fleet_id = fleet.as_str();
            let status = unknown.to_owned();
            tracing::warn!(
                event = EVENT_STATUS_UNKNOWN,
                fleet_id,
                service,
                status,
                "a grant row carries a status this build cannot place; the event waits"
            );
            Requested::Pending
        }
    }
}

/// Says what the request did, in the two lines an operator counts.
fn report(outcome: Requested, fleet: &Uuid7, wanted: &Wanted<'_>) {
    let fleet_id = fleet.as_str();
    let service = wanted.service;
    let origin = wanted.origin.as_str();
    match outcome {
        Requested::Raised => tracing::info!(
            event = EVENT_REQUESTED,
            fleet_id,
            service,
            origin,
            "a grant was requested and a person now owes an answer"
        ),
        Requested::Pending => tracing::debug!(
            event = EVENT_SUPPRESSED,
            fleet_id,
            service,
            origin,
            "a card is already open for this service; nothing was written"
        ),
        Requested::Approved | Requested::Denied => (),
    }
}

#[cfg(test)]
mod tests;
