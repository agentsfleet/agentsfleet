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

mod announce;
mod resolve;
mod row;
mod sweep;

use afd_admission::Admissions;
use afd_core::id::Uuid7;
use afd_db::Db;
use afd_dragonfly::Dragonfly;

pub use self::row::{Cursor, Filter, GateRow, Resolution, Resolved};

use self::row::read_gate;
use crate::gate_status::GateStatus;
use crate::sql;
use crate::{Result, error};

/// The filter that means "every fleet" / "every kind".
///
/// An empty string rather than a `NULL`: the statement spells the disabled arm
/// `$3 = ''`, so one binding serves both the filtered and unfiltered read and
/// there is no second statement to keep in step.
pub(super) const NO_FILTER: &str = "";

const CONTEXT_PAGE: &str = "gate.inbox.page";
const CONTEXT_ONE: &str = "gate.inbox.one";

/// The operator's queue over one workspace's gates.
#[derive(Debug, Clone)]
pub struct Inbox {
    database: Db,
    queue: Dragonfly,
    /// Where an approved gate's continuation is accepted, before anything is
    /// queued. Distinct from [`Self::queue`], which still publishes the
    /// answer frame a watcher sees.
    admissions: Admissions,
}

impl Inbox {
    /// A queue over `database`, continuing approved runs through
    /// `admissions` and announcing them on `queue`.
    #[must_use]
    pub const fn new(database: Db, queue: Dragonfly, admissions: Admissions) -> Self {
        Self {
            database,
            queue,
            admissions,
        }
    }

    /// One page of `workspace`'s gates, newest first.
    ///
    /// An absent `filter.status` reads every state rather than defaulting to
    /// pending, which is why the order flipped: see [`sql::SELECT_GATE_PAGE`].
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
            .bind(filter.status.map_or(NO_FILTER, GateStatus::as_str))
            .bind(filter.fleet_id.unwrap_or(NO_FILTER))
            .bind(filter.gate_kind.unwrap_or(NO_FILTER))
            .bind(cursor.is_some())
            .bind(cursor.map_or(0, |at| at.created_at))
            // NULL, not `''`, when there is no cursor: the statement casts this
            // to uuid so the keyset seek can ride the index, and `''::uuid` is
            // not a uuid. The `$5 = false` arm is what actually excludes it.
            .bind(cursor.map(|at| at.gate_id))
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
}
