//! The statements behind the tool-call record verb, and the settlement's
//! dead-fence delete. Split from [`super`] at the length cap: that file
//! decides what is kept, this one writes it.

use std::collections::BTreeMap;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use serde_json::Value;
use sqlx::{Acquire as _, PgConnection, Row as _};

use super::{Admissible, DetailTarget, Skip, within_budget};
use crate::error::{Result, query};
use crate::lease::sql;
use crate::lease::sql::tool_detail as statement;
use crate::lease::store::Leases;

/// Statement name, for the context a query failure carries.
const CONTEXT_TARGET: &str = "tool detail lease load";

/// Statement name, for the context a query failure carries.
const CONTEXT_KEEP: &str = "tool detail keep";

/// Statement name, for the context a query failure carries.
const CONTEXT_DROP: &str = "tool detail dead fence drop";

impl Leases {
    /// The lease `lease_id` names, if `runner_id` holds it live.
    pub(super) async fn detail_target(
        &self,
        lease_id: &str,
        runner_id: &Uuid7,
        now: UnixMillis,
    ) -> Result<Option<DetailTarget>> {
        let mut connection = self.pool().acquire().await?;
        let found = sqlx::query(statement::SELECT_LIVE_LEASE)
            .bind(lease_id)
            .bind(runner_id.as_str())
            .bind(sql::LEASE_STATUS_ACTIVE)
            .bind(now.as_millis())
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_TARGET))?;
        let Some(row) = found else {
            return Ok(None);
        };
        Ok(Some(DetailTarget {
            fleet_id: row.try_get(0).map_err(query(CONTEXT_TARGET))?,
            workspace_id: row.try_get(1).map_err(query(CONTEXT_TARGET))?,
            event_id: row.try_get(2).map_err(query(CONTEXT_TARGET))?,
            fence: row.try_get(3).map_err(query(CONTEXT_TARGET))?,
            live_seq: row.try_get(4).map_err(query(CONTEXT_TARGET))?,
        }))
    }

    /// Writes the records that fit the event's budget, in one transaction.
    ///
    /// Answers how many were written and which did not fit.
    pub(super) async fn keep_records(
        &self,
        target: &DetailTarget,
        candidates: BTreeMap<u64, Admissible<'_>>,
        now: UnixMillis,
    ) -> Result<(usize, Vec<Skip>)> {
        let numbers: Vec<i64> = candidates
            .keys()
            .map(|&number| i64::try_from(number).unwrap_or(i64::MAX))
            .collect();
        let mut connection = self.pool().acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_KEEP))?;
        sqlx::query(statement::LOCK_EVENT)
            .bind(&target.fleet_id)
            .bind(&target.event_id)
            .execute(&mut *transaction)
            .await
            .map_err(query(CONTEXT_KEEP))?;
        let spent: i64 = sqlx::query_scalar(statement::SELECT_KEPT_BYTES)
            .bind(&target.fleet_id)
            .bind(&target.event_id)
            .bind(target.fence)
            .bind(&numbers)
            .fetch_one(&mut *transaction)
            .await
            .map_err(query(CONTEXT_KEEP))?;
        let (kept, over) = within_budget(candidates, usize::try_from(spent).unwrap_or(usize::MAX));
        let stored = kept.len();
        if stored > 0 {
            self.upsert(&mut transaction, target, kept, now).await?;
        }
        transaction.commit().await.map_err(query(CONTEXT_KEEP))?;
        Ok((stored, over))
    }

    /// One statement writing every kept record, its columns as arrays.
    async fn upsert(
        &self,
        connection: &mut PgConnection,
        target: &DetailTarget,
        kept: Vec<Admissible<'_>>,
        now: UnixMillis,
    ) -> Result<()> {
        let mut columns = Columns::default();
        for Admissible { record, bytes, .. } in kept {
            columns
                .ids
                .push(self.entropy().uuid7(now)?.as_str().to_owned());
            columns
                .numbers
                .push(i64::try_from(record.call_number).unwrap_or(i64::MAX));
            columns
                .arguments
                .push(Value::Object(record.arguments).to_string());
            columns.truncated_arguments.push(record.truncated_arguments);
            columns.outputs.push(record.output.into_owned());
            columns
                .line_counts
                .push(i64::try_from(record.output_line_count).unwrap_or(i64::MAX));
            columns.truncated.push(record.truncated);
            columns.bytes.push(i64::try_from(bytes).unwrap_or(i64::MAX));
        }
        sqlx::query(statement::UPSERT_RECORDS)
            .bind(&target.workspace_id)
            .bind(&target.fleet_id)
            .bind(&target.event_id)
            .bind(target.fence)
            .bind(now.as_millis())
            .bind(columns.ids)
            .bind(columns.numbers)
            .bind(columns.arguments)
            .bind(columns.truncated_arguments)
            .bind(columns.outputs)
            .bind(columns.line_counts)
            .bind(columns.truncated)
            .bind(columns.bytes)
            .execute(&mut *connection)
            .await
            .map_err(query(CONTEXT_KEEP))?;
        Ok(())
    }

    /// Delete every other lease's records of the event that settled.
    ///
    /// Runs on the settling transaction's connection, beside the answer.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub(crate) async fn drop_other_fences(
        &self,
        connection: &mut PgConnection,
        fleet_id: &Uuid7,
        event_id: &str,
        fence: i64,
    ) -> Result<()> {
        sqlx::query(statement::DELETE_OTHER_FENCES)
            .bind(fleet_id.as_str())
            .bind(event_id)
            .bind(fence)
            .execute(&mut *connection)
            .await
            .map_err(query(CONTEXT_DROP))?;
        Ok(())
    }
}

/// The upsert's arrays, one per column, filled in step.
#[derive(Debug, Default)]
struct Columns {
    ids: Vec<String>,
    numbers: Vec<i64>,
    arguments: Vec<String>,
    truncated_arguments: Vec<bool>,
    outputs: Vec<String>,
    line_counts: Vec<i64>,
    truncated: Vec<bool>,
    bytes: Vec<i64>,
}
