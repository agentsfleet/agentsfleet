//! Raising the question: the durable gate row, and the reference that finds it.
//!
//! # Two writes, in an order that is a safety property
//!
//! The durable row goes down BEFORE anything points a human at it. That row is
//! what a resolve updates and what the write-scoped mint spends against, so a
//! card reaching a reviewer ahead of it would be answerable and worthless — the
//! click would find nothing to resolve and the mint nothing to honour. Failing
//! closed here costs one poll; the other order costs a human's decision.
//!
//! # Any datastore loss is [`Parked::Unavailable`]
//!
//! Fail closed, default-deny. Never a silently released run, and never a
//! question whose answer has nowhere to land. The caller answers no-work and
//! the next poll tries again.
//!
//! # Two writes, where `approval_gate_park.zig` makes four
//!
//! The Zig also writes `fleet:gate:pending:{fleet}:{action}` — a three-field
//! summary of the detail — and `fleet:gate:notify:{fleet}:{action}`, a rendered
//! Slack message staged "for the provider to pick up". **Nothing reads either
//! one.** Not the daemon, not the sweeper, not the resolver, not the tenant
//! plane, not the web application: the pending key has one writer and one
//! `DEL`, and the notify key has one writer and no reader at all. They are the
//! residue of the gate's original design, in which the event loop blocked on
//! `BRPOP fleet:gate:response:{action}` until a human answered — a shape the
//! async gate replaced, and whose reader went with it.
//!
//! So they are not ported, and this is not a judgment about whether
//! notification should be staged in Redis. It is that porting a write with no
//! reader would add two round trips to every park, two key shapes for the
//! sweeper and the resolver to agree on, and a `DEL` on the refusal path, to
//! reproduce bytes no code has ever read. Registered as a divergence rather
//! than done quietly, because "row-equivalent" is this milestone's graded
//! claim and a Redis key is not a row.

use afd_core::clock::UnixMillis;
use afd_core::id::{ENTROPY_LEN, Uuid7};

use crate::error::{Error, Result, query};
use crate::gate::claim::Claim;
use crate::gate::detail::Stated;
use crate::gate::pending::GateRef;
use crate::gate::store::Gates;
use super::sql;

/// Statement name, for the context a query failure carries.
const CONTEXT_PARK: &str = "approval gate park";

/// A gate could not be raised because a datastore would not answer.
const EVENT_PARK_UNAVAILABLE: &str = "gate_park_unavailable";

/// A gate was raised and a human now owes an answer.
const EVENT_PARK_PENDING: &str = "gate_pending";

/// Which write could not be made, for the line an operator reads.
///
/// Named rather than inlined at the two call sites: an operator responds to
/// both the same way, but knowing WHICH store dropped is the difference between
/// suspecting Postgres and suspecting Redis.
const WRITE_ROW: &str = "durable_row";
/// See [`WRITE_ROW`].
const WRITE_REFERENCE: &str = "event_reference";

/// The spend count a bounded approval opens at.
///
/// Zero, and `None` for an approval that funds no spending at all — the column
/// pair is `NULL`/`NULL` or `0`/`ceiling`, never one of each. The schema's
/// append-only trigger fixes both before resolution.
const SPEND_OPENS_AT: i64 = 0;

/// What a park attempt produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parked {
    /// The question is recorded and a human can answer it. The reference is
    /// what every later poll resolves through.
    Awaiting(GateRef),
    /// A datastore would not answer. The event neither ran nor ended.
    Unavailable,
}

/// One question, with both halves of the card it will be read from.
///
/// A parameter bundle rather than seven positional arguments: three of them are
/// identifiers of the same shape, which compile clean in any order.
#[derive(Debug, Clone, Copy)]
pub struct Park<'a> {
    /// The fleet whose work is being held.
    pub fleet_id: &'a Uuid7,
    /// The workspace it belongs to.
    pub workspace_id: &'a Uuid7,
    /// The event being parked.
    pub event_id: &'a str,
    /// What the daemon and the workspace assert.
    pub stated: Stated<'a>,
    /// What the fleet's model claims.
    pub claim: &'a Claim,
}

impl Gates {
    /// Park `request`'s event behind a human's answer.
    ///
    /// Answers a [`Parked`] rather than a `Result`, and that is deliberate:
    /// every fault this can meet has exactly one correct response — fail
    /// closed — and there is no decision left for a caller to make. Handing
    /// back an `Err` would offer a choice whose only safe answer is already
    /// known here, and the three call sites would each have to remember it.
    pub async fn park(&self, request: Park<'_>, now: UnixMillis) -> Parked {
        let deadline = now.saturating_add_millis(request.stated.timeout_ms);
        let Ok(reference) = self.mint(now).map(|action| GateRef::new(action, deadline)) else {
            // An entropy or clock failure is the one fault with no store to
            // blame, and it is absorbed the same way for the same reason.
            return Self::unavailable(&request, WRITE_ROW, None);
        };

        if let Err(fault) = self.record_row(&request, &reference, now).await {
            return Self::unavailable(&request, WRITE_ROW, Some(&fault));
        }
        // Last, and the ordering the module note argues for. A reference that
        // fails to land leaves a `pending` row nothing points at: the event
        // re-polls, finds no reference, and parks again — which is why
        // `SELECT_GATE_STATUS` reads the NEWEST row for an action rather than
        // assuming one. A second card is the cost; a released run is not.
        if let Err(fault) = self
            .record(request.fleet_id, request.event_id, &reference)
            .await
        {
            return Self::unavailable(&request, WRITE_REFERENCE, Some(&fault));
        }

        let fleet = request.fleet_id.as_str();
        let action = reference.action_id().as_str();
        tracing::debug!(
            event = EVENT_PARK_PENDING,
            fleet_id = fleet,
            agentsfleet_event_id = request.event_id,
            action_id = action,
            "a human has been asked, and this event waits for the answer"
        );
        Parked::Awaiting(reference)
    }

    /// Insert the row a resolve updates and the mint spends against.
    async fn record_row(
        &self,
        request: &Park<'_>,
        reference: &GateRef,
        now: UnixMillis,
    ) -> Result<()> {
        let gate_id = self.mint(now)?;
        // Recorded so the write mint can compare the approved reach against the
        // fleet's current config without trusting anything PATCHable.
        let binding = request
            .stated
            .binding
            .map(|declared| serde_json::to_string(&declared.recorded()))
            .transpose()
            .map_err(|_shape| {
                crate::error::rejected(crate::error::DETAIL_GATE_BINDING_UNWRITABLE)
            })?;

        let mut connection = self.database().acquire().await?;
        sql::PendingRow {
            gate_id: &gate_id,
            fleet_id: request.fleet_id,
            workspace_id: request.workspace_id,
            action_id: reference.action_id(),
            stated: request.stated,
            claim: request.claim,
            deadline: reference.deadline(),
            event_id: request.event_id,
            stated_binding: binding.as_deref(),
            // Both columns or neither: a ceiling with no counter could never be
            // spent down, and a counter with no ceiling bounds nothing.
            spend_count: request.stated.spend_ceiling.map(|_| SPEND_OPENS_AT),
            now,
        }
        .bind()
        .execute(&mut *connection)
        .await
        .map_err(query(CONTEXT_PARK))?;
        Ok(())
    }

    /// Draw one identifier for a gate.
    ///
    /// Both the action a human answers about and the row that records it are
    /// minted here, through the workspace's one entropy surface rather than a
    /// second source with its own failure mode.
    fn mint(&self, now: UnixMillis) -> Result<Uuid7> {
        let mut bytes = [0u8; ENTROPY_LEN];
        self.entropy().fill(&mut bytes)?;
        Ok(Uuid7::encode(now, bytes)?)
    }

    /// Absorb a park fault into the fail-closed answer, saying what dropped.
    ///
    /// `fault` is rendered rather than propagated: what an operator needs is
    /// which store would not answer, and the value this returns already says
    /// the park did not happen.
    ///
    /// Associated rather than a method — it reads no state — and kept inside
    /// this `impl` so the three call sites spell it beside the writes it
    /// absorbs.
    fn unavailable(request: &Park<'_>, write: &'static str, fault: Option<&Error>) -> Parked {
        let fleet = request.fleet_id.as_str();
        let reason = fault.map_or_else(
            || "no identifier could be minted".to_owned(),
            Error::to_string,
        );
        tracing::warn!(
            event = EVENT_PARK_UNAVAILABLE,
            fleet_id = fleet,
            agentsfleet_event_id = request.event_id,
            write,
            reason,
            "the gate could not be raised; the event waits rather than running ungated"
        );
        Parked::Unavailable
    }
}
