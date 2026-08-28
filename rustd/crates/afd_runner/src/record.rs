//! The runner's own row, read back.
//!
//! # Why this hands back COLUMNS and not a wire payload
//!
//! `afd_wire`'s payloads borrow (`Cow<'a, str>` behind `#[serde(borrow)]`),
//! which is what keeps the lease path from allocating per field. A store that
//! returned one would have to own every byte it borrows — either by leaking, or
//! by growing an owned mirror of each wire type, which is sixty lines of near
//! duplicate whose only job is to be converted away one line later.
//!
//! So the store answers the row, the caller assembles the payload borrowing
//! from it, and neither owns a type the other already has. That split is
//! `core_api`'s `models/` and `into.rs`, and it is the half of that codebase
//! worth taking.

use afd_core::id::Uuid7;
use sqlx::Row as _;

use crate::error::{Error, Result, query, row_malformed};
use crate::policy::{AssignmentColumns, StoredVerdict};
use crate::sql;
use crate::store::Runners;

/// The statement name a query failure carries.
const CONTEXT_SELF_READ: &str = "runner self read";

/// The table every column here comes from, for a malformed-value report.
const TABLE_RUNNERS: &str = "fleet.runners";

/// One `fleet.runners` row, as columns.
///
/// Owned, because the pooled connection is released before the caller reads it;
/// the JSON columns stay TEXT so the caller can deserialise a borrowing wire
/// type straight out of them.
///
/// The assignment and the verdict are grouped rather than flat, so the self
/// read and the heartbeat hand their callers the SAME two types — and so the
/// decoder that turns either into a wire policy has one shape to accept
/// (`crate::policy`).
#[derive(Debug)]
pub struct SelfRow {
    /// The runner's identifier, already proven canonical.
    pub id: Uuid7,
    /// Operator-facing administrative state.
    pub status: String,
    /// The host it runs on.
    pub host_id: String,
    /// Epoch milliseconds of the last beat; [`sql::LAST_SEEN_NEVER`] when it has
    /// never connected.
    pub last_seen_at: i64,
    /// What the operator assigned, as stored.
    pub assignment: AssignmentColumns,
    /// What the host last reported it can enforce, as stored JSON.
    pub capability_report_json: Option<String>,
    /// The verdict the row currently holds.
    pub verdict: StoredVerdict,
}

impl Runners {
    /// Reads the runner's own row.
    ///
    /// Deliberately does NOT bump `last_seen_at`: liveness is written by the
    /// heartbeat alone, so inspecting a host with `agentsfleet-runner status`
    /// can never mask a dead runner (`docs/AUTH.md` §Runner token).
    ///
    /// # Errors
    /// Reports a datastore that would not answer, a statement Postgres refused,
    /// and — as its own kind — a token that authenticated against a row which
    /// has since been reaped. That last one is NOT collapsed into a 401 for a
    /// bad token: the credential is real and the enrolment is gone, so the
    /// remedy is to re-enrol the host rather than to retry.
    pub async fn self_record(&self, runner: &Uuid7) -> Result<SelfRow> {
        let mut connection = self.pool().acquire().await?;
        let found = sqlx::query(sql::runner::SELECT_RUNNER_SELF)
            .bind(runner.as_str())
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_SELF_READ))?;
        // Fail closed rather than answer 200 for a phantom runner.
        let row = found.ok_or_else(|| Error::RunnerVanished)?;

        let column = query(CONTEXT_SELF_READ);
        let id: String = row.try_get(0).map_err(&column)?;
        Ok(SelfRow {
            id: Uuid7::parse(&id).map_err(row_malformed(TABLE_RUNNERS, "id"))?,
            status: row.try_get(1).map_err(&column)?,
            host_id: row.try_get(2).map_err(&column)?,
            last_seen_at: row.try_get(4).map_err(&column)?,
            assignment: AssignmentColumns {
                sandbox_tier: row.try_get(3).map_err(&column)?,
                network_policy: row.try_get(5).map_err(&column)?,
                registry_allowlist_json: row.try_get(6).map_err(&column)?,
                worker_count: row.try_get(7).map_err(&column)?,
                extra_binds_json: row.try_get(11).map_err(&column)?,
            },
            capability_report_json: row.try_get(8).map_err(&column)?,
            verdict: StoredVerdict {
                degraded: row.try_get(9).map_err(&column)?,
                reason: row.try_get(10).map_err(&column)?,
            },
        })
    }
}
