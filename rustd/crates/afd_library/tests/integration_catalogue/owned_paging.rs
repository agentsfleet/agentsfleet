//! Walking a workspace's own entries past the first page.
//!
//! # Why the second page needs its own statement, and its own proof
//!
//! `owned_entries` chooses between two statements: `FIRST_PAGE` when the
//! caller brought no cursor, and `PAGE_AFTER` when it did. Only the first was
//! ever executed by a test — every suite asked for one page of a collection
//! small enough to fit in it — so the seek predicate, the bind order it
//! depends on, and the boundary the walk resumes from were carried entirely by
//! review.
//!
//! That is the half of a keyset walk where the interesting mistakes live. A
//! `>` written where the order wants `<` returns the page the caller just
//! read; a tie-break comparing only the instant drops or repeats every row
//! sharing a millisecond; a transposed bind silently seeks on the wrong
//! column. Each of those still answers with rows, so nothing but a walk across
//! a real boundary tells them apart.
//!
//! The fixture therefore seeds one workspace with entries at DISTINCT instants
//! and one deliberate TIE, then walks the whole collection two at a time and
//! asserts the union is exactly what was seeded, in order, with nothing
//! repeated.

use afd_core::id::Uuid7;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_library::Libraries;

/// The page size the walk uses, small enough that five entries need three
/// pages.
const PAGE: u32 = 2;

/// The oldest fixture instant. The rest are offsets from it, so the expected
/// order is readable without arithmetic.
const BASE_AT: i64 = 7_000;

/// How many entries the fixture seeds.
const SEEDED: u32 = 5;

/// Dimension — the second page resumes after the first, and the walk is exact.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn the_walk_crosses_every_page_boundary_without_loss_or_repeat() {
    let lane = TestDatabase::shared();
    let database = lane.open(DbRole::Api, &[]).await;

    let workspace = mint_id();
    let seeded = seed_workspace(&database, &workspace).await;
    let workspace_id = Uuid7::parse(&workspace).expect("the seeded workspace id parses");
    let libraries = Libraries::new(database.clone());

    // The whole collection, gathered one page at a time through the cursor the
    // previous page handed back — which is the only way PAGE_AFTER runs.
    let mut walked: Vec<String> = Vec::new();
    let mut after = None;
    let mut pages: u32 = 0;
    loop {
        let page = libraries
            .owned_entries(&workspace_id, PAGE, after.as_ref())
            .await
            .expect("each page of the owned collection reads");
        pages += 1;
        assert!(
            u32::try_from(page.items.len()).is_ok_and(|served| served <= PAGE),
            "a page must never serve more than it was asked for"
        );
        walked.extend(page.items.iter().map(|entry| entry.id.clone()));
        match page.next {
            Some(position) => after = Some(position),
            None => break,
        }
        assert!(
            pages <= SEEDED + 1,
            "the walk must terminate; a cursor that does not advance loops here"
        );
    }

    // Newest first, which is the order the page is documented to serve and the
    // order the seek depends on.
    let expected: Vec<String> = seeded.iter().rev().cloned().collect();
    assert_eq!(
        walked, expected,
        "the walk must yield every seeded entry exactly once, newest first"
    );
    assert_eq!(pages, 3, "five entries at two a page is three reads");

    assert_boundary_is_strict(&libraries, &workspace_id, &expected).await;

    drop(database);
    lane.cleanup().await;
}

/// The seek excludes the row it resumes from, rather than serving it twice.
///
/// The walk above would still pass if the boundary were inclusive on a
/// collection whose page size divided it evenly — the repeat would land at a
/// page edge and the totals would survive. Resuming from the FIRST entry and
/// asking for everything after it names the property directly.
async fn assert_boundary_is_strict(libraries: &Libraries, workspace: &Uuid7, expected: &[String]) {
    let first = libraries
        .owned_entries(workspace, 1, None)
        .await
        .expect("the first entry reads");
    let boundary = first.next.expect("four entries remain behind the first");

    let rest = libraries
        .owned_entries(workspace, SEEDED, Some(&boundary))
        .await
        .expect("the remainder reads");
    let ids: Vec<String> = rest.items.iter().map(|entry| entry.id.clone()).collect();

    let behind = expected
        .get(1..)
        .expect("the fixture seeds more than one entry");
    let boundary_id = expected
        .first()
        .expect("the fixture seeds at least one entry");
    assert_eq!(
        ids, behind,
        "the seek must resume strictly after the entry it names"
    );
    assert!(
        !ids.contains(boundary_id),
        "the boundary entry must not be served again"
    );
    assert!(
        rest.next.is_none(),
        "a page that served the remainder has nothing behind it"
    );
}

/// A tenant, a workspace under it, and [`SEEDED`] entries in that workspace.
///
/// Returns the entry identifiers oldest first. Two of them share an instant on
/// purpose: a tie-break that compares only `created_at` drops or repeats one of
/// the pair the moment a page boundary falls between them.
async fn seed_workspace(database: &afd_db::Db, workspace: &str) -> Vec<String> {
    let mut connection = database.acquire().await.expect("an API connection");
    sqlx::query(
        "WITH tenant AS ( \
           INSERT INTO core.tenants (id, name, created_at, updated_at) \
           VALUES ($1::uuid, 'Owned paging fixture', 1, 1) \
           RETURNING id \
         ) \
         INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
         SELECT $2::uuid, id, $2, 'test', 1 FROM tenant",
    )
    .bind(mint_id())
    .bind(workspace)
    .execute(&mut *connection)
    .await
    .expect("the paging fixture's scope seeds");

    // Distinct instants, except the pair at index 2 and 3, which tie.
    let instants = [
        BASE_AT,
        BASE_AT + 10,
        BASE_AT + 20,
        BASE_AT + 20,
        BASE_AT + 30,
    ];
    let mut seeded: Vec<String> = Vec::new();
    for (ordinal, created_at) in instants.iter().enumerate() {
        let entry = mint_id();
        sqlx::query(
            "INSERT INTO core.tenant_fleet_library ( \
               id, workspace_id, name, description, source_kind, source_ref, visibility, \
               content_hash, skill_markdown, trigger_markdown, support_files_json, \
               requirements_json, created_at, updated_at) \
             VALUES ($1::uuid, $2::uuid, $3, 'paging fixture', 'github', 'main', \
               'tenant', $4, '# Fixture', NULL, '[]', \
               '{\"credentials\":[],\"tools\":[],\"network_hosts\":[],\"trigger_present\":false}', \
               $5, $5)",
        )
        .bind(&entry)
        .bind(workspace)
        .bind(format!("paging-fixture-{ordinal}"))
        .bind(format!("hash-{entry}"))
        .bind(created_at)
        .execute(&mut *connection)
        .await
        .expect("the paging fixture's entry seeds");
        seeded.push(entry);
    }

    // The tie pair shares an instant, so the identifier decides their order and
    // the expected sequence has to agree with the statement's tie-break.
    seeded
        .get_mut(2..4)
        .expect("the fixture seeds the tie pair at index 2 and 3")
        .sort();
    seeded
}
