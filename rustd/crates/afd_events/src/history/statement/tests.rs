//! The read texts carry one column list, and the plan suite plans every text
//! `History` picks between.
//!
//! What each text returns and how Postgres plans it are graded live, in
//! `tests/integration_list_plans/mod.rs`; this keeps the files in step with
//! each other and that suite's list complete.

use super::*;

/// The fourteen columns every read selects, in decode order, as each file
/// spells them. Written out here rather than read from one of the files, so a
/// file edited on its own disagrees with this and fails.
const ROW_COLUMNS: &str = "\
SELECT fleet_id::text AS fleet_id, event_id, workspace_id::text AS workspace_id,
       actor, event_type, status, token_count, wall_ms,
       failure_reason, failure_detail, resumes_event_id,
       created_at, updated_at,
       (SELECT SUM(charge.charged_nanos)::bigint
          FROM billing.fleet_charges charge
         WHERE charge.event_id = fleet.events.event_id
           AND charge.fleet_id = fleet.events.fleet_id) AS cost_nanos";

/// The bodies the detail and thread reads append after the row.
const BODY_COLUMNS: &str = ",
       request_json::text AS request_json, response_text, tool_calls::text AS tool_calls
FROM fleet.events";

/// The texts outside the listing grid.
const THREADS: [&str; 5] = [
    SELECT_THREAD_PAGE,
    SELECT_THREAD_PAGE_AFTER,
    SELECT_THREAD_FINISHED_BEFORE,
    SELECT_FLEET_PAGE_OF_ACTOR,
    SELECT_FLEET_PAGE_OF_ACTOR_AFTER,
];

/// Every listing text `History::page` can pick, by `(fleet_scoped, resumes,
/// by_actor)`.
fn listings() -> Vec<&'static str> {
    [false, true]
        .into_iter()
        .flat_map(|fleet| {
            [false, true].into_iter().flat_map(move |resumes| {
                [false, true].map(|actor| listing_text(fleet, resumes, actor))
            })
        })
        .collect()
}

#[test]
fn every_read_opens_with_the_one_row_column_list() {
    for text in listings().into_iter().chain(THREADS).chain([SELECT_DETAIL]) {
        assert!(text.starts_with(ROW_COLUMNS), "drifted:\n{text}");
    }
}

#[test]
fn every_body_read_appends_the_bodies_after_the_row() {
    for text in [
        SELECT_DETAIL,
        SELECT_THREAD_PAGE,
        SELECT_THREAD_PAGE_AFTER,
        SELECT_THREAD_FINISHED_BEFORE,
    ] {
        let rest = text.strip_prefix(ROW_COLUMNS).unwrap_or_default();
        assert!(rest.starts_with(BODY_COLUMNS), "drifted:\n{text}");
    }
}

#[test]
fn every_listing_bounds_since_on_the_database_clock() {
    for text in listings() {
        assert!(
            text.contains("statement_timestamp() - $"),
            "a listing resolves its window in Rust:\n{text}"
        );
    }
}

/// The suite that plans the texts plans every one `History` runs.
#[cfg(feature = "test-util")]
#[test]
fn the_planned_texts_are_every_text_history_runs() {
    use std::collections::BTreeSet;

    let planned: BTreeSet<&str> = READ_TEXTS.iter().map(|(_, text)| *text).collect();
    let every: BTreeSet<&str> = listings().into_iter().chain(THREADS).collect();
    assert_eq!(planned, every);
}
