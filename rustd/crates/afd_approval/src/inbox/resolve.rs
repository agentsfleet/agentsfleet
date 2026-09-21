//! Answering a gate, and everything one answer sets off.
//!
//! Split from [`super`] on the seam between READING the queue and MOVING a
//! row. The read half is two statements and a decoder; this half is one
//! guarded UPDATE plus the three things that follow from it — the tail is
//! told, a runless gate's parked delivery is woken, and an approval that held
//! a run lands the continuation. They belong together because they share one
//! invariant: the row moved first, so none of them may turn a durable
//! decision back into a retry.

use std::borrow::Cow;

use afd_core::clock::UnixMillis;
use afd_dragonfly::{FleetStreams, ReadyIndex};
use afd_wire::approval::status;
use afd_wire::grant::status as grant_status;
use afd_wire::tail::TailFrame;
use sqlx::Row as _;

use super::announce::Answer;
use super::row::read_resolved;
use super::{Inbox, NO_FILTER, Resolution, Resolved};
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

/// The ready mark that wakes a parked runless gate would not write.
const EVENT_RUNLESS_READY_MARK_FAILED: &str = "gate_runless_ready_mark_failed";

/// The column the pending count sits in, after the resolved row's nine.
const COLUMN_PENDING_APPROVALS: usize = 9;

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
/// blocked event's own body through `resumes_event_id`, and duplicating it here
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
        now: UnixMillis,
    ) -> Result<Resolution> {
        let scope = fleet.unwrap_or(NO_FILTER);
        let mut connection = self.database.acquire().await?;

        let won = sqlx::query(sql::RESOLVE_GATE)
            .bind(outcome.as_str())
            .bind(detail)
            .bind(by)
            .bind(now.as_millis())
            .bind(action)
            .bind(status::PENDING)
            .bind(scope)
            .bind(status::APPROVED)
            .bind(GRANT_APPROVED)
            .bind(GRANT_REVOKED)
            .bind(KIND_INTEGRATION_GRANT)
            .fetch_optional(&mut *connection)
            .await
            .map_err(error::query(CONTEXT_RESOLVE))?;

        if let Some(row) = won {
            return Ok(Resolution::Resolved(
                self.won(&mut connection, &row, outcome, now).await?,
            ));
        }

        // Nothing updated: either somebody answered first, or there was never
        // a gate. The second read is what tells those apart, and it runs only
        // on the losing path so the winner pays one statement.
        let existing = sqlx::query(sql::SELECT_GATE_BY_ACTION)
            .bind(action)
            .bind(scope)
            .fetch_optional(&mut *connection)
            .await
            .map_err(error::query(CONTEXT_RESOLVE))?;

        Ok(match existing {
            Some(row) => {
                let resolved = read_resolved(&row)?;
                if resolved.event_id.is_none() {
                    self.wake_runless_resolution(&resolved).await;
                }
                Resolution::AlreadyResolved(resolved)
            }
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
    async fn won(
        &self,
        connection: &mut sqlx::PgConnection,
        row: &sqlx::postgres::PgRow,
        outcome: Decision,
        now: UnixMillis,
    ) -> Result<Resolved> {
        let mut resolved = read_resolved(row)?;
        let pending_approvals: i64 = row
            .try_get(COLUMN_PENDING_APPROVALS)
            .map_err(error::query(CONTEXT_RESOLVE))?;
        // The continuation is part of RESOLVING, not something a caller
        // remembers to do afterwards: an approval that landed without one is
        // a run a person unblocked and nothing restarted.
        let continuation = match (outcome.continues_the_run(), resolved.event_id.as_deref()) {
            (true, Some(event_id)) => self.continue_from(&resolved, event_id, now).await,
            (true | false, None) => {
                self.wake_runless_resolution(&resolved).await;
                Ok(None)
            }
            (false, Some(_event_id)) => Ok(None),
        };
        // After the continuation, so an approval's frame carries the count the
        // continued run's own row moved — on the connection the resolve still
        // holds, so the announcement costs no acquire of its own.
        let counters =
            afd_events::fleet_counters_best_effort_on(connection, &resolved.fleet_id).await;
        self.announce(Answer {
            fleet_id: &resolved.fleet_id,
            gate_id: &resolved.gate_id,
            event_id: resolved.event_id.as_deref(),
            status: &resolved.status,
            resolved_by: &resolved.resolved_by,
            pending_approvals,
            counters,
        })
        .await;
        resolved.continuation_event_id = continuation?;
        Ok(resolved)
    }

    /// Wakes the fleet after a runless gate changes state.
    ///
    /// Install-time integration grants deliberately carry no `event_id`: the
    /// original delivery stays on the fleet stream, and the answer changes what
    /// that same delivery will read on its next poll. Resolving the card must
    /// therefore wake the ready index without appending a continuation event.
    ///
    /// Best-effort for the same reason regular chat ingress is: the database
    /// answer is already durable, and a Dragonfly mark failure should not turn a
    /// completed human decision into a retry that can no longer win the row.
    async fn wake_runless_resolution(&self, resolved: &Resolved) {
        let fleet = resolved.fleet_id.as_str();
        if let Err(error) = ReadyIndex::new(self.queue.clone()).mark(fleet, fleet).await {
            let code = afd_core::error_code::INTERNAL_OPERATION_FAILED.as_str();
            let gate = resolved.gate_id.as_str();
            let reason = error.to_string();
            tracing::warn!(
                error_code = code,
                event = EVENT_RUNLESS_READY_MARK_FAILED,
                fleet_id = fleet,
                gate_id = gate,
                reason,
                "the gate resolved but the fleet readiness mark could not be refreshed"
            );
        }
    }

    /// Lands the event that resumes the run an approved gate had blocked.
    ///
    /// The blocked row is NEVER reopened. This is a new event carrying
    /// `resumes_event_id`, so the history keeps both the run that was stopped
    /// and the run that followed from the answer — reopening the first would
    /// erase the fact that a person was ever asked.
    ///
    /// Idempotent on the gate's ACTION: the admission is keyed by it, and
    /// the row insert carries the `(fleet_id, event_id)` conflict arm, so a
    /// retried resolve continues the run exactly once.
    ///
    /// The row is announced on the fleet's tail as `event_received` when it
    /// lands here, and only here: the lease verb announces the rows it
    /// writes, and this one is already there when the runner pulls it, so a
    /// watcher would otherwise never see the continued run open.
    async fn continue_from(
        &self,
        resolved: &Resolved,
        event_id: &str,
        now: UnixMillis,
    ) -> Result<Option<String>> {
        let actor = format!("{CONTINUATION_ACTOR_PREFIX}{event_id}");
        let kind = afd_wire::event::EventType::Continuation.as_str();
        let admitted = self
            .admissions
            .admit(afd_admission::Admission {
                producer: afd_admission::Producer::GateContinuation,
                // The gate's ACTION, which a retried resolve repeats: two
                // people answering one gate, or one person's retry, continue
                // the run exactly once.
                key: afd_admission::Key::Repeated(&resolved.action_id),
                fleet: &resolved.fleet_id,
                workspace: &resolved.workspace_id,
                actor: actor.as_str(),
                event_type: afd_wire::event::EventType::Continuation,
                request_json: CONTINUATION_BODY,
            })
            .await?;

        let (inserted, counters) = {
            let mut connection = self.database.acquire().await?;
            let inserted: bool = sqlx::query(afd_events::sql::INSERT_FLEET_EVENT)
                .bind(&resolved.fleet_id)
                .bind(admitted.id.as_str())
                .bind(&resolved.workspace_id)
                .bind(&actor)
                .bind(kind)
                .bind(CONTINUATION_BODY)
                .bind(event_id)
                .bind(now.as_millis())
                .bind(afd_core::event::status::RECEIVED)
                .fetch_one(&mut *connection)
                .await
                .map_err(error::query(CONTEXT_CONTINUE))?
                .try_get(0)
                .map_err(error::query(CONTEXT_CONTINUE))?;
            // Counters are read after the row landed, on the same connection:
            // the trigger's write is visible there and no second acquire is paid.
            let counters = if inserted {
                afd_events::fleet_counters_best_effort_on(&mut connection, &resolved.fleet_id).await
            } else {
                None
            };
            (inserted, counters)
        };
        // Once, on the write that landed the row: a retried resolve finds it
        // already there and announces nothing, as the lease verb does.
        if inserted {
            let frame = TailFrame::EventReceived {
                event_id: Cow::Borrowed(admitted.id.as_str()),
                actor: Cow::Borrowed(&actor),
                event_type: Cow::Borrowed(kind),
                created_at: now.as_millis(),
                counters,
            };
            FleetStreams::new(self.queue.clone())
                .publish_frame(&resolved.fleet_id, &frame)
                .await;
        }

        Ok(Some(admitted.id))
    }
}
