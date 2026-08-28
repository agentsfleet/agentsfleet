//! Dimension 2.3 — the keyset walks a crafted corpus without skipping or
//! repeating a row.
//!
//! The paging VOCABULARY — how a `?sort=`/`?cursor=`/`?limit=` query becomes a
//! [`Page`], and which comparator each sort order implies — is decided in
//! `afd_core::paging` and proven there without a datastore. What no unit test
//! can prove is that the statement those choices assemble actually orders rows
//! that way in Postgres, because the ordering is the DATABASE's behaviour and
//! not this crate's.
//!
//! # Why the tie is the case that matters
//!
//! A keyset seeks on `(sort_value, id) > (cursor_value, cursor_id)`. Get the
//! tuple wrong — compare `created_at` alone — and two keys minted in the same
//! millisecond become indistinguishable to the cursor: the second page either
//! repeats the boundary row or steps over its sibling. Neither is visible on a
//! corpus where every timestamp differs, which is exactly the corpus a fixture
//! writes by accident. So this one is crafted to collide on purpose.
//!
//! Marked `#[ignore]` so `make test-unit-all` still COMPILES and lints it
//! without a datastore, and `make test-integration-rustd` — which runs
//! `--ignored` and nothing else — is the only lane that executes it.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::paging::Cursor;
use afd_tenant::apikey::{ApiKeySort, KeyRow};

/// How far apart the corpus spaces two keys that must NOT tie.
///
/// One second, which is only a readable gap — nothing depends on its size.
/// What the suite depends on is the pair that shares an instant exactly, and
/// naming this makes that pair legible as the deliberate exception it is.
const STEP_MS: i64 = 1_000;

#[path = "support/apikey_lane.rs"]
mod support;

use self::support::{ApiKeyLane, PAGE_SIZE};

/// Dimension 2.3 — a page boundary lands exactly between two rows, ties
/// included, and the total does not move while a client walks.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_list_keyset_pagination() {
    let lane = ApiKeyLane::create().await;

    // Five keys over four instants: the middle PAIR shares one millisecond, so
    // the tiebreak is exercised rather than assumed. Minted oldest-first, which
    // is the opposite of the order the default sort answers in — a listing that
    // simply echoed insertion order would pass a same-order fixture.
    let base = lane.instant();
    lane.mint_key("alpha", base).await;
    lane.mint_key("bravo", base + STEP_MS).await;
    lane.mint_key("charlie", base + 2 * STEP_MS).await;
    // Same instant as `charlie`, on purpose: this is the tie.
    lane.mint_key("delta", base + 2 * STEP_MS).await;
    lane.mint_key("echo", base + 3 * STEP_MS).await;

    let first = lane.page(ApiKeySort::CreatedDescending, None).await;
    assert_eq!(
        first.keys.len(),
        PAGE_SIZE as usize,
        "a full page answers exactly the limit"
    );
    assert!(
        is_newest_first(&first.keys),
        "the default sort answers newest first: {:?}",
        names(&first.keys)
    );
    assert_eq!(first.total, 5, "the total counts every key, not this page");

    // The boundary is the LAST row of the page, which is the only row a client
    // can build a cursor from — it holds no others by then.
    let boundary = first.keys.last().expect("a full page has a last row");
    let cursor = Cursor::Timestamp {
        at_ms: boundary.created_at_ms,
        id: boundary.id.clone(),
    };

    let second = lane.page(ApiKeySort::CreatedDescending, Some(cursor)).await;

    assert_eq!(
        second.total, first.total,
        "the total is page-stable: the count subquery carries no keyset \
         predicate, so a client walking pages sees one number and not a \
         shrinking one"
    );

    // The two claims the keyset exists to make, stated over the rows rather
    // than over their count: nothing appears twice, and nothing is missing.
    let walked: Vec<String> = first
        .keys
        .iter()
        .chain(second.keys.iter())
        .map(|key| key.name.clone())
        .collect();
    let mut unique = walked.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(
        unique.len(),
        walked.len(),
        "a row must not appear on two pages: {walked:?}"
    );
    assert_eq!(
        unique.len(),
        5,
        "every key must appear on some page — a skipped row is what a \
         created_at-only comparator loses at the tie: {walked:?}"
    );
    assert!(
        is_newest_first(&second.keys),
        "the second page holds the order the first one set: {:?}",
        names(&second.keys)
    );

    // And the order across the seam, which neither page can assert alone.
    assert!(
        boundary.created_at_ms
            >= second
                .keys
                .first()
                .expect("the second page carries the remainder")
                .created_at_ms,
        "the page after the boundary must not contain a row NEWER than it"
    );

    lane.cleanup().await;
}

/// Dimension 2.3 — the name sort walks on the text comparator, not the clock.
///
/// The sibling case, and it is not redundant: `Cursor::Text` binds a different
/// slot in the statement than `Cursor::Timestamp`, so a keyset that worked on
/// instants can still be wrong on names. The corpus is deliberately ordered so
/// alphabetical and chronological DISAGREE — minted newest-first by name — and
/// a listing that fell back to the clock answers in the wrong order rather than
/// merely in a different one.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_list_keyset_pagination_by_name() {
    let lane = ApiKeyLane::create().await;

    let base = lane.instant();
    lane.mint_key("zulu", base).await;
    lane.mint_key("yankee", base + STEP_MS).await;
    lane.mint_key("xray", base + 2 * STEP_MS).await;
    lane.mint_key("whiskey", base + 3 * STEP_MS).await;

    let first = lane.page(ApiKeySort::NameAscending, None).await;
    assert_eq!(
        names(&first.keys),
        vec!["whiskey", "xray", "yankee"],
        "A to Z, which is the reverse of the order these were minted in"
    );

    let boundary = first.keys.last().expect("a full page has a last row");
    let second = lane
        .page(
            ApiKeySort::NameAscending,
            Some(Cursor::Text {
                value: boundary.name.clone(),
                id: boundary.id.clone(),
            }),
        )
        .await;

    assert_eq!(
        names(&second.keys),
        vec!["zulu"],
        "the remainder resumes after the boundary NAME, without repeating it"
    );

    lane.cleanup().await;
}

/// The names on a page, in the order the page answered them.
fn names(keys: &[KeyRow]) -> Vec<String> {
    keys.iter().map(|key| key.name.clone()).collect()
}

/// Whether every row is at least as old as the one before it.
///
/// `>=` rather than `>`: the corpus deliberately holds a tie, and a tie is
/// ordered rather than out of order.
fn is_newest_first(keys: &[KeyRow]) -> bool {
    keys.windows(2)
        .all(|pair| pair[0].created_at_ms >= pair[1].created_at_ms)
}
