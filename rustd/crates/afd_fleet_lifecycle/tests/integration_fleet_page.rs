//! The fleet list's two statements walk one workspace and read rows alike.
//!
//! The first page and every later page are different statements over one
//! column list, decoded by position. A walk one row at a time runs the later
//! statement at every boundary, so it is compared row for row with the first
//! statement's whole page; a neighbour's fleet created just before the walk's
//! rows sits after every boundary and must never surface.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::id::Uuid7;
use afd_fleet_lifecycle::{After, FleetRow};
use jiff::SignedDuration;

use crate::integration_wall_counters::{RAN, seed_counters};
use crate::support::{Lane, NOW};

/// A page larger than the lane seeds, so one read holds the whole workspace.
const WHOLE_PAGE: u32 = 50;

/// One row a page, so every row after the first comes from the later statement.
const ONE_ROW: u32 = 1;

/// How many fleets the walked workspace holds.
const OWN_FLEETS: usize = 3;

/// A deadlock guard: a seek that failed to advance would otherwise spin.
const MAX_PAGES: usize = 16;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres and Dragonfly"]
async fn the_fleet_page_walks_one_workspace_and_both_statements_read_rows_alike() {
    let lane = Lane::create().await;
    let mut own = Vec::new();
    for _ in 0..OWN_FLEETS {
        own.push(lane.fleet().await);
    }
    let ran = own.last().expect("the walked workspace holds fleets");
    seed_counters(&lane, ran, RAN).await;
    let neighbour = lane.another_workspace().await;
    let theirs = lane
        .fleet_at(
            &neighbour,
            NOW.checked_sub(SignedDuration::from_millis(1))
                .expect("the lane's instant has a millisecond before it"),
        )
        .await;
    seed_counters(&lane, &theirs, RAN).await;

    let whole = lane
        .fleets
        .page(&lane.workspace, None, WHOLE_PAGE)
        .await
        .expect("the workspace pages")
        .rows;
    let walked = walk(&lane).await;

    assert_eq!(
        walked, whole,
        "the later statement reads every row the first does, field for field"
    );
    let mut listed: Vec<&str> = walked.iter().map(|row| row.id.as_str()).collect();
    listed.sort_unstable();
    let mut expected: Vec<&str> = own.iter().map(Uuid7::as_str).collect();
    expected.sort_unstable();
    assert_eq!(listed, expected, "only this workspace's fleets list");

    lane.cleanup().await;
}

/// Every row of the lane's workspace, one per page.
async fn walk(lane: &Lane) -> Vec<FleetRow> {
    let mut walked = Vec::new();
    let mut after: Option<After> = None;
    for _page in 0..MAX_PAGES {
        let page = lane
            .fleets
            .page(&lane.workspace, after.as_ref(), ONE_ROW)
            .await
            .expect("the workspace pages");
        let Some(row) = page.rows.into_iter().next() else {
            break;
        };
        after = Some(After {
            created_at: row.created_at,
            id: Uuid7::parse(&row.id).expect("a listed id is a v7 spelling"),
        });
        walked.push(row);
        if !page.more {
            break;
        }
    }
    walked
}
