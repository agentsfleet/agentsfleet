//! The statements behind the tool-call record verb, and the settlement's
//! dead-fence delete. Split from [`super`] at the length cap: that file
//! decides what is kept, this one writes it.

use std::collections::BTreeMap;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use serde_json::Value;
use sqlx::{Acquire as _, PgConnection};

use super::{Admissible, Kept, within_budget};
use crate::error::{Result, query};
use crate::lease::settle::Reported;
use crate::lease::sql;
use crate::lease::sql::tool_detail as statement;
use crate::lease::standing::LiveLease;
use crate::lease::store::Leases;

/// Statement name, for the context a query failure carries.
const CONTEXT_KEEP: &str = "tool detail keep";

/// Statement name, for the context a query failure carries.
const CONTEXT_DROP: &str = "tool detail dead fence drop";

impl Leases {
    /// One post, in one transaction: lock the lease, check its fence, clear
    /// dead fences' records, and write the records that fit.
    ///
    /// The lease row stays locked until the commit, so nothing can supersede
    /// the lease between the fence check and the write (`SELECT_LIVE_LEASE`).
    pub(super) async fn keep_records(
        &self,
        lease_id: &str,
        runner_id: &Uuid7,
        presented: u64,
        candidates: BTreeMap<u64, Admissible<'_>>,
        now: UnixMillis,
    ) -> Result<Kept> {
        let mut connection = self.pool().acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_KEEP))?;
        let Some(target) = locked_target(&mut transaction, lease_id, runner_id, now).await? else {
            return Ok(Kept::NoLease);
        };
        if !target.holds(presented) {
            return Ok(Kept::Fenced(target));
        }
        drop_others(
            &mut transaction,
            &target.fleet_id,
            &target.event_id,
            target.fence,
        )
        .await?;
        let numbers: Vec<i64> = candidates
            .keys()
            .map(|&number| i64::try_from(number).unwrap_or(i64::MAX))
            .collect();
        let (spent, replaced) = spend(&mut transaction, &target, &numbers).await?;
        let (kept, over) = within_budget(candidates, spent, &replaced);
        let stored = kept.len();
        if stored > 0 {
            self.upsert(&mut transaction, &target, kept, now).await?;
        }
        transaction.commit().await.map_err(query(CONTEXT_KEEP))?;
        Ok(Kept::Stored {
            target,
            stored,
            over,
        })
    }

    /// One statement writing every kept record, its columns as arrays.
    async fn upsert(
        &self,
        connection: &mut PgConnection,
        target: &LiveLease,
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

    /// Delete every other lease's records of the event `lease` settles.
    ///
    /// Runs on the settling transaction's connection, beside the answer.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub(crate) async fn drop_other_fences(
        &self,
        connection: &mut PgConnection,
        lease: &Reported,
    ) -> Result<()> {
        drop_others(
            connection,
            lease.fleet_id.as_str(),
            &lease.event_id,
            lease.fence.as_i64(),
        )
        .await
    }
}

/// The lease `lease_id` names, if `runner_id` holds it live, locked.
async fn locked_target(
    connection: &mut PgConnection,
    lease_id: &str,
    runner_id: &Uuid7,
    now: UnixMillis,
) -> Result<Option<LiveLease>> {
    let found = sqlx::query(statement::SELECT_LIVE_LEASE)
        .bind(lease_id)
        .bind(runner_id.as_str())
        .bind(sql::LEASE_STATUS_ACTIVE)
        .bind(now.as_millis())
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(CONTEXT_KEEP))?;
    let Some(row) = found else {
        return Ok(None);
    };
    LiveLease::read(&row, CONTEXT_KEEP).map(Some)
}

/// What the lease already keeps, and what each posted call's record spends.
async fn spend(
    connection: &mut PgConnection,
    target: &LiveLease,
    numbers: &[i64],
) -> Result<(usize, BTreeMap<u64, usize>)> {
    let spent: i64 = sqlx::query_scalar(statement::SELECT_KEPT_BYTES)
        .bind(&target.fleet_id)
        .bind(&target.event_id)
        .bind(target.fence)
        .fetch_one(&mut *connection)
        .await
        .map_err(query(CONTEXT_KEEP))?;
    let rows: Vec<(i64, i64)> = sqlx::query_as(statement::SELECT_REPLACED_BYTES)
        .bind(&target.fleet_id)
        .bind(&target.event_id)
        .bind(target.fence)
        .bind(numbers)
        .fetch_all(&mut *connection)
        .await
        .map_err(query(CONTEXT_KEEP))?;
    let replaced = rows
        .into_iter()
        .map(|(number, bytes)| (to_usize(number), to_usize(bytes)))
        .map(|(number, bytes)| (u64::try_from(number).unwrap_or_default(), bytes))
        .collect();
    Ok((to_usize(spent), replaced))
}

/// A stored count as a size; a negative one, which nothing writes, is zero.
fn to_usize(stored: i64) -> usize {
    usize::try_from(stored).unwrap_or_default()
}

/// Delete every record of the event outside `fence`.
async fn drop_others(
    connection: &mut PgConnection,
    fleet_id: &str,
    event_id: &str,
    fence: i64,
) -> Result<()> {
    sqlx::query(statement::DELETE_OTHER_FENCES)
        .bind(fleet_id)
        .bind(event_id)
        .bind(fence)
        .execute(&mut *connection)
        .await
        .map_err(query(CONTEXT_DROP))?;
    Ok(())
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
