//! A Library page is one statement at any page size: every card arrives joined
//! to its head, and a keyset walk stays coherent when a card goes mid-walk
//! (spec Dimension 7.2).

use super::Case;
use afd_authoring::types::{LibraryCard, LibraryQuery, Page};
use afd_core::test_util::trace::Capture;
use std::collections::BTreeSet;

/// The tables a per-card follow-up read would name.
const LIBRARY_TABLES: &str = "authoring.library_";
/// The page sizes the walk is read at: the smallest, and a full page.
const PAGE_SIZES: [u32; 2] = [1, 25];
/// Cards published: one full page, then two more for a second page to hold.
const CARDS: usize = 27;
/// The field only sqlx's per-statement event carries.
const SQLX_STATEMENT: &str = "db.statement";

/// One owned page at `limit` after `cursor`, with the statements that named a
/// Library table while it was read.
async fn page(case: &Case, limit: u32, cursor: Option<String>) -> (Page<LibraryCard>, usize) {
    let capture = Capture::install();
    let page = case
        .setup
        .libraries(
            case.base.access(),
            LibraryQuery {
                limit,
                starting_after: cursor,
            },
            true,
        )
        .await
        .expect("owned Library page");
    let statements = capture
        .events()
        .iter()
        .filter(|line| {
            line.fields.contains_key(SQLX_STATEMENT)
                && line
                    .fields
                    .values()
                    .any(|value| value.contains(LIBRARY_TABLES))
        })
        .count();
    (page, statements)
}

/// The ids on `page`, in order.
fn ids(page: &Page<LibraryCard>) -> Vec<String> {
    page.items.iter().map(|card| card.id.clone()).collect()
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_library_list_single_statement() {
    let mut case = Case::create().await;
    let mut published = BTreeSet::new();
    for _ in 0..CARDS {
        let draft = case.draft().await;
        published.insert(case.publish(&draft).await.library_id);
    }
    for limit in PAGE_SIZES {
        let (first, statements) = page(&case, limit, None).await;
        assert_eq!(
            first.items.len(),
            limit as usize,
            "a page of {limit} is full"
        );
        assert_eq!(
            statements, 1,
            "a page of {limit} and every card's head revision are one statement"
        );
        assert!(
            first.next_cursor.is_some(),
            "more cards follow a page of {limit}"
        );
    }

    // A full page, then its last card (the cursor's anchor) and one card of
    // the next page go before the next page is read.
    let full = PAGE_SIZES[1];
    let (first, _) = page(&case, full, None).await;
    let seen = ids(&first);
    let anchor = seen.last().expect("a full page has a last card").clone();
    let rest: BTreeSet<String> = published
        .iter()
        .filter(|id| !seen.contains(id))
        .cloned()
        .collect();
    let next = rest
        .iter()
        .next()
        .expect("a second page holds cards")
        .clone();
    for gone in [&anchor, &next] {
        case.setup
            .delete_library(case.base.access(), gone)
            .await
            .expect("the owner deletes a card mid-walk");
    }
    let (second, statements) = page(&case, full, first.next_cursor.clone()).await;
    let expected: Vec<String> = rest.into_iter().filter(|id| *id != next).collect();
    let mut listed = ids(&second);
    listed.sort();
    assert_eq!(
        listed, expected,
        "the next page lists the cards still there after the cursor, and no deleted one"
    );
    assert!(second.next_cursor.is_none(), "the walk ends");
    assert_eq!(
        statements, 1,
        "a page after a deletion is still one statement"
    );
    case.base.cleanup().await;
}
