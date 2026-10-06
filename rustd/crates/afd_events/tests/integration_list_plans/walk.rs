//! The walking half of the plan suite: every listing and the thread, paged a
//! few rows at a time through `History` to the end.
//!
//! A walk that returns every seeded row once, in keyset order, proves each
//! text numbers its placeholders the way `History` binds them, and that the
//! cursor pair carries two events in one millisecond across a page boundary.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_events::{Cursor, EventRow, Filter, History, next_cursor, prefix_to_like};

use crate::support::EventsLane;

/// Events the walks page through, two to a millisecond.
const EVENTS: i64 = 24;
/// Rows per page: few, so every walk resumes several times and ends deep.
const PAGE: i64 = 5;
/// The oldest walked event's timestamp; each later pair is one step newer.
const FIRST_MS: i64 = 1_800_000_000_000;
const STEP_MS: i64 = 1_000;
/// The prefix of the actor every walked event carries (`EventsLane::seed_event`),
/// and one none of them does.
const SEEDED_ACTOR: &str = "steer:";
const OTHER_ACTOR: &str = "webhook:";
/// The whole actor every walked event carries, for the exact-actor read.
const SEEDED_ACTOR_EXACT: &str = "steer:api";

/// The lane's workspace and fleet, as `History` takes them.
struct Scope {
    workspace: Uuid7,
    fleet: Uuid7,
}

impl Scope {
    fn of(lane: &EventsLane) -> Self {
        Self {
            workspace: Uuid7::parse(&lane.workspace).expect("the fixture workspace is canonical"),
            fleet: Uuid7::parse(&lane.fleet).expect("the fixture fleet is canonical"),
        }
    }
}

/// Which listing entry point a walk pages through.
#[derive(Debug, Clone, Copy)]
enum Listing {
    Fleet,
    Workspace,
    /// The workspace listing narrowed to one fleet, which runs the fleet texts.
    WorkspaceDrillDown,
}

/// Seeds the walked events and answers `(created_at, event_id)` for each,
/// newest first, as every page orders them.
pub(super) async fn seed_walk(lane: &EventsLane) -> Vec<(i64, String)> {
    let mut seeded = Vec::new();
    for nth in 0..EVENTS {
        let created_at = FIRST_MS + (nth / 2) * STEP_MS;
        let event_id = format!("{created_at}-{}", nth % 2);
        lane.seed_event(&event_id, created_at).await;
        seeded.push((created_at, event_id));
    }
    seeded.sort_by(|older, newer| newer.cmp(older));
    seeded
}

/// One page of `listing`.
async fn listing_page(
    history: &History,
    scope: &Scope,
    listing: Listing,
    filter: &Filter,
    cursor: Option<&Cursor>,
) -> Vec<EventRow> {
    let (workspace, fleet) = (&scope.workspace, &scope.fleet);
    match listing {
        Listing::Fleet => {
            history
                .page_for_fleet(workspace, fleet, filter, cursor, PAGE)
                .await
        }
        Listing::Workspace => {
            history
                .page_for_workspace(workspace, None, filter, cursor, PAGE)
                .await
        }
        Listing::WorkspaceDrillDown => {
            history
                .page_for_workspace(workspace, Some(fleet), filter, cursor, PAGE)
                .await
        }
    }
    .expect("a history page must read")
}

/// Every event id `listing` serves, walked page by page to the end.
async fn walk_listing(
    history: &History,
    scope: &Scope,
    listing: Listing,
    filter: &Filter,
) -> Vec<String> {
    let mut seen = Vec::new();
    let mut cursor = None;
    loop {
        let page = listing_page(history, scope, listing, filter, cursor.as_ref()).await;
        seen.extend(page.iter().map(|row| row.event_id.clone()));
        match next_cursor(&page, PAGE) {
            Some(next) => cursor = Some(next),
            None => return seen,
        }
    }
}

/// Every event id `actor` has in the fleet, walked page by page to the end.
async fn walk_actor(history: &History, scope: &Scope, actor: &str) -> Vec<String> {
    let mut seen = Vec::new();
    let mut cursor = None;
    loop {
        let page = history
            .page_of_actor(&scope.workspace, &scope.fleet, actor, cursor.as_ref(), PAGE)
            .await
            .expect("an actor page must read");
        seen.extend(page.iter().map(|row| row.event_id.clone()));
        match next_cursor(&page, PAGE) {
            Some(next) => cursor = Some(next),
            None => return seen,
        }
    }
}

/// Every event id the fleet's thread serves, walked page by page to the end.
async fn walk_thread(history: &History, scope: &Scope) -> Vec<String> {
    let mut seen = Vec::new();
    let mut cursor: Option<Cursor> = None;
    loop {
        let page = history
            .thread_page(&scope.workspace, &scope.fleet, cursor.as_ref(), PAGE)
            .await
            .expect("a thread page must read");
        seen.extend(page.iter().map(|detail| detail.row.event_id.clone()));
        let full = i64::try_from(page.len()).is_ok_and(|len| len == PAGE);
        match page.last().filter(|_| full) {
            Some(last) => cursor = Some(Cursor::after(last.row.created_at, &last.row.event_id)),
            None => return seen,
        }
    }
}

/// The seeded events' ids at or after `since`, newest first.
fn ids_since(seeded: &[(i64, String)], since: i64) -> Vec<String> {
    seeded
        .iter()
        .filter(|(created_at, _)| *created_at >= since)
        .map(|(_, id)| id.clone())
        .collect()
}

/// A listing filter, from an actor prefix and a `since` millisecond.
fn filter(actor_prefix: Option<&str>, since: Option<i64>) -> Filter {
    Filter {
        actor_like: actor_prefix.map(prefix_to_like),
        since: since.map(UnixMillis::from_millis),
    }
}

/// Every listing walks exactly the events each filter admits, so every text
/// `History` picks runs with its binds, and the thread walks them all.
pub(super) async fn assert_walks(lane: &EventsLane, seeded: &[(i64, String)]) {
    let history = History::new(lane.database.clone());
    let scope = Scope::of(lane);
    let since = FIRST_MS + (EVENTS / 4) * STEP_MS;
    let everything = ids_since(seeded, i64::MIN);
    let recent = ids_since(seeded, since);
    let cases = [
        (filter(None, None), everything.clone()),
        (filter(None, Some(since)), recent.clone()),
        (filter(Some(SEEDED_ACTOR), None), everything.clone()),
        (filter(Some(SEEDED_ACTOR), Some(since)), recent),
        (filter(Some(OTHER_ACTOR), None), Vec::new()),
    ];
    let listings = [
        Listing::Fleet,
        Listing::Workspace,
        Listing::WorkspaceDrillDown,
    ];
    for (filter, expected) in &cases {
        for listing in listings {
            let walked = walk_listing(&history, &scope, listing, filter).await;
            assert_eq!(&walked, expected, "{listing:?} under {filter:?}");
        }
    }
    assert_eq!(
        walk_actor(&history, &scope, SEEDED_ACTOR_EXACT).await,
        everything,
        "the exact-actor walk"
    );
    assert!(
        walk_actor(&history, &scope, SEEDED_ACTOR).await.is_empty(),
        "an exact read never matches a prefix"
    );
    assert_eq!(
        walk_thread(&history, &scope).await,
        everything,
        "the thread walk"
    );
}
