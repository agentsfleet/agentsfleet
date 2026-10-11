//! Filter and keyset mechanics for runner history reads.

use afd_api_wire::admin::{RunnerEventItem, RunnerEventType};
use afd_core::id::Uuid7;
use afd_core::spelling::to_spelling;

use crate::error::Result;
use crate::sql;

use super::KeysetCursor;

/// Validated filters over one runner's append-only history.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunnerEventFilter {
    /// The stored spellings of the types asked for, as the column holds them.
    event_types: Vec<String>,
    since: Option<i64>,
    until: Option<i64>,
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
        since: Option<i64>,
        until: Option<i64>,
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
    limit: u32,
) -> afd_db::statement::Bound<'a> {
    cursor.map_or(
        sqlx::query(sql::runner_view::LIST_EVENTS_FIRST)
            .bind(runner.as_str())
            .bind(event_types.clone())
            .bind(filter.since())
            .bind(filter.until())
            .bind(i64::from(limit)),
        |boundary| {
            sqlx::query(sql::runner_view::LIST_EVENTS_AFTER)
                .bind(runner.as_str())
                .bind(event_types)
                .bind(filter.since())
                .bind(filter.until())
                .bind(boundary.created_at())
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
