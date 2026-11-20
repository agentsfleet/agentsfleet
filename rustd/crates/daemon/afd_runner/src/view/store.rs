use af_core::id::Uuid7;
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;
use sqlx::Acquire as _;

use super::decode::{DetailRow, ItemRow, TurnRow};
use super::{
    KeysetCursor, RunnerDetail, RunnerHistoryFilter, RunnerHistoryPage, RunnerItem, RunnerPage,
    history,
};
use crate::error::{Result, query, runner_not_found};
use crate::scope::RunnerScope;
use crate::sql;
use crate::store::Runners;

const CONTEXT_SNAPSHOT: &str = "page read snapshot";
const CONTEXT_RUNNER_COUNT: &str = "runner list count";
const CONTEXT_RUNNER_LIST: &str = "runner list page";
const CONTEXT_RUNNER_DETAIL: &str = "runner detail";
const CONTEXT_RUNNER_EXISTS: &str = "runner event owner read";
const CONTEXT_EVENT_COUNT: &str = "runner event count";
const CONTEXT_EVENT_LIST: &str = "runner event page";

impl Runners {
    /// Lists one runner's append-only history newest first.
    ///
    /// # Errors
    /// Reports a missing runner, datastore failures, or malformed history rows.
    pub async fn runner_history(
        &self,
        runner: &Uuid7,
        filter: &RunnerHistoryFilter,
        cursor: Option<&KeysetCursor>,
        limit: u32,
    ) -> Result<RunnerHistoryPage> {
        let mut connection = self.pool().acquire().await?;
        let mut snapshot = connection.begin().await.map_err(query(CONTEXT_SNAPSHOT))?;
        sqlx::query(sql::runner_view::READ_SNAPSHOT)
            .execute(&mut *snapshot)
            .await
            .map_err(query(CONTEXT_SNAPSHOT))?;
        let exists = sqlx::query(sql::runner_view::RUNNER_EXISTS)
            .bind(runner)
            .fetch_optional(&mut *snapshot)
            .await
            .map_err(query(CONTEXT_RUNNER_EXISTS))?;
        if exists.is_none() {
            return Err(runner_not_found());
        }
        let event_types = filter.event_type_names();
        let total = sqlx::query_scalar(sql::runner_view::COUNT_EVENTS)
            .bind(runner)
            .bind(event_types.clone())
            .bind(filter.since().map(Timestamp::to_sqlx))
            .bind(filter.until().map(Timestamp::to_sqlx))
            .fetch_one(&mut *snapshot)
            .await
            .map_err(query(CONTEXT_EVENT_COUNT))?;
        let mut rows = history::statement(runner, filter, event_types, cursor, limit)
            .fetch_all(&mut *snapshot)
            .await
            .map_err(query(CONTEXT_EVENT_LIST))?;
        snapshot.commit().await.map_err(query(CONTEXT_SNAPSHOT))?;
        let more = trim_to_page(&mut rows, limit);
        let items = rows
            .into_iter()
            .map(TurnRow::parse)
            .collect::<Result<Vec<_>>>()?;
        Ok(RunnerHistoryPage {
            next_cursor: history::cursor(&items, more)?,
            items,
            total,
        })
    }
}

impl RunnerScope<'_> {
    /// Lists runners newest first, deriving liveness at the database's clock.
    /// The total and the page read one snapshot, so a runner registered
    /// between them is in both or in neither.
    ///
    /// # Errors
    /// Reports datastore failures or malformed stored rows.
    pub async fn list_runners(
        &self,
        cursor: Option<&KeysetCursor>,
        limit: u32,
    ) -> Result<RunnerPage> {
        let mut connection = self.runners.pool().acquire().await?;
        let mut snapshot = connection.begin().await.map_err(query(CONTEXT_SNAPSHOT))?;
        sqlx::query(sql::runner_view::READ_SNAPSHOT)
            .execute(&mut *snapshot)
            .await
            .map_err(query(CONTEXT_SNAPSHOT))?;
        let total = sqlx::query_scalar(sql::runner_view::COUNT_RUNNERS)
            .bind(self.owner.tenant())
            .bind(self.owner.platform())
            .fetch_one(&mut *snapshot)
            .await
            .map_err(query(CONTEXT_RUNNER_COUNT))?;
        let statement = cursor.map_or_else(
            || {
                sqlx::query_as::<_, ItemRow>(sql::runner_view::LIST_RUNNERS_FIRST)
                    .bind(sql::RUN_STATUS_ACTIVE)
                    .bind(past_page(limit))
                    .bind(self.owner.tenant())
            },
            |boundary| {
                sqlx::query_as::<_, ItemRow>(sql::runner_view::LIST_RUNNERS_AFTER)
                    .bind(sql::RUN_STATUS_ACTIVE)
                    .bind(boundary.created_at().to_sqlx())
                    .bind(boundary.id())
                    .bind(past_page(limit))
                    .bind(self.owner.tenant())
            },
        );
        let mut rows = statement
            .bind(self.owner.platform())
            .fetch_all(&mut *snapshot)
            .await
            .map_err(query(CONTEXT_RUNNER_LIST))?;
        snapshot.commit().await.map_err(query(CONTEXT_SNAPSHOT))?;
        let more = trim_to_page(&mut rows, limit);
        let items = rows
            .into_iter()
            .map(|row| row.parse(self.runners.clock()))
            .collect::<Result<Vec<_>>>()?;
        Ok(RunnerPage {
            next_cursor: page_cursor(&items, more),
            items,
            total,
        })
    }

    /// Reads one runner with its current work summary and lifetime counters.
    ///
    /// # Errors
    /// Reports a missing runner, datastore failures, or malformed stored rows.
    pub async fn runner_detail(&self, runner: &Uuid7) -> Result<RunnerDetail> {
        let mut connection = self.runners.pool().acquire().await?;
        sqlx::query_as::<_, DetailRow>(sql::runner_view::RUNNER_DETAIL)
            .bind(runner)
            .bind(sql::RUN_STATUS_ACTIVE)
            .bind(self.owner.tenant())
            .bind(self.owner.platform())
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_RUNNER_DETAIL))?
            .ok_or_else(runner_not_found)?
            .parse(self.runners.clock())
    }
}

/// The row count a page reads: one past `limit`, so the extra row can prove
/// that another page follows. A full page alone does not prove it, because the
/// stream may end exactly on it.
pub(super) fn past_page(limit: u32) -> i64 {
    i64::from(limit) + 1
}

/// Cuts `rows` back to `limit` and answers whether a row lay past it.
fn trim_to_page<T>(rows: &mut Vec<T>, limit: u32) -> bool {
    let page = usize::try_from(limit).unwrap_or(usize::MAX);
    let more = rows.len() > page;
    rows.truncate(page);
    more
}

fn page_cursor(items: &[RunnerItem], more: bool) -> Option<KeysetCursor> {
    more.then(|| items.last())
        .flatten()
        .map(|item| KeysetCursor::new(item.created_at, item.id.clone()))
}
