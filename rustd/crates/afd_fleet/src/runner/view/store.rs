use super::*;

const CONTEXT_RUNNER_COUNT: &str = "runner list count";
const CONTEXT_RUNNER_LIST: &str = "runner list page";
const CONTEXT_RUNNER_DETAIL: &str = "runner detail";
const CONTEXT_RUNNER_EXISTS: &str = "runner event owner read";
const CONTEXT_EVENT_COUNT: &str = "runner event count";
const CONTEXT_EVENT_LIST: &str = "runner event page";

impl Runners {
    /// Lists runners newest first, deriving liveness at `now`.
    ///
    /// # Errors
    /// Reports datastore failures or malformed stored rows.
    pub async fn list_runners(
        &self,
        cursor: Option<&KeysetCursor>,
        limit: PageLimit,
        now: UnixMillis,
    ) -> Result<RunnerPage> {
        let mut connection = self.pool().acquire().await?;
        let total = sqlx::query(sql::runner_view::COUNT_RUNNERS)
            .fetch_one(&mut *connection)
            .await
            .map_err(query(CONTEXT_RUNNER_COUNT))?
            .try_get(0)
            .map_err(query(CONTEXT_RUNNER_COUNT))?;
        let statement = cursor.map_or(
            sqlx::query(sql::runner_view::LIST_RUNNERS_FIRST)
                .bind(sql::LEASE_STATUS_ACTIVE)
                .bind(now.as_millis())
                .bind(limit.as_i64()),
            |boundary| {
                sqlx::query(sql::runner_view::LIST_RUNNERS_AFTER)
                    .bind(sql::LEASE_STATUS_ACTIVE)
                    .bind(now.as_millis())
                    .bind(boundary.created_at())
                    .bind(boundary.id().as_str())
                    .bind(limit.as_i64())
            },
        );
        let rows = statement
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT_RUNNER_LIST))?;
        let items = rows
            .iter()
            .map(|row| runner_item(row, now))
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
    pub async fn runner_detail(&self, runner: &Uuid7, now: UnixMillis) -> Result<RunnerDetail> {
        let mut connection = self.pool().acquire().await?;
        let row = sqlx::query(sql::runner_view::RUNNER_DETAIL)
            .bind(runner.as_str())
            .bind(sql::LEASE_STATUS_ACTIVE)
            .bind(now.as_millis())
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_RUNNER_DETAIL))?
            .ok_or_else(runner_not_found)?;
        runner_detail(&row, now)
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
        limit: PageLimit,
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
        let total = sqlx::query(sql::runner_view::COUNT_EVENTS)
            .bind(runner.as_str())
            .bind(event_types.clone())
            .bind(filter.since())
            .bind(filter.until())
            .fetch_one(&mut *connection)
            .await
            .map_err(query(CONTEXT_EVENT_COUNT))?
            .try_get(0)
            .map_err(query(CONTEXT_EVENT_COUNT))?;
        let rows = events::statement(runner, filter, event_types, cursor, limit)
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT_EVENT_LIST))?;
        let items = rows.iter().map(runner_event).collect::<Result<Vec<_>>>()?;
        Ok(RunnerEventPage {
            next_cursor: events::cursor(&items, limit)?,
            items,
            total,
        })
    }
}

fn page_cursor(items: &[RunnerItem], limit: PageLimit) -> Option<KeysetCursor> {
    (items.len() == limit.get() as usize)
        .then(|| items.last())
        .flatten()
        .map(|item| KeysetCursor::new(item.created_at, item.id.clone()))
}
