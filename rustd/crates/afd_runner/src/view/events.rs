//! Filter and keyset mechanics for runner history reads.

use afd_api_wire::admin::{RunnerEventItem, RunnerEventType};
use afd_core::id::Uuid7;
use afd_core::spelling::to_spelling;
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;

use crate::error::Result;
use crate::sql;

use super::KeysetCursor;
use super::decode::EventRow;

/// The statement an event page runs: its rows decode by name.
type EventPage<'a> =
    sqlx::query::QueryAs<'a, sqlx::Postgres, EventRow, sqlx::postgres::PgArguments>;

/// Validated filters over one runner's append-only history.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunnerEventFilter {
    /// The stored spellings of the types asked for, as the column holds them.
    event_types: Vec<String>,
    since: Option<Timestamp>,
    until: Option<Timestamp>,
}

impl RunnerEventFilter {
    /// Builds a filter when the inclusive time window is ordered and every
    /// type asked for has a stored spelling.
    ///
    /// The spelling is the type's own serde declaration, the one the column was
    /// written with. A type without one refuses the filter rather than dropping
    /// out of it: a dropped type would widen the read to every type.
    #[must_use]
    pub fn new(
        event_types: Vec<RunnerEventType>,
        since: Option<Timestamp>,
        until: Option<Timestamp>,
    ) -> Option<Self> {
        if matches!((since, until), (Some(start), Some(end)) if end < start) {
            return None;
        }
        let event_types = event_types
            .into_iter()
            .map(|event_type| to_spelling(&event_type))
            .collect::<Option<Vec<String>>>()?;
        Some(Self {
            event_types,
            since,
            until,
        })
    }

    pub(super) fn event_type_names(&self) -> Option<Vec<String>> {
        (!self.event_types.is_empty()).then(|| self.event_types.clone())
    }

    pub(super) const fn since(&self) -> Option<Timestamp> {
        self.since
    }

    pub(super) const fn until(&self) -> Option<Timestamp> {
        self.until
    }
}

pub(super) fn statement<'a>(
    runner: &'a Uuid7,
    filter: &RunnerEventFilter,
    event_types: Option<Vec<String>>,
    cursor: Option<&'a KeysetCursor>,
    limit: u32,
) -> EventPage<'a> {
    let since = filter.since().map(Timestamp::to_sqlx);
    let until = filter.until().map(Timestamp::to_sqlx);
    cursor.map_or(
        sqlx::query_as(sql::runner_view::LIST_EVENTS_FIRST)
            .bind(runner.as_str())
            .bind(event_types.clone())
            .bind(since)
            .bind(until)
            .bind(i64::from(limit)),
        |boundary| {
            sqlx::query_as(sql::runner_view::LIST_EVENTS_AFTER)
                .bind(runner.as_str())
                .bind(event_types)
                .bind(since)
                .bind(until)
                .bind(boundary.created_at().to_sqlx())
                .bind(boundary.id().as_str())
                .bind(i64::from(limit))
        },
    )
}

pub(super) fn cursor(
    items: &[RunnerEventItem<'static>],
    limit: u32,
) -> Result<Option<KeysetCursor>> {
    if items.len() != limit as usize {
        return Ok(None);
    }
    items
        .last()
        .map(|item| {
            Uuid7::parse(&item.id)
                .map(|id| KeysetCursor::new(item.occurred_at, id))
                .map_err(crate::error::row_malformed("runner.events", "id"))
        })
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_filter_accepts_open_bounds_and_refuses_a_reversed_window() {
        let start = Timestamp::constant(10, 0);
        let later = Timestamp::constant(11, 0);
        assert!(RunnerEventFilter::new(Vec::new(), None, None).is_some());
        assert!(RunnerEventFilter::new(Vec::new(), Some(start), None).is_some());
        assert!(RunnerEventFilter::new(Vec::new(), None, Some(start)).is_some());
        assert!(RunnerEventFilter::new(Vec::new(), Some(start), Some(start)).is_some());
        assert!(RunnerEventFilter::new(Vec::new(), Some(later), Some(start)).is_none());
    }

    #[test]
    fn event_filter_binds_each_type_by_the_spelling_the_column_stores() {
        let filter = RunnerEventFilter::new(
            vec![
                RunnerEventType::RunnerTokenRotated,
                RunnerEventType::LeaseAcquired,
            ],
            None,
            None,
        );
        // pin test: literal is the contract
        assert_eq!(
            filter.and_then(|filter| filter.event_type_names()),
            Some(vec![
                "runner_token_rotated".to_owned(),
                "lease_acquired".to_owned()
            ])
        );
        let unfiltered = RunnerEventFilter::new(Vec::new(), None, None);
        assert_eq!(
            unfiltered.and_then(|filter| filter.event_type_names()),
            None
        );
    }
}
