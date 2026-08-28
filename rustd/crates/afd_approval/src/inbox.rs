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

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_db::Db;
use afd_redis::{FleetStreams, Redis};
use afd_wire::approval::status;
use sqlx::Row as _;

use crate::decision::Decision;
use crate::sql;
use crate::{Result, error};

/// The status a continuation event opens in.
///
/// `received`, the same word the runner plane's report path flips out of.
const EVENT_STATUS_RECEIVED: &str = "received";

/// The grant spellings the resolve's second arm writes.
const GRANT_APPROVED: &str = "approved";
const GRANT_REVOKED: &str = "revoked";

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
const FIELD_ACTOR: &str = "actor";
const FIELD_EVENT_TYPE: &str = "event_type";
const FIELD_WORKSPACE: &str = "workspace_id";
const FIELD_REQUEST: &str = "request_json";

/// One gate as the inbox shows it.
#[derive(Debug, Clone)]
pub struct GateRow {
    /// The gate's own row id — what a decision addresses it by.
    pub gate_id: String,
    /// The fleet that raised it.
    pub fleet_id: String,
    /// That fleet's name, joined rather than stored — see the statement.
    pub fleet_name: String,
    /// The workspace the gate belongs to.
    pub workspace_id: String,
    /// The action the gate is about.
    pub action_id: String,
    /// The tool asking.
    pub tool_name: String,
    /// The verb it wants.
    pub action_name: String,
    /// Which family of gate this is.
    pub gate_kind: String,
    /// What the fleet proposes to do, in a person's words.
    pub proposed_action: String,
    /// The evidence behind the proposal, as stored JSON text.
    pub evidence_json: String,
    /// How far the consequences reach.
    pub blast_radius: String,
    /// Where the gate stands.
    pub status: String,
    /// The resolver's note, empty while pending.
    pub detail: String,
    /// When the gate was raised.
    pub created_at: i64,
    /// When it stops waiting.
    pub timeout_at: i64,
    /// When it was resolved, or `None` while pending.
    pub updated_at: Option<i64>,
    /// Who resolved it, empty while pending.
    pub resolved_by: String,
}

/// Where a page resumes.
///
/// The instant AND the id, because gates raised in the same millisecond are
/// ordinary — one run parks several tools at once — and an instant alone would
/// skip every sibling of the cursor row.
#[derive(Debug, Clone, Copy)]
pub struct Cursor<'a> {
    /// The last row's instant.
    pub created_at: i64,
    /// The last row's id, breaking the tie.
    pub gate_id: &'a str,
}

/// What a queue read is narrowed by.
#[derive(Debug, Clone, Copy, Default)]
pub struct Filter<'a> {
    /// Only gates at this status; pending when absent.
    pub status: Option<Decision>,
    /// Only gates raised by this fleet.
    pub fleet_id: Option<&'a str>,
    /// Only gates of this kind.
    pub gate_kind: Option<&'a str>,
}

/// What answering a gate did.
///
/// Three outcomes and not two: "somebody already answered" is not a failure —
/// the gate IS resolved, which is what the operator wanted — but it is also not
/// this person's decision, and an inbox that reported it as theirs would
/// attribute an answer to the wrong human.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// This caller won the race; the row carries their attribution.
    Resolved(Resolved),
    /// An earlier writer had already terminated the row.
    AlreadyResolved(Resolved),
    /// No gate for that action, in that scope.
    NotFound,
}

/// The canonical attribution a resolve reports either way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// The gate's row id.
    pub gate_id: String,
    /// The action it gated.
    pub action_id: String,
    /// The workspace it belonged to.
    pub workspace_id: String,
    /// The fleet that raised it.
    pub fleet_id: String,
    /// Where it now stands.
    pub status: String,
    /// When it was resolved.
    pub updated_at: i64,
    /// Who resolved it.
    pub resolved_by: String,
    /// Their note.
    pub detail: String,
    /// The event the gate blocked, which a continuation resumes from.
    pub event_id: String,
    /// The continuation this decision landed, when it landed one.
    ///
    /// `Some` only for an approval THIS caller won: a denial continues
    /// nothing, and a caller who lost the race did not write the row the
    /// winner already did.
    pub continuation_event_id: Option<String>,
}

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
            .bind(filter.status.map_or(status::PENDING, Decision::as_str))
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
        sqlx::query(sql::INSERT_FLEET_EVENT)
            .bind(&resolved.fleet_id)
            .bind(appended.id.as_str())
            .bind(&resolved.workspace_id)
            .bind(&actor)
            .bind(kind)
            .bind(CONTINUATION_BODY)
            .bind(&resolved.event_id)
            .bind(now.as_millis())
            .bind(EVENT_STATUS_RECEIVED)
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

/// Reads one inbox row, through one error context.
fn read_gate(row: &sqlx::postgres::PgRow, context: &'static str) -> Result<GateRow> {
    let unreadable = error::query(context);
    Ok(GateRow {
        gate_id: row.try_get(0).map_err(&unreadable)?,
        fleet_id: row.try_get(1).map_err(&unreadable)?,
        fleet_name: row.try_get(2).map_err(&unreadable)?,
        workspace_id: row.try_get(3).map_err(&unreadable)?,
        action_id: row.try_get(4).map_err(&unreadable)?,
        tool_name: row.try_get(5).map_err(&unreadable)?,
        action_name: row.try_get(6).map_err(&unreadable)?,
        gate_kind: row.try_get(7).map_err(&unreadable)?,
        proposed_action: row.try_get(8).map_err(&unreadable)?,
        evidence_json: row.try_get(9).map_err(&unreadable)?,
        blast_radius: row.try_get(10).map_err(&unreadable)?,
        status: row.try_get(11).map_err(&unreadable)?,
        detail: row.try_get(12).map_err(&unreadable)?,
        created_at: row.try_get(13).map_err(&unreadable)?,
        timeout_at: row.try_get(14).map_err(&unreadable)?,
        updated_at: row.try_get(15).map_err(&unreadable)?,
        resolved_by: row.try_get(16).map_err(&unreadable)?,
    })
}

/// Reads the attribution a resolve reports.
fn read_resolved(row: &sqlx::postgres::PgRow) -> Result<Resolved> {
    let unreadable = error::query(CONTEXT_RESOLVE);
    Ok(Resolved {
        gate_id: row.try_get(0).map_err(&unreadable)?,
        action_id: row.try_get(1).map_err(&unreadable)?,
        workspace_id: row.try_get(2).map_err(&unreadable)?,
        fleet_id: row.try_get(3).map_err(&unreadable)?,
        status: row.try_get(4).map_err(&unreadable)?,
        updated_at: row.try_get(5).map_err(&unreadable)?,
        resolved_by: row.try_get(6).map_err(&unreadable)?,
        detail: row.try_get(7).map_err(&unreadable)?,
        event_id: row.try_get(8).map_err(&unreadable)?,
        continuation_event_id: None,
    })
}
