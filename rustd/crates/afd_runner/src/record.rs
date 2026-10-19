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
use jiff::Timestamp;

use crate::error::{Result, query, row_malformed, runner_vanished};
use crate::policy::{AssignmentColumns, StoredVerdict};
use crate::sql;
use crate::store::Runners;

/// The statement name a query failure carries.
const CONTEXT_SELF_READ: &str = "runner self read";

/// The table every column here comes from, for a malformed-value report.
const TABLE_RUNNERS: &str = "runner.runners";

/// One `runner.runners` row, as columns.
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
    /// Operator-facing status.
    pub status: String,
    /// The host it runs on.
    pub host_id: String,
    /// The last beat; [`sql::NEVER_SEEN`] when it has never connected.
    pub last_seen_at: Timestamp,
    /// What the operator assigned, as stored.
    pub assignment: AssignmentColumns,
    /// What the host last reported it can enforce, as stored JSON.
    pub capability_report_json: Option<String>,
    /// The verdict the row currently holds.
    pub verdict: StoredVerdict,
}

/// [`sql::runner::SELECT_RUNNER_SELF`]'s row, by column name.
#[derive(Debug, sqlx::FromRow)]
struct SelfColumns {
    id: String,
    status: String,
    host_id: String,
    sandbox_tier: String,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    last_seen_at: Timestamp,
    network_policy: String,
    registry_allowlist: String,
    worker_count: i32,
    capability_report: Option<String>,
    degraded_reason: Option<String>,
    sandbox_config: Option<String>,
}

impl SelfColumns {
    /// The row, its identifier proven canonical.
    fn parse(self) -> Result<SelfRow> {
        Ok(SelfRow {
            id: Uuid7::parse(&self.id).map_err(row_malformed(TABLE_RUNNERS, "id"))?,
            status: self.status,
            host_id: self.host_id,
            last_seen_at: self.last_seen_at,
            assignment: AssignmentColumns {
                sandbox_tier: self.sandbox_tier,
                network_policy: self.network_policy,
                registry_allowlist_json: self.registry_allowlist,
                worker_count: self.worker_count,
                sandbox_config_json: self.sandbox_config,
            },
            capability_report_json: self.capability_report,
            verdict: StoredVerdict {
                reason: self.degraded_reason,
            },
        })
    }
}

impl Runners {
    /// Reads the runner's own row.
    ///
    /// Deliberately does NOT bump `last_seen_at`: liveness is written by the
    /// heartbeat alone, so reading a runner's row through `GET /v1/runners/me`
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
        let found = sqlx::query_as::<_, SelfColumns>(sql::runner::SELECT_RUNNER_SELF)
            .bind(runner.as_str())
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_SELF_READ))?;
        // Fail closed rather than answer 200 for a phantom runner.
        found.ok_or_else(runner_vanished)?.parse()
    }
}
