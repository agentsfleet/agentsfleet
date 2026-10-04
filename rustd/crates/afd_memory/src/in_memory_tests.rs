//! The in-memory store answers each verb the way the trait says a store does:
//! the operator page's keyset, where two writers' shared entries tied on
//! `created_at` and `key` both reach a walk, then the window, recall, the
//! filtered pages and forget, each holding another fleet's shared entries only
//! for a reader and never touching another fleet's rows.
#![expect(
    clippy::expect_used,
    reason = "test module: an unmet precondition should fail the test loudly"
)]

use std::borrow::Cow;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_wire::memory::{MemoryDelta, PINNED_CATEGORY, Visibility};

use crate::page::{After, View};
use crate::record::{Owner, Record};
use crate::window::DAILY_CATEGORY;
use crate::{InMemory, MemoryStore as _};

const WORKSPACE: &str = "01990000-0000-7000-8000-0000000000c1";
/// Two publishers, in ascending identifier order.
const WRITERS: [&str; 2] = [
    "01990000-0000-7000-8000-0000000000d1",
    "01990000-0000-7000-8000-0000000000d2",
];
/// A fleet holding the read grant and nothing of its own.
const READER: &str = "01990000-0000-7000-8000-0000000000e1";
/// The key both writers share.
const KEY: &str = "deploy_target";
/// The one instant both writes land at.
const AT: i64 = 1_760_000_000_000;

fn id(text: &str) -> Uuid7 {
    Uuid7::parse(text).expect("a fixture identifier is a v7 spelling")
}

/// The boundary a walk resumes from after `row`.
fn after(row: &Record) -> After<'_> {
    After {
        created_at_ms: row.created_at_ms,
        key: &row.key,
        fleet: &row.fleet,
    }
}

#[tokio::test]
async fn a_walk_past_two_writers_tied_on_instant_and_key_skips_neither() {
    let store = InMemory::new("page");
    let workspace = id(WORKSPACE);
    let shared = MemoryDelta {
        key: Cow::Borrowed(KEY),
        content: Cow::Borrowed("iad"),
        category: Cow::Borrowed(PINNED_CATEGORY),
        visibility: Visibility::Workspace,
    };
    for writer in WRITERS {
        let fleet = id(writer);
        let owner = Owner {
            workspace: &workspace,
            fleet: &fleet,
        };
        store
            .upsert(owner, &[&shared], UnixMillis::from_millis(AT))
            .await
            .expect("a publish");
    }
    let reader = id(READER);
    let owner = Owner {
        workspace: &workspace,
        fleet: &reader,
    };

    let mut walked: Vec<Record> = Vec::new();
    // One page per writer, and one more that must come back empty.
    for _page in 0..=WRITERS.len() {
        let boundary = walked.last().map(after);
        let page = store
            .page(owner, true, View::Recent, boundary, 1)
            .await
            .expect("a page");
        walked.extend(page);
    }

    let writers: Vec<_> = walked.iter().map(|row| row.fleet.as_str()).collect();
    assert_eq!(
        writers,
        [WRITERS[1], WRITERS[0]],
        "newest first, the writer breaking the tie, and neither skipped"
    );
}

/// The fleet a verb is asked for, and the other fleet in its workspace.
const OWN: &str = WRITERS[0];
const OTHER: &str = WRITERS[1];
/// The window's keys: two of the fleet's own, then the other fleet's share.
const OLDER: &str = "older";
const NEWER: &str = "newer";
const SHARED: &str = "shared";
/// The recall's query, a key holding it, and a share holding it.
const DEPLOY: &str = "deploy";
const NOTES: &str = "notes";
const DEPLOY_WINDOW: &str = "deploy_window";
/// What `KEY` holds.
const REGION: &str = "iad";
/// The two filed entries the pages pick between.
const ALPHA: &str = "alpha";
const BETA: &str = "beta";

/// A delta under `key` holding `content` in `category`.
fn note(
    key: &'static str,
    content: &'static str,
    category: &'static str,
    visibility: Visibility,
) -> MemoryDelta<'static> {
    MemoryDelta {
        key: Cow::Borrowed(key),
        content: Cow::Borrowed(content),
        category: Cow::Borrowed(category),
        visibility,
    }
}

/// Writes `deltas` as `fleet` in the fixture workspace, `offset` ms after `AT`.
async fn write(store: &InMemory, fleet: &str, deltas: &[MemoryDelta<'_>], offset: i64) {
    let (workspace, fleet) = (id(WORKSPACE), id(fleet));
    let owner = Owner {
        workspace: &workspace,
        fleet: &fleet,
    };
    let refs: Vec<_> = deltas.iter().collect();
    store
        .upsert(owner, &refs, UnixMillis::from_millis(AT + offset))
        .await
        .expect("the in-memory store takes a write");
}

fn keys(rows: &[Record]) -> Vec<&str> {
    rows.iter().map(|row| row.key.as_str()).collect()
}

/// `OWN` in the fixture workspace, for a verb that reads or forgets.
fn own_owner<'a>(workspace: &'a Uuid7, own: &'a Uuid7) -> Owner<'a> {
    Owner {
        workspace,
        fleet: own,
    }
}

/// What `OWN`'s window holds, by key.
async fn windowed(store: &InMemory, reads: bool) -> Vec<String> {
    let (workspace, own) = (id(WORKSPACE), id(OWN));
    let rows = store.window(own_owner(&workspace, &own), reads).await;
    rows.expect("a window")
        .into_iter()
        .map(|row| row.key)
        .collect()
}

#[tokio::test]
async fn a_window_holds_own_entries_newest_first_then_shared_ones_only_for_a_reader() {
    let store = InMemory::new("window");
    let core = PINNED_CATEGORY;
    write(&store, OWN, &[note(OLDER, "a", core, Visibility::Fleet)], 0).await;
    write(&store, OWN, &[note(NEWER, "b", core, Visibility::Fleet)], 1).await;
    let theirs = [
        note(SHARED, "c", core, Visibility::Workspace),
        note("private", "d", core, Visibility::Fleet),
    ];
    write(&store, OTHER, &theirs, 0).await;

    let alone = windowed(&store, false).await;
    let reading = windowed(&store, true).await;

    assert_eq!(alone, [NEWER, OLDER], "own entries, newest first");
    assert_eq!(
        reading,
        [NEWER, OLDER, SHARED],
        "then the other fleet's shared entry, never its private one"
    );
}

/// What a recall for `OWN` finds, by key.
async fn recalled(store: &InMemory, reads: bool, query: &str, limit: usize) -> Vec<String> {
    let (workspace, own) = (id(WORKSPACE), id(OWN));
    let rows = store
        .search(own_owner(&workspace, &own), reads, query, limit)
        .await;
    rows.expect("a recall")
        .into_iter()
        .map(|row| row.key)
        .collect()
}

#[tokio::test]
async fn a_recall_ranks_key_matches_over_newer_content_matches_ignoring_case() {
    let store = InMemory::new("recall");
    let core = PINNED_CATEGORY;
    let own = [
        note(KEY, REGION, core, Visibility::Fleet),
        note("lunch", "tacos", core, Visibility::Fleet),
    ];
    write(&store, OWN, &own, 0).await;
    let content_match = note(NOTES, "we DEPLOY on fridays", core, Visibility::Fleet);
    write(&store, OWN, &[content_match], 1).await;
    let theirs = [
        note(DEPLOY_WINDOW, "nightly", core, Visibility::Workspace),
        note("deploy_secret", "hush", core, Visibility::Fleet),
    ];
    write(&store, OTHER, &theirs, 0).await;

    let found = recalled(&store, false, "Deploy", 5).await;
    let bounded = recalled(&store, false, DEPLOY, 1).await;
    let reading = recalled(&store, true, DEPLOY, 5).await;

    assert_eq!(
        found,
        [KEY, NOTES],
        "the key match outranks the newer content match; `lunch` holds neither"
    );
    assert_eq!(bounded, [KEY], "at most `limit` of the fleet's own");
    assert_eq!(
        reading,
        [KEY, NOTES, DEPLOY_WINDOW],
        "then the other fleet's shared match, never its private one"
    );
}

#[tokio::test]
async fn a_category_page_and_a_search_page_hold_only_the_entries_they_name() {
    let store = InMemory::new("views");
    let filed = [
        note(ALPHA, "first light", PINNED_CATEGORY, Visibility::Fleet),
        note(BETA, "second wind", DAILY_CATEGORY, Visibility::Fleet),
    ];
    write(&store, OWN, &filed, 0).await;
    let (workspace, own) = (id(WORKSPACE), id(OWN));
    let owner = own_owner(&workspace, &own);

    let daily = store
        .page(owner, false, View::Category(DAILY_CATEGORY), None, 10)
        .await
        .expect("a category page");
    let searched = store
        .page(owner, false, View::Search("LIGHT"), None, 10)
        .await
        .expect("a search page");

    assert_eq!(keys(&daily), [BETA], "only the named category");
    assert_eq!(keys(&searched), [ALPHA], "only the content match, any case");
}

#[tokio::test]
async fn a_forget_removes_only_the_owners_entry_and_answers_whether_it_held_one() {
    let store = InMemory::new("forget");
    let shared = note(KEY, REGION, PINNED_CATEGORY, Visibility::Workspace);
    for writer in WRITERS {
        write(&store, writer, std::slice::from_ref(&shared), 0).await;
    }
    let (workspace, own) = (id(WORKSPACE), id(OWN));
    let owner = own_owner(&workspace, &own);

    let [held, again] = [
        store.forget(owner, KEY).await,
        store.forget(owner, KEY).await,
    ]
    .map(|forgot| forgot.expect("a forget"));
    let left = store.export(&workspace).await.expect("an export");

    assert!(held, "the owner held the key");
    assert!(!again, "and holds it no longer");
    let writers: Vec<_> = left.iter().map(|row| row.fleet.as_str()).collect();
    assert_eq!(
        writers,
        [OTHER],
        "the other fleet's entry under the key stays"
    );
}

#[tokio::test]
async fn a_stale_forget_spares_a_newer_row_and_takes_one_at_or_under_its_bound() {
    let store = InMemory::new("stale");
    let filed = [
        note(ALPHA, REGION, PINNED_CATEGORY, Visibility::Fleet),
        note(BETA, REGION, PINNED_CATEGORY, Visibility::Fleet),
    ];
    write(&store, OWN, &filed, 0).await;
    write(&store, OTHER, &filed, 0).await;
    let (workspace, own) = (id(WORKSPACE), id(OWN));
    let owner = own_owner(&workspace, &own);

    let removed = [
        store.forget_stale(owner, ALPHA, AT - 1).await,
        store.forget_stale(owner, ALPHA, AT).await,
        store.forget_stale(owner, BETA, AT + 1).await,
    ]
    .map(|answer| answer.expect("a stale forget"));

    assert_eq!(removed, [false, true, true], "newer stays; at and under go");
    let left = store.export(&workspace).await.expect("an export");
    let writers: Vec<_> = left.iter().map(|row| row.fleet.as_str()).collect();
    assert_eq!(writers, [OTHER, OTHER], "the other fleet's rows stay");
}
