//! The statements behind the tool-call record verb, and the dead-fence delete
//! a post runs. Split from [`super`] at the length cap: that file
//! decides what is kept, this one writes it.

use std::collections::{BTreeMap, BTreeSet};

use af_core::id::Uuid7;
use af_wire::tool_trace::FileChange;
use jiff::Timestamp;
use serde_json::{Map, Value};
use sqlx::{Acquire as _, PgConnection};

use super::{Admissible, Kept, named_turns, serving, within_budget};
use crate::error::{Result, query};
use crate::run::sql;
use crate::run::sql::tool_detail as statement;
use crate::run::standing::LiveRun;
use crate::run::store::Runs;

/// Statement name, for the context a query failure carries.
const CONTEXT_KEEP: &str = "tool detail keep";

/// Statement name, for the context a query failure carries.
const CONTEXT_DROP: &str = "tool detail dead fence drop";

impl Runs {
    /// One post, in one transaction: lock the run, check its fence, clear
    /// dead runs' records, and write the records that name a turn the run
    /// serves and fit.
    ///
    /// The run row stays locked until the commit, so nothing can supersede
    /// the run between the fence check and the write (`SELECT_LIVE_RUN`).
    pub(super) async fn keep_records(
        &self,
        run_id: &Uuid7,
        runner_id: &Uuid7,
        presented: u64,
        candidates: BTreeMap<u64, Admissible<'_>>,
        now: Timestamp,
    ) -> Result<Kept> {
        let mut connection = self.pool().acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_KEEP))?;
        let Some(target) = locked_target(&mut transaction, run_id, runner_id).await? else {
            return Ok(Kept::NoRun);
        };
        if !target.holds(presented) {
            return Ok(Kept::Fenced(target));
        }
        drop_others(&mut transaction, &target, run_id).await?;
        let named = named_turns(&candidates);
        let served = served_turns(&mut transaction, &target, run_id, &named).await?;
        let (candidates, unserved) = serving(candidates, &served);
        let numbers: Vec<i64> = candidates
            .keys()
            .map(|&number| i64::try_from(number).unwrap_or(i64::MAX))
            .collect();
        let (spent, replaced) = spend(&mut transaction, run_id, &numbers).await?;
        let (kept, mut over) = within_budget(candidates, spent, &replaced);
        over.extend(unserved);
        let stored = kept.len();
        if stored > 0 {
            self.upsert(&mut transaction, &target, run_id, kept, now)
                .await?;
        }
        transaction.commit().await.map_err(query(CONTEXT_KEEP))?;
        Ok(Kept::Stored {
            target,
            stored,
            over,
        })
    }

    /// One statement writing every kept record of `run_id`, its columns as
    /// arrays.
    async fn upsert(
        &self,
        connection: &mut PgConnection,
        target: &LiveRun,
        run_id: &Uuid7,
        kept: Vec<Admissible<'_>>,
        now: Timestamp,
    ) -> Result<()> {
        let mut columns = Columns::default();
        for Admissible { record, bytes, .. } in kept {
            columns.ids.push(self.entropy().uuid7(now)?);
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
            columns.turns.push(Uuid7::parse(&record.turn_id)?);
            columns.kinds.push(record.kind.as_str());
            columns.tools.push(record.tool.into_owned());
            columns.statuses.push(record.status.as_str());
            columns.changes.push(changes_text(&record.changes));
        }
        sqlx::query(statement::UPSERT_RECORDS)
            .bind(&target.workspace_id)
            .bind(&target.fleet_id)
            .bind(&target.turn_id)
            .bind(target.fence)
            .bind(columns.ids)
            .bind(columns.numbers)
            .bind(columns.arguments)
            .bind(columns.truncated_arguments)
            .bind(columns.outputs)
            .bind(columns.line_counts)
            .bind(columns.truncated)
            .bind(columns.bytes)
            .bind(run_id)
            .bind(columns.turns)
            .bind(columns.kinds)
            .bind(columns.tools)
            .bind(columns.statuses)
            .bind(columns.changes)
            .execute(&mut *connection)
            .await
            .map_err(query(CONTEXT_KEEP))?;
        Ok(())
    }
}

/// The run `run_id` names, if `runner_id` holds it live, locked.
async fn locked_target(
    connection: &mut PgConnection,
    run_id: &Uuid7,
    runner_id: &Uuid7,
) -> Result<Option<LiveRun>> {
    sqlx::query_as(statement::SELECT_LIVE_RUN)
        .bind(run_id)
        .bind(runner_id)
        .bind(sql::RUN_STATUS_ACTIVE)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(CONTEXT_KEEP))
}

/// Which of `named` the run `run_id`, holding `target`, serves, as their
/// canonical text.
async fn served_turns(
    connection: &mut PgConnection,
    target: &LiveRun,
    run_id: &Uuid7,
    named: &[Uuid7],
) -> Result<BTreeSet<String>> {
    let served: Vec<Uuid7> = sqlx::query_scalar(statement::SELECT_SERVED_TURNS)
        .bind(&target.fleet_id)
        .bind(run_id)
        .bind(target.fence)
        .bind(named)
        .bind(&target.turn_id)
        .fetch_all(&mut *connection)
        .await
        .map_err(query(CONTEXT_KEEP))?;
    Ok(served.iter().map(|turn| turn.as_str().to_owned()).collect())
}

/// What the run already keeps, and what each posted call's record spends.
async fn spend(
    connection: &mut PgConnection,
    run_id: &Uuid7,
    numbers: &[i64],
) -> Result<(usize, BTreeMap<u64, usize>)> {
    let spent: i64 = sqlx::query_scalar(statement::SELECT_KEPT_BYTES)
        .bind(run_id)
        .fetch_one(&mut *connection)
        .await
        .map_err(query(CONTEXT_KEEP))?;
    let rows: Vec<(i64, i64)> = sqlx::query_as(statement::SELECT_REPLACED_BYTES)
        .bind(run_id)
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

/// Delete every record another run of `target`'s turn kept.
async fn drop_others(
    connection: &mut PgConnection,
    target: &LiveRun,
    run_id: &Uuid7,
) -> Result<()> {
    sqlx::query(statement::DELETE_OTHER_FENCES)
        .bind(&target.fleet_id)
        .bind(&target.turn_id)
        .bind(run_id)
        .execute(&mut *connection)
        .await
        .map_err(query(CONTEXT_DROP))?;
    Ok(())
}

/// An edit's changes as the `changes` column holds them, the wire's own
/// spelling; none for a record that changed nothing.
fn changes_text(changes: &[FileChange<'_>]) -> Option<String> {
    if changes.is_empty() {
        return None;
    }
    let rows = changes.iter().map(|change| {
        let mut row = Map::new();
        row.insert("path".to_owned(), Value::from(change.path.as_ref()));
        row.insert("added".to_owned(), Value::from(change.added));
        row.insert("removed".to_owned(), Value::from(change.removed));
        Value::Object(row)
    });
    Some(Value::Array(rows.collect()).to_string())
}

/// The upsert's arrays, one per column, filled in step.
#[derive(Debug, Default)]
struct Columns {
    ids: Vec<Uuid7>,
    numbers: Vec<i64>,
    arguments: Vec<String>,
    truncated_arguments: Vec<bool>,
    outputs: Vec<String>,
    line_counts: Vec<i64>,
    truncated: Vec<bool>,
    bytes: Vec<i64>,
    turns: Vec<Uuid7>,
    kinds: Vec<&'static str>,
    tools: Vec<String>,
    statuses: Vec<&'static str>,
    changes: Vec<Option<String>>,
}

#[cfg(test)]
mod changes_text_tests {
    use af_wire::tool_trace::FileChange;

    use super::changes_text;

    #[test]
    fn an_edits_changes_are_stored_as_the_wire_spells_them() {
        let changes = [
            FileChange {
                path: "src/a.rs".into(),
                added: 24,
                removed: 8,
            },
            FileChange {
                path: "docs/\"naïve\".md".into(),
                added: 0,
                removed: 1,
            },
        ];
        assert_eq!(
            changes_text(&changes),
            serde_json::to_string(&changes).ok(),
            "the column reads back as the wire's own list"
        );
    }

    #[test]
    fn a_record_that_changed_nothing_stores_no_changes() {
        assert_eq!(changes_text(&[]), None);
    }
}
