//! Answering a gate, and everything one answer sets off.
//!
//! Split from [`super`] on the seam between READING the queue and MOVING a
//! row. The read half is two statements and a decoder; this half is one
//! guarded UPDATE plus the three things that follow from it — the tail is
//! told, a parked delivery is woken, and an approval that held a run lands
//! the continuation. They belong together because they share one
//! invariant: the row moved first, so none of them may turn a durable
//! decision back into a retry.

use std::borrow::Cow;

use af_core::id::Uuid7;
use afd_api_wire::approval::status;
use afd_api_wire::grant::status as grant_status;
use afd_api_wire::tail::{FleetCounters, TailFrame};

use super::announce::Answer;
use super::row::{StoredResolution, StoredWin};
use super::{Inbox, Resolution, Resolved};
use crate::decision::Decision;
use crate::request::KIND_INTEGRATION_GRANT;
use crate::sql;
use crate::{Result, error};

/// The grant spellings the resolve's second arm writes.
///
/// Read from the shared vocabulary, never spelled here: [`crate::grant`] writes
/// the same column and the runner plane reads it, so a local copy of either
/// word is a row one writer produces that a reader stops matching.
const GRANT_APPROVED: &str = grant_status::APPROVED;

const GRANT_REVOKED: &str = grant_status::REVOKED;

pub(super) const CONTEXT_RESOLVE: &str = "gate.inbox.resolve";

const CONTEXT_CONTINUE: &str = "gate.inbox.continuation";

/// The actor a continuation event records.
///
/// `continuation:<the event the gate blocked>`, so the history of a run reads
/// forward: the blocked row says what was stopped, and this one says what it
/// was stopped BY and resumed from. A reader following the chain never has to
/// join back through the gate table.
const CONTINUATION_ACTOR_PREFIX: &str = "continuation:";

/// The body a continuation carries.
///
/// Empty rather than a copy of the original request: the runner re-reads the
/// blocked event's own body through `resumes_turn_id`, and duplicating it here
/// would make two rows that could disagree about what was asked.
const CONTINUATION_BODY: &str = "{}";

impl Inbox {
    /// Answers one gate, atomically.
    ///
    /// `fleet` narrows the decision to a fleet the caller proved from a trusted
    /// source. It must be `Some` wherever the action id and the fleet come from
    /// the SAME untrusted payload: without it, an actor holding a signature for
    /// one fleet could answer another's gate by guessing an action id.
    ///
    /// There is no "resolve to pending" arm to refuse: [`Decision`] cannot
    /// express it, so the statement needs no guard and no caller can ask.
    ///
    /// The answer is stamped on the database clock; nothing here reads one.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a queue that would not
    /// take the continuation an approval lands.
    pub async fn resolve(
        &self,
        action: &str,
        outcome: Decision,
        by: &str,
        detail: &str,
        fleet: Option<&str>,
    ) -> Result<Resolution> {
        // Every gate's action and fleet are UUIDs: anything else names no gate.
        let Ok(action) = Uuid7::parse(action) else {
            return Ok(Resolution::NotFound);
        };
        let Ok(scope) = fleet.map(Uuid7::parse).transpose() else {
            return Ok(Resolution::NotFound);
        };
        let (action, scope) = (&action, scope.as_ref());
        let mut connection = self.database.acquire().await?;

        let won = sqlx::query_as::<_, StoredWin>(sql::RESOLVE_GATE)
            .bind(outcome.as_str())
            .bind(detail)
            .bind(by)
            .bind(action)
            .bind(scope)
            .bind(status::APPROVED)
            .bind(GRANT_APPROVED)
            .bind(GRANT_REVOKED)
            .bind(KIND_INTEGRATION_GRANT)
            .fetch_optional(&mut *connection)
            .await
            .map_err(error::query(CONTEXT_RESOLVE))?;

        if let Some(row) = won {
            drop(connection);
            let resolved = self.won(row).await?;
            return Ok(Resolution::Resolved(resolved));
        }

        // Nothing updated: either somebody answered first, or there was never
        // a gate. The second read is what tells those apart, and it runs only
        // on the losing path so the winner pays one statement.
        let existing = sqlx::query_as::<_, StoredResolution>(sql::SELECT_GATE_BY_ACTION)
            .bind(action)
            .bind(scope)
            .fetch_optional(&mut *connection)
            .await
            .map_err(error::query(CONTEXT_RESOLVE))?;

        drop(connection);
        Ok(match existing {
            Some(row) => Resolution::AlreadyResolved(self.stood(row.into()).await?),
            None => Resolution::NotFound,
        })
    }

    /// What winning the race owes: the continuation an approval lands, and
    /// the announcement every answer does.
    ///
    /// The announcement runs whatever the continuation did. The row moved
    /// when the statement returned it, so a watcher owed the answer is owed
    /// it even when the run could not be restarted — the count on the frame
    /// is the one the statement read, and an error from the continuation is
    /// reported after the tail has heard.
    async fn won(&self, row: StoredWin) -> Result<Resolved> {
        let pending_approvals = row.pending_approvals;
        let mut resolved = Resolved::from(row.resolution);
        // The continuation is part of RESOLVING, not something a caller
        // remembers to do afterwards: an approval that landed without one is
        // a run a person unblocked and nothing restarted.
        let continuation = if resolved.status == status::APPROVED
            && let Some(turn_id) = resolved.turn_id.clone()
        {
            self.continue_from(&resolved, &turn_id).await
        } else {
            // Resolution already cleared the durable park. Notify a waiting
            // poll that the original delivery is available again.
            self.wake_parked_delivery(&resolved.fleet_id).await;
            Ok(None)
        };
        let counters =
            afd_turns::view::fleet_counters_best_effort(&self.database, &resolved.fleet_id).await;
        self.announce(Answer {
            fleet_id: &resolved.fleet_id,
            gate_id: resolved.gate_id.as_str(),
            turn_id: resolved.turn_id.as_ref().map(Uuid7::as_str),
            status: &resolved.status,
            resolved_by: &resolved.resolved_by,
            pending_approvals,
            counters,
        })
        .await;
        resolved.continuation_turn_id = continuation?;
        Ok(resolved)
    }

    /// The answer already cleared the durable park in its own transaction.
    pub(super) async fn wake_parked_delivery(&self, fleet: &Uuid7) {
        afd_bus::wake::publish(&self.bus, fleet.clone()).await;
    }

    /// Lands the event that resumes the run an approved gate had blocked.
    ///
    /// The blocked row is NEVER reopened. This is a new event carrying
    /// `resumes_turn_id`, so the history keeps both the run that was stopped
    /// and the run that followed from the answer — reopening the first would
    /// erase the fact that a person was ever asked.
    ///
    /// Idempotent on the gate's ACTION: the admission is keyed by it, and
    /// the row insert carries the `turn_id` conflict arm, so a
    /// retried resolve continues the run exactly once.
    ///
    /// The row is announced on the fleet's tail as `turn_received` when it
    /// lands here, and only here: the run verb announces the rows it
    /// writes, and this one is already there when the runner pulls it, so a
    /// watcher would otherwise never see the continued run open.
    async fn continue_from(&self, resolved: &Resolved, turn_id: &Uuid7) -> Result<Option<Uuid7>> {
        let actor = format!("{CONTINUATION_ACTOR_PREFIX}{turn_id}");
        let kind = af_wire::turn::TurnKind::Continuation.as_str();
        let admitted = self
            .admissions
            .admit(afd_turns::IncomingTurn {
                producer: afd_turns::Producer::GateContinuation,
                // The gate's ACTION, which a retried resolve repeats: two
                // people answering one gate, or one person's retry, continue
                // the run exactly once.
                key: afd_turns::Key::Repeated(resolved.action_id.as_str()),
                fleet: &resolved.fleet_id,
                workspace: &resolved.workspace_id,
                actor: actor.as_str(),
                event_type: af_wire::turn::TurnKind::Continuation,
                request_json: CONTINUATION_BODY,
                // The run a gate parked answers where it would have: a thread
                // that asked before the gate still gets the answer after it.
                reply: afd_turns::Reply::Inherit { turn_id },
            })
            .await?;

        let (inserted, counters) = self
            .insert_continuation(resolved, &admitted.stored.id, &actor, turn_id)
            .await?;
        // Once, on the write that landed the row: a retried resolve finds it
        // already there and announces nothing, as the run verb does.
        if inserted {
            let frame = TailFrame::TurnReceived {
                turn_id: Cow::Borrowed(admitted.stored.id.as_str()),
                actor: Cow::Borrowed(&actor),
                event_type: Cow::Borrowed(kind),
                // The admission's own instant, which the ledger stamped and
                // a thread page keys this event on.
                created_at: admitted.stored.created_at,
                // A continuation carries no typed words: a person's message
                // is announced on its own admission.
                message: None,
                counters,
            };
            let subject = afd_bus::Subject::FleetTail(resolved.fleet_id.clone());
            let publication = self.bus.publish_frame(&subject, &frame).await;
            afd_bus::observe_notification(&subject, publication);
        }

        Ok(Some(admitted.stored.id))
    }

    /// Inserts the continuation row the admission keyed, answering whether
    /// THIS call landed it and, when it did, the fleet's counters after it.
    ///
    /// Counters are read after the row landed, on the same connection: the
    /// trigger's write is visible there and no second acquire is paid.
    async fn insert_continuation(
        &self,
        resolved: &Resolved,
        stored_id: &Uuid7,
        actor: &str,
        turn_id: &Uuid7,
    ) -> Result<(bool, Option<FleetCounters>)> {
        let mut connection = self.database.acquire().await?;
        let landed = sqlx::query_as::<_, Landed>(afd_turns::view::sql::INSERT_FLEET_EVENT)
            .bind(&resolved.fleet_id)
            .bind(stored_id)
            .bind(&resolved.workspace_id)
            .bind(actor)
            .bind(af_wire::turn::TurnKind::Continuation.as_str())
            .bind(sqlx::types::Json(serde_json::json!({})))
            .bind(turn_id)
            .bind(af_core::turn::status::RECEIVED)
            .fetch_one(&mut *connection)
            .await
            .map_err(error::query(CONTEXT_CONTINUE))?;
        let inserted = landed.inserted;
        let counters = if inserted {
            afd_turns::view::fleet_counters_best_effort_on(&mut connection, &resolved.fleet_id)
                .await
        } else {
            None
        };
        Ok((inserted, counters))
    }
}

/// What the continuation insert answers, by name: whether this call wrote
/// the row.
#[derive(Debug, sqlx::FromRow)]
struct Landed {
    inserted: bool,
}

mod stood;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod wake_tests;
