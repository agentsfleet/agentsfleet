//! Filter and keyset mechanics for runner history reads.

use afd_core::id::Uuid7;
use afd_wire::admin::{RunnerEventItem, RunnerEventType};

use crate::error::Result;
use crate::sql;

use super::{KeysetCursor, PageLimit};

/// Validated filters over one runner's append-only history.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunnerEventFilter {
    event_types: Vec<RunnerEventType>,
    since: Option<i64>,
    until: Option<i64>,
}

impl RunnerEventFilter {
    /// Builds a filter when the inclusive time window is ordered.
    #[must_use]
    pub fn new(
        event_types: Vec<RunnerEventType>,
        since: Option<i64>,
        until: Option<i64>,
    ) -> Option<Self> {
        if matches!((since, until), (Some(start), Some(end)) if end < start) {
            return None;
        }
        Some(Self {
            event_types,
            since,
            until,
        })
    }

    pub(super) fn event_type_names(&self) -> Option<Vec<String>> {
        (!self.event_types.is_empty()).then(|| {
            self.event_types
                .iter()
                .map(|event_type| event_type.as_str().to_owned())
                .collect()
        })
    }

    pub(super) const fn since(&self) -> Option<i64> {
        self.since
    }

    pub(super) const fn until(&self) -> Option<i64> {
        self.until
    }
}

pub(super) fn statement<'a>(
    runner: &'a Uuid7,
    filter: &RunnerEventFilter,
    event_types: Option<Vec<String>>,
    cursor: Option<&KeysetCursor>,
    limit: PageLimit,
) -> sqlx::query::Query<'a, sqlx::Postgres, sqlx::postgres::PgArguments> {
    cursor.map_or(
        sqlx::query(sql::runner_view::LIST_EVENTS_FIRST)
            .bind(runner.as_str())
            .bind(event_types.clone())
            .bind(filter.since())
            .bind(filter.until())
            .bind(limit.as_i64()),
        |boundary| {
            sqlx::query(sql::runner_view::LIST_EVENTS_AFTER)
                .bind(runner.as_str())
                .bind(event_types)
                .bind(filter.since())
                .bind(filter.until())
                .bind(boundary.created_at())
                .bind(boundary.id().as_str())
                .bind(limit.as_i64())
        },
    )
}

pub(super) fn cursor(
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
mod tests {
    use super::*;

    #[test]
    fn event_filter_accepts_open_bounds_and_refuses_a_reversed_window() {
        assert!(RunnerEventFilter::new(Vec::new(), None, None).is_some());
        assert!(RunnerEventFilter::new(Vec::new(), Some(10), None).is_some());
        assert!(RunnerEventFilter::new(Vec::new(), None, Some(10)).is_some());
        assert!(RunnerEventFilter::new(Vec::new(), Some(10), Some(10)).is_some());
        assert!(RunnerEventFilter::new(Vec::new(), Some(11), Some(10)).is_none());
    }
}
