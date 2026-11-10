//! The definition cache parses each revision once, evicts by weight, and
//! counts every lookup by outcome.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the restriction set is for the library"
)]

use std::cell::Cell;

use af_fleet_runtime::Definition;
use af_observability::test_util::Capture as Metrics;
use afd_db::test_util::{FIXTURE_SETUP_DEFINITION, mint_id};

use super::{Definitions, RevisionKey};

/// The lookup counter's wire name.
const LOOKUPS: &str = "agentsfleet_lease_cache_lookups_total";

/// The fixture setup with its instructions replaced, so two revisions differ.
fn document(instructions: &str) -> String {
    FIXTURE_SETUP_DEFINITION.replace("Run the fixture.", instructions)
}

/// Parses `document`, counting each call in `parses`.
fn counted(parses: &Cell<u32>) -> impl FnOnce(&str) -> crate::error::Result<Definition> + '_ {
    move |document| {
        parses.set(parses.get() + 1);
        Ok(Definition::parse(document)?)
    }
}

/// One document's worth of capacity: fleets A, B, A parse three times, and
/// each lookup answers its own revision's definition.
#[tokio::test]
async fn definition_cache_evicts_and_reads_again() {
    let (first, second) = (document("Run fleet A."), document("Run fleet B."));
    let cap = u64::try_from(first.len().max(second.len())).expect("a document's size fits");
    let cache = Definitions::with_capacity(cap);
    let (a, b) = (
        RevisionKey::observed(mint_id()),
        RevisionKey::observed(mint_id()),
    );
    let parses = Cell::new(0);

    let read_a = cache
        .get_or_parse(&a, &first, counted(&parses))
        .await
        .expect("revision A parses");
    cache.settle().await;
    let read_b = cache
        .get_or_parse(&b, &second, counted(&parses))
        .await
        .expect("revision B parses");
    cache.settle().await;
    let again_a = cache
        .get_or_parse(&a, &first, counted(&parses))
        .await
        .expect("revision A parses again");

    assert_eq!(parses.get(), 3, "B evicted A, so A was parsed again");
    assert_eq!(read_a.instructions_md(), "Run fleet A.");
    assert_eq!(read_b.instructions_md(), "Run fleet B.");
    assert_eq!(again_a.instructions_md(), "Run fleet A.");
}

/// A held revision answers from memory: the second lookup parses nothing.
#[tokio::test]
async fn definition_cache_parses_a_held_revision_once() {
    let cache = Definitions::default();
    let key = RevisionKey::observed(mint_id());
    let parses = Cell::new(0);
    let text = document("Run fleet A.");
    for _ in 0..2 {
        cache
            .get_or_parse(&key, &text, counted(&parses))
            .await
            .expect("the revision parses");
    }
    assert_eq!(parses.get(), 1);
}

/// A document the parser refuses is not cached: the next lookup tries again.
#[tokio::test]
async fn definition_cache_keeps_no_refused_document() {
    let cache = Definitions::default();
    let key = RevisionKey::observed(mint_id());
    let parses = Cell::new(0);
    for _ in 0..2 {
        let refused = cache
            .get_or_parse(&key, "{\"schema_version\":1}", counted(&parses))
            .await;
        assert!(refused.is_err(), "an incomplete setup is refused");
    }
    assert_eq!(parses.get(), 2);
}

/// A miss then a hit on one key moves `{definition, miss}` and
/// `{definition, hit}` by one each, and no other series.
#[tokio::test]
async fn cache_lookups_counted_by_outcome() {
    let _counted = crate::lease::test_dead::CACHE_LOOKUPS.lock().await;
    let metrics = Metrics::install();
    let series = [
        ("definition", "hit"),
        ("definition", "miss"),
        ("price_book", "hit"),
        ("price_book", "miss"),
        ("platform_key", "hit"),
        ("platform_key", "miss"),
    ];
    let read = || {
        series
            .map(|(cache, outcome)| metrics.sum(LOOKUPS, &[("cache", cache), ("outcome", outcome)]))
    };
    let before = read();

    let cache = Definitions::default();
    let key = RevisionKey::observed(mint_id());
    let text = document("Run fleet A.");
    for _ in 0..2 {
        cache
            .get_or_parse(&key, &text, |document| Ok(Definition::parse(document)?))
            .await
            .expect("the revision parses");
    }

    let moved: Vec<u64> = read()
        .iter()
        .zip(before)
        .map(|(after, before)| after - before)
        .collect();
    assert_eq!(moved, [1, 1, 0, 0, 0, 0], "{series:?}");
}
