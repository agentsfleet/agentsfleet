//! The gallery's order where the creation instant cannot decide it.
//!
//! The merge-and-seek proof next door seeds four distinct instants, so the
//! order's second and third keys — the tier rank and the bytewise id — never
//! decide a boundary there. Here every row shares one instant, so each page
//! boundary is resolved by the tier or by the id, and a seek arm written in
//! the wrong direction skips or repeats rows at exactly these boundaries.

use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_library::{Libraries, Position, SummaryEntry, Tier};

use super::gallery_seed::{
    DRAFT, PUBLISHED, parse, seed_platform, seed_platform_as, seed_scope, seed_workspace,
};

/// The one instant every row in this proof shares.
///
/// Small, so the rows sit at the old end of a gallery the lane shares with
/// other tests and never crowd another test's first page.
const TIED_INSTANT: i64 = 5_000;

/// One card per page, so every adjacent pair of rows is a page boundary.
const ONE_CARD: u32 = 1;

/// A deadlock guard: a seek that failed to advance would otherwise spin.
const MAX_PAGES: usize = 500;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn the_gallery_orders_a_tied_instant_by_tier_across_pages() {
    let lane = TestDatabase::shared();
    let database = lane.open(DbRole::Api, &[]).await;
    let workspace = mint_id();
    seed_scope(&database, &workspace).await;
    let suffix = mint_id().replace('-', "");

    let mut platform = vec![format!("tie-a-{suffix}"), format!("tie-b-{suffix}")];
    for id in &platform {
        seed_platform(&database, id, TIED_INSTANT).await;
    }
    let mut tenant = Vec::new();
    for name in [format!("tie-c-{suffix}"), format!("tie-d-{suffix}")] {
        tenant.push(seed_workspace(&database, &workspace, &name, TIED_INSTANT).await);
    }
    // Two platform rows a tenant cannot install, one failing each half of the
    // arm's filter. Neither may surface on any page.
    seed_platform_as(
        &database,
        &format!("tie-draft-{suffix}"),
        TIED_INSTANT,
        DRAFT,
        Some("draft-hash"),
    )
    .await;
    seed_platform_as(
        &database,
        &format!("tie-empty-{suffix}"),
        TIED_INSTANT,
        PUBLISHED,
        None,
    )
    .await;

    // Platform before tenant within the instant, then ids descending bytewise.
    platform.sort_unstable_by(|a, b| b.cmp(a));
    tenant.sort_unstable_by(|a, b| b.cmp(a));
    let expected: Vec<String> = platform.into_iter().chain(tenant).collect();

    let walked = walk(&Libraries::new(database.clone()), &workspace, &suffix).await;
    let ids: Vec<&str> = walked.iter().map(|card| card.id.as_str()).collect();
    assert_eq!(
        ids, expected,
        "one card per page walks platform before tenant, then each tier by id, once each"
    );
    let tiers: Vec<Tier> = walked.iter().map(|card| card.tier).collect();
    assert_eq!(
        tiers,
        vec![Tier::Platform, Tier::Platform, Tier::Tenant, Tier::Tenant],
        "the tier each card reports is the library it came from"
    );

    drop(database);
    lane.cleanup().await;
}

/// This proof's cards, one per page, from just above the tied instant down.
///
/// The walk opens at a boundary rather than the first page because the lane's
/// gallery carries every other test's platform rows ahead of these; it stops
/// once the pages fall below the instant, which nothing of this proof's sits
/// under.
async fn walk(libraries: &Libraries, workspace: &str, suffix: &str) -> Vec<SummaryEntry> {
    let mut after = Some(Position {
        created_at_ms: TIED_INSTANT + 1,
        tier: Tier::Platform,
        id: String::new(),
    });
    let mut walked = Vec::new();
    for _page in 0..MAX_PAGES {
        let Some(boundary) = after.take() else {
            break;
        };
        let page = libraries
            .gallery(&parse(workspace), ONE_CARD, Some(&boundary))
            .await
            .expect("the gallery reads");
        let Some(card) = page.items.into_iter().next() else {
            break;
        };
        if card.created_at_ms < TIED_INSTANT {
            break;
        }
        if card.name.ends_with(suffix) {
            walked.push(card);
        }
        after = page.next;
    }
    walked
}
