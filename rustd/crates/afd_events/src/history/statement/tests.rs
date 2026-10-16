//! The read texts the plan suite asks Postgres about are the ones `History`
//! picks between.
//!
//! What each text returns and how Postgres plans it are graded live, in
//! `tests/integration_list_plans/mod.rs`; this keeps that suite's list complete.
#![cfg(feature = "test-util")]

use std::collections::BTreeSet;

use super::*;

/// The texts outside the listing grid.
const THREADS: [&str; 5] = [
    SELECT_THREAD_PAGE,
    SELECT_THREAD_PAGE_AFTER,
    SELECT_THREAD_FINISHED_BEFORE,
    SELECT_FLEET_PAGE_OF_ACTOR,
    SELECT_FLEET_PAGE_OF_ACTOR_AFTER,
];

/// Every listing text `History::page` can pick, by `(fleet_scoped, resumes,
/// by_actor)`, and every thread text.
fn every_text() -> BTreeSet<&'static str> {
    [false, true]
        .into_iter()
        .flat_map(|fleet| {
            [false, true].into_iter().flat_map(move |resumes| {
                [false, true].map(|actor| listing_text(fleet, resumes, actor))
            })
        })
        .chain(THREADS)
        .collect()
}

/// The suite that plans the texts plans every one `History` runs.
#[test]
fn the_planned_texts_are_every_text_history_runs() {
    let planned: BTreeSet<&str> = READ_TEXTS.iter().map(|(_, text)| *text).collect();
    assert_eq!(planned, every_text());
}
