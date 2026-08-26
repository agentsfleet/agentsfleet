//! Operator-plane runner list, detail, and history reads.

mod decode;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_wire::admin::RunnerEventItem;
use afd_wire::runner::{AssignedPolicy, CapabilityReport, RunnerLiveness, SelftestReport};
use sqlx::Row as _;

use crate::error::{Result, query, runner_not_found};
use crate::runner::Runners;
use crate::sql;

use self::decode::{runner_detail, runner_event, runner_item};

const CONTEXT_RUNNER_COUNT: &str = "runner list count";
const CONTEXT_RUNNER_LIST: &str = "runner list page";
const CONTEXT_RUNNER_DETAIL: &str = "runner detail";
const CONTEXT_RUNNER_EXISTS: &str = "runner event owner read";
const CONTEXT_EVENT_COUNT: &str = "runner event count";
const CONTEXT_EVENT_LIST: &str = "runner event page";

/// The runner page size used when a caller omits `limit`.
pub const DEFAULT_PAGE_LIMIT: u32 = 50;
/// The largest runner page the public API accepts.
pub const MAX_PAGE_LIMIT: u32 = 100;

/// A page size already proven to be inside the public API bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageLimit(u32);

impl PageLimit {
    /// Builds a limit in the inclusive range 1 through 100.
    #[must_use]
    pub const fn new(value: u32) -> Option<Self> {
        if value == 0 || value > MAX_PAGE_LIMIT {
            None
        } else {
            Some(Self(value))
        }
    }

    /// The checked value for response sizing.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    fn as_i64(self) -> i64 {
        i64::from(self.0)
    }
}

impl Default for PageLimit {
    fn default() -> Self {
        Self(DEFAULT_PAGE_LIMIT)
    }
}

/// The final composite key from a page, used to seek the next one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeysetCursor {
    created_at: i64,
    id: Uuid7,
}

impl KeysetCursor {
    /// A cursor from a row the service already decoded.
    #[must_use]
    pub const fn new(created_at: i64, id: Uuid7) -> Self {
        Self { created_at, id }
    }

    /// The timestamp half of the database boundary.
    #[must_use]
    pub const fn created_at(&self) -> i64 {
        self.created_at
    }

    /// The identifier half of the database boundary.
    #[must_use]
    pub const fn id(&self) -> &Uuid7 {
        &self.id
    }
}

/// One operator list row. Authentication material is unrepresentable here.
#[derive(Debug, Clone, PartialEq)]
pub struct RunnerItem {
    /// Canonical runner identifier.
    pub id: Uuid7,
    /// Stable host identity supplied at enrolment.
    pub host_id: String,
    /// Assigned isolation tier spelling.
    pub sandbox_tier: String,
    /// Operator-controlled admission state.
    pub admin_state: afd_wire::admin::AdminState,
    /// Runtime state derived from heartbeat and lease rows.
    pub liveness: RunnerLiveness,
    /// Placement labels assigned at enrolment.
    pub labels: Vec<String>,
    /// Last heartbeat instant in epoch milliseconds.
    pub last_seen_at: i64,
    /// Enrolment instant in epoch milliseconds.
    pub created_at: i64,
    /// Policy currently assigned to the host.
    pub assigned_policy: Option<AssignedPolicy<'static>>,
    /// Capability report most recently supplied by the host.
    pub achievable: Option<CapabilityReport<'static>>,
    /// Whether the assigned policy exceeds the reported capability.
    pub degraded: bool,
    /// Stored explanation for a degraded verdict.
    pub degraded_reason: Option<String>,
}

/// A keyset page of runners.
#[derive(Debug, Clone, PartialEq)]
pub struct RunnerPage {
    /// Rows in newest-first keyset order.
    pub items: Vec<RunnerItem>,
    /// Total runners independent of this page boundary.
    pub total: i64,
    /// Boundary for the next page, absent when this page is short.
    pub next_cursor: Option<KeysetCursor>,
}

/// The single-runner read with live and lifetime counters.
#[derive(Debug, Clone, PartialEq)]
pub struct RunnerDetail {
    /// The same runner shape the list returns.
    pub item: RunnerItem,
    /// Live leases currently held by this runner.
    pub active_lease_count: i64,
    /// Distinct fleets represented by those live leases.
    pub active_fleet_count: i64,
    /// Leases acquired over the runner's lifetime.
    pub leases_acquired: i64,
    /// Leases settled successfully over the runner's lifetime.
    pub leases_succeeded: i64,
    /// Leases settled unsuccessfully over the runner's lifetime.
    pub leases_failed: i64,
    /// Leases expired over the runner's lifetime.
    pub leases_expired: i64,
    /// Outstanding self-test request instant.
    pub selftest_requested_at: Option<i64>,
    /// Most recent self-test completion instant.
    pub selftest_completed_at: Option<i64>,
    /// Most recent complete self-test report.
    pub selftest: Option<SelftestReport<'static>>,
}

/// A keyset page of append-only runner history.
#[derive(Debug, Clone, PartialEq)]
pub struct RunnerEventPage {
    /// History rows in newest-first keyset order.
    pub items: Vec<RunnerEventItem<'static>>,
    /// Total history rows for this runner.
    pub total: i64,
    /// Boundary for the next page, absent when this page is short.
    pub next_cursor: Option<KeysetCursor>,
}

impl Runners {
    /// Lists runners newest first, deriving liveness at `now`.
    ///
    /// # Errors
    /// Reports an unavailable datastore, a refused statement, or a stored row
    /// whose identifier, state, or JSON fields cannot be read safely.
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
    /// Reports a missing runner, an unavailable datastore, a refused
    /// statement, or a stored row whose fields cannot be read safely.
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
    /// Reports a missing runner, an unavailable datastore, a refused
    /// statement, or a history row whose identifier or JSON cannot be read.
    pub async fn runner_events(
        &self,
        runner: &Uuid7,
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
        let total = sqlx::query(sql::runner_view::COUNT_EVENTS)
            .bind(runner.as_str())
            .fetch_one(&mut *connection)
            .await
            .map_err(query(CONTEXT_EVENT_COUNT))?
            .try_get(0)
            .map_err(query(CONTEXT_EVENT_COUNT))?;
        let rows = event_statement(runner, cursor, limit)
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT_EVENT_LIST))?;
        let items = rows.iter().map(runner_event).collect::<Result<Vec<_>>>()?;
        Ok(RunnerEventPage {
            next_cursor: event_cursor(&items, limit)?,
            items,
            total,
        })
    }
}

fn event_statement<'a>(
    runner: &'a Uuid7,
    cursor: Option<&KeysetCursor>,
    limit: PageLimit,
) -> sqlx::query::Query<'a, sqlx::Postgres, sqlx::postgres::PgArguments> {
    cursor.map_or(
        sqlx::query(sql::runner_view::LIST_EVENTS_FIRST)
            .bind(runner.as_str())
            .bind(limit.as_i64()),
        |boundary| {
            sqlx::query(sql::runner_view::LIST_EVENTS_AFTER)
                .bind(runner.as_str())
                .bind(boundary.created_at())
                .bind(boundary.id().as_str())
                .bind(limit.as_i64())
        },
    )
}

fn page_cursor(items: &[RunnerItem], limit: PageLimit) -> Option<KeysetCursor> {
    (items.len() == limit.get() as usize)
        .then(|| items.last())
        .flatten()
        .map(|item| KeysetCursor::new(item.created_at, item.id.clone()))
}

fn event_cursor(
    items: &[RunnerEventItem<'static>],
    limit: PageLimit,
) -> Result<Option<KeysetCursor>> {
    if items.len() != limit.get() as usize {
        return Ok(None);
    }
    items
        .last()
        .map(|item| {
            Uuid7::parse(&item.id)
                .map(|id| KeysetCursor::new(item.occurred_at, id))
                .map_err(crate::error::row_malformed("fleet.runner_events", "id"))
        })
        .transpose()
}

#[cfg(test)]
mod tests;
