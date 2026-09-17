//! What the census pass decides without a table: which spellings it counts,
//! and what a failed count leaves behind.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use afd_observability::metrics::label::fleet::FleetStatusLabel;
use afd_observability::producers::fleet::census::fleet_census_readings;

use super::{Counted, Tally, settle, tally};
use crate::error::query;

/// A grouped-count row.
fn counted(status: &str, fleets: u64) -> Counted {
    Counted {
        status: status.to_owned(),
        fleets,
    }
}

#[test]
fn every_modelled_spelling_is_counted_under_its_member() {
    let tally = tally([
        counted("active", 2),
        counted("paused", 1),
        counted("stopped", 1),
    ]);
    assert_eq!(
        tally.counts,
        vec![
            (FleetStatusLabel::Active, 2),
            (FleetStatusLabel::Paused, 1),
            (FleetStatusLabel::Stopped, 1),
        ]
    );
    assert!(tally.unmodelled.is_empty());
    assert_eq!(tally.scanned(), 4);
}

#[test]
fn an_unknown_spelling_is_set_aside_and_still_scanned() {
    let tally = tally([counted("active", 3), counted("archived", 2)]);
    assert_eq!(
        tally.counts,
        vec![(FleetStatusLabel::Active, 3)],
        "the unknown spelling produces no label"
    );
    assert_eq!(tally.unmodelled, vec![counted("archived", 2)]);
    assert_eq!(
        tally.scanned(),
        5,
        "rows the pass could not model were still rows it saw"
    );
}

#[test]
fn a_case_or_whitespace_variant_is_not_a_known_status() {
    let tally = tally([counted("Active", 1), counted(" active", 1), counted("", 1)]);
    assert!(tally.counts.is_empty(), "spellings are byte-exact");
    assert_eq!(tally.unmodelled.len(), 3);
}

#[test]
fn an_empty_table_tallies_to_nothing_and_scans_nothing() {
    assert_eq!(tally([]), Tally::default());
    assert_eq!(Tally::default().scanned(), 0);
}

/// A failed count withdraws every cell, and a later good count restores them.
///
/// The process-wide cells are shared with every test in this binary, and this
/// is the only one that touches them, which is what makes the two reads
/// below assertable.
#[test]
fn a_failed_count_withdraws_the_census_and_a_good_one_restores_it() {
    let failed = settle(Err(query("fleet census count")(sqlx::Error::PoolClosed)));
    assert!(failed.is_err(), "the failure is reported, not swallowed");
    assert!(
        fleet_census_readings().is_empty(),
        "a failed count leaves a gap, never a zero"
    );

    let swept = settle(Ok(vec![counted("killed", 4), counted("archived", 1)]))
        .expect("a count that answered is published");
    assert_eq!(swept.scanned, 5);
    assert_eq!(swept.changed, 0, "a census changes nothing");
    let readings = fleet_census_readings();
    assert_eq!(
        readings.len(),
        FleetStatusLabel::ALL.len(),
        "every modelled status publishes, the unknown one does not"
    );
    let killed = readings
        .iter()
        .find(|reading| {
            reading
                .attributes
                .iter()
                .any(|attribute| attribute.value.as_str() == "killed")
        })
        .map(|reading| reading.value);
    assert_eq!(killed, Some(4));
}
