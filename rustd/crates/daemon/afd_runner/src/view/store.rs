use afd_core::id::Uuid7;
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;

use super::decode::{DetailRow, EventRow, ItemRow};
use super::{
    KeysetCursor, RunnerDetail, RunnerEventFilter, RunnerEventPage, RunnerItem, RunnerPage, events,
};
use crate::error::{Result, query, runner_not_found};
use crate::sql;
use crate::store::Runners;

const CONTEXT_RUNNER_COUNT: &str = "runner list count";
const CONTEXT_RUNNER_LIST: &str = "runner list page";
const CONTEXT_RUNNER_DETAIL: &str = "runner detail";
const CONTEXT_RUNNER_EXISTS: &str = "runner event owner read";
const CONTEXT_EVENT_COUNT: &str = "runner event count";
const CONTEXT_EVENT_LIST: &str = "runner event page";

impl Runners {
    /// Lists runners newest first, deriving liveness at the database's clock.
    ///
    /// # Errors
    /// Reports datastore failures or malformed stored rows.
    pub async fn list_runners(
        &self,
        cursor: Option<&KeysetCursor>,
        limit: u32,
    ) -> Result<RunnerPage> {
        let mut connection = self.pool().acquire().await?;
        let total = sqlx::query_scalar(sql::runner_view::COUNT_RUNNERS)
            .fetch_one(&mut *connection)
            .await
            .map_err(query(CONTEXT_RUNNER_COUNT))?;
        let statement = cursor.map_or(
            sqlx::query_as::<_, ItemRow>(sql::runner_view::LIST_RUNNERS_FIRST)
                .bind(sql::LEASE_STATUS_ACTIVE)
                .bind(i64::from(limit)),
            |boundary| {
                sqlx::query_as::<_, ItemRow>(sql::runner_view::LIST_RUNNERS_AFTER)
                    .bind(sql::LEASE_STATUS_ACTIVE)
                    .bind(boundary.created_at().to_sqlx())
                    .bind(boundary.id().as_str())
                    .bind(i64::from(limit))
            },
        );
        let rows = statement
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT_RUNNER_LIST))?;
        let items = rows
            .into_iter()
            .map(ItemRow::parse)
            .collect::<Result<Vec<_>>>()?;
        Ok(RunnerPage {
            next_cursor: page_cursor(&items, limit),
            items,
            total,
        })
    }

    /// Reads one runner with its current work summary and lifetime counters.
    ///
    /// # Errors
    /// Reports a missing runner, datastore failures, or malformed stored rows.
    pub async fn runner_detail(&self, runner: &Uuid7) -> Result<RunnerDetail> {
        let mut connection = self.pool().acquire().await?;
        sqlx::query_as::<_, DetailRow>(sql::runner_view::RUNNER_DETAIL)
            .bind(runner.as_str())
            .bind(sql::LEASE_STATUS_ACTIVE)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_RUNNER_DETAIL))?
            .ok_or_else(runner_not_found)?
            .parse()
    }

    /// Lists one runner's append-only history newest first.
    ///
    /// # Errors
    /// Reports a missing runner, datastore failures, or malformed history rows.
    pub async fn runner_events(
        &self,
        runner: &Uuid7,
        filter: &RunnerEventFilter,
        cursor: Option<&KeysetCursor>,
        limit: u32,
    ) -> Result<RunnerEventPage> {
        let mut connection = self.pool().acquire().await?;
        let exists = sqlx::query(sql::runner_view::RUNNER_EXISTS)
            .bind(runner.as_str())
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_RUNNER_EXISTS))?;
        if exists.is_none() {
            return Err(runner_not_found());
        }
        let event_types = filter.event_type_names();
        let total = sqlx::query_scalar(sql::runner_view::COUNT_EVENTS)
            .bind(runner.as_str())
            .bind(event_types.clone())
            .bind(filter.since().map(Timestamp::to_sqlx))
            .bind(filter.until().map(Timestamp::to_sqlx))
            .fetch_one(&mut *connection)
            .await
            .map_err(query(CONTEXT_EVENT_COUNT))?;
        let rows = events::statement(runner, filter, event_types, cursor, limit)
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT_EVENT_LIST))?;
        let items = rows
            .into_iter()
            .map(EventRow::parse)
            .collect::<Result<Vec<_>>>()?;
        Ok(RunnerEventPage {
            next_cursor: events::cursor(&items, limit)?,
            items,
            total,
        })
    }
}

fn page_cursor(items: &[RunnerItem], limit: u32) -> Option<KeysetCursor> {
    (items.len() == limit as usize)
        .then(|| items.last())
        .flatten()
        .map(|item| KeysetCursor::new(item.created_at, item.id.clone()))
}
