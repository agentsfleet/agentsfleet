//! The operator's side of an approval gate: read the queue, answer one, expire
//! the ones nobody answered.
//!
//! # Two sides of one table, and they are not symmetric
//!
//! [`crate::gate::Gates`] is the RUNNER's side — it parks a run behind a gate
//! and reads the durable answer back. This is the PERSON's side, and the
//! asymmetry is deliberate: a runner asks about one action it already holds,
//! where an operator browses a queue they did not raise and answers rows they
//! have to be authorised for. Different questions, different scoping, so
//! different types over one table.
//!
//! # The race is decided by Postgres, not by this crate
//!
//! Two operators answering one gate at the same instant both run one UPDATE
//! carrying `WHERE status = 'pending'`. Exactly one updates a row; the other's
//! `RETURNING` comes back empty, which is how [`Resolution`] tells "you decided
//! this" from "somebody already had". A read-then-write would let both believe
//! they won and both tell their person so.
//!
//! # A resolved gate never reopens the row it blocked
//!
//! Nothing here writes back to `core.fleet_events`. The blocked row is
//! terminal by design: a resolved gate lands a NEW event carrying
//! `actor=continuation:<original>`, so the history keeps both the run that was
//! stopped and the run that followed from the answer. Re-opening the first
//! would erase the fact that a person was ever asked.

mod row;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_db::Db;
use afd_redis::streams::OnceScope;
use afd_redis::{FleetStreams, Redis};
use afd_wire::approval::status;
use afd_wire::grant::status as grant_status;

pub use self::row::{Cursor, Filter, GateRow, Resolution, Resolved};

use self::row::{read_gate, read_resolved};
use crate::gate_status::GateStatus;

use crate::decision::Decision;
use crate::sql;
use crate::{Result, error};

/// The grant spellings the resolve's second arm writes.
///
/// Read from the shared vocabulary, never spelled here: [`crate::grant`] writes
/// the same column and the runner plane reads it, so a local copy of either
/// word is a row one writer produces that a reader stops matching.
const GRANT_APPROVED: &str = grant_status::APPROVED;
const GRANT_REVOKED: &str = grant_status::REVOKED;

/// The gate kind whose approval also moves an integration grant.
const KIND_INTEGRATION_GRANT: &str = "integration_grant";

/// The filter that means "every fleet" / "every kind".
///
/// An empty string rather than a `NULL`: the statement spells the disabled arm
/// `$3 = ''`, so one binding serves both the filtered and unfiltered read and
/// there is no second statement to keep in step.
const NO_FILTER: &str = "";

/// Who a swept gate records as its resolver.
const SWEEPER: &str = "system:approval_gate_sweeper";

/// What a swept gate records as its detail.
const SWEPT_DETAIL: &str = "the approval window closed with no answer";

const CONTEXT_PAGE: &str = "gate.inbox.page";
const CONTEXT_ONE: &str = "gate.inbox.one";
const CONTEXT_RESOLVE: &str = "gate.inbox.resolve";
const CONTEXT_EXPIRE: &str = "gate.inbox.expire";
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
/// blocked event's own body through `resumes_event_id`, and duplicating it here
/// would make two rows that could disagree about what was asked.
const CONTINUATION_BODY: &str = "{}";

/// The stream fields a continuation is appended with.
const FIELD_ACTOR: &str = afd_wire::event::field::ACTOR;
const FIELD_EVENT_TYPE: &str = afd_wire::event::field::EVENT_TYPE;
const FIELD_WORKSPACE: &str = afd_wire::event::field::WORKSPACE_ID;
const FIELD_REQUEST: &str = afd_wire::event::field::REQUEST_JSON;

/// The operator's queue over one workspace's gates.
#[derive(Debug, Clone)]
pub struct Inbox {
    database: Db,
    queue: Redis,
}

impl Inbox {
    /// A queue over `database`, continuing approved runs on `queue`.
    #[must_use]
    pub const fn new(database: Db, queue: Redis) -> Self {
        Self { database, queue }
    }

    /// One page of `workspace`'s gates, oldest first.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn page(
        &self,
        workspace: &Uuid7,
        filter: Filter<'_>,
        cursor: Option<Cursor<'_>>,
        limit: i64,
    ) -> Result<Vec<GateRow>> {
        let mut connection = self.database.acquire().await?;
        let rows = sqlx::query(sql::SELECT_GATE_PAGE)
            .bind(workspace.as_str())
            .bind(filter.status.map_or(status::PENDING, GateStatus::as_str))
            .bind(filter.fleet_id.unwrap_or(NO_FILTER))
            .bind(filter.gate_kind.unwrap_or(NO_FILTER))
            .bind(cursor.is_some())
            .bind(cursor.map_or(0, |at| at.created_at))
            .bind(cursor.map_or(NO_FILTER, |at| at.gate_id))
            .bind(limit)
            .fetch_all(&mut *connection)
            .await
            .map_err(error::query(CONTEXT_PAGE))?;

        rows.iter()
            .map(|row| read_gate(row, CONTEXT_PAGE))
            .collect()
    }

    /// One gate by id, inside `workspace`.
    ///
    /// `Ok(None)` covers both "no such gate" and "that gate is another
    /// workspace's" — the scope is an authorization, so the two must be
    /// indistinguishable to a caller probing identifiers.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn one(&self, workspace: &Uuid7, gate: &Uuid7) -> Result<Option<GateRow>> {
        let mut connection = self.database.acquire().await?;
        let row = sqlx::query(sql::SELECT_GATE_BY_ID)
            .bind(gate.as_str())
            .bind(workspace.as_str())
            .fetch_optional(&mut *connection)
            .await
            .map_err(error::query(CONTEXT_ONE))?;

        row.as_ref()
            .map(|row| read_gate(row, CONTEXT_ONE))
            .transpose()
    }

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
            let mut resolved = read_resolved(&row)?;
            // The continuation is part of RESOLVING, not something a caller
            // remembers to do afterwards: an approval that landed without one
            // is a run a person unblocked and nothing restarted.
            if outcome.continues_the_run() {
                resolved.continuation_event_id = self.continue_from(&resolved, now).await?;
            }
            return Ok(Resolution::Resolved(resolved));
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
            Some(row) => Resolution::AlreadyResolved(read_resolved(&row)?),
            None => Resolution::NotFound,
        })
    }

    /// Lands the event that resumes the run an approved gate had blocked.
    ///
    /// The blocked row is NEVER reopened. This is a new event carrying
    /// `resumes_event_id`, so the history keeps both the run that was stopped
    /// and the run that followed from the answer — reopening the first would
    /// erase the fact that a person was ever asked.
    ///
    /// Idempotent on the gate's ACTION: the stream append is `append_once`
    /// keyed by it, and the row insert carries the `(fleet_id, event_id)`
    /// conflict arm, so a retried resolve continues the run exactly once.
    async fn continue_from(&self, resolved: &Resolved, now: UnixMillis) -> Result<Option<String>> {
        let actor = format!("{CONTINUATION_ACTOR_PREFIX}{}", resolved.event_id);
        let kind = afd_wire::event::EventType::Continuation.as_str();
        let appended = FleetStreams::new(self.queue.clone())
            .append_once(
                OnceScope::FleetIntent,
                &resolved.action_id,
                &resolved.fleet_id,
                &[
                    (FIELD_ACTOR, actor.as_str()),
                    (FIELD_EVENT_TYPE, kind),
                    (FIELD_WORKSPACE, resolved.workspace_id.as_str()),
                    (FIELD_REQUEST, CONTINUATION_BODY),
                ],
            )
            .await?;

        let mut connection = self.database.acquire().await?;
        sqlx::query(afd_events::sql::INSERT_FLEET_EVENT)
            .bind(&resolved.fleet_id)
            .bind(appended.id.as_str())
            .bind(&resolved.workspace_id)
            .bind(&actor)
            .bind(kind)
            .bind(CONTINUATION_BODY)
            .bind(&resolved.event_id)
            .bind(now.as_millis())
            .bind(afd_core::event::status::RECEIVED)
            .execute(&mut *connection)
            .await
            .map_err(error::query(CONTEXT_CONTINUE))?;

        Ok(Some(appended.id.as_str().to_owned()))
    }

    /// Expires every gate whose deadline has passed, reporting how many.
    ///
    /// Scoped to PENDING rows, so an answer that landed a millisecond before
    /// the deadline is not overwritten: the operator's decision outranks the
    /// clock's.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn expire(&self, now: UnixMillis) -> Result<u64> {
        let mut connection = self.database.acquire().await?;
        let rows = sqlx::query(sql::EXPIRE_GATES)
            .bind(status::TIMED_OUT)
            .bind(status::PENDING)
            .bind(SWEEPER)
            .bind(SWEPT_DETAIL)
            .bind(now.as_millis())
            .fetch_all(&mut *connection)
            .await
            .map_err(error::query(CONTEXT_EXPIRE))?;
        Ok(rows.len() as u64)
    }
}
