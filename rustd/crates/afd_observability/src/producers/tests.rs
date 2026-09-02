//! Dimension 2.1 — every declared family is fed, or is excused by name.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use std::collections::BTreeSet;

use opentelemetry::metrics::MeterProvider as _;
use opentelemetry_sdk::metrics::SdkMeterProvider;

use super::{GaugeSources, Producers};
use crate::metrics::instrument::Instruments;
use crate::metrics::produced::UNPRODUCED;
use crate::metrics::registry::Registry;
use crate::semconv;

/// An instrument set over a provider that exports nowhere.
fn instruments() -> Instruments {
    let provider = SdkMeterProvider::builder().build();
    Instruments::new(
        Registry::declared().expect("the compiled-in census reads"),
        provider.meter(semconv::SCOPE_NAME),
        provider.meter(semconv::SCOPE_NAME),
    )
}

/// Every family the contract declares is claimed by a producer, or excused.
///
/// # What this proves, and what it does not
///
/// It proves a producer EXISTS for every family and that the claim runs —
/// which is the failure that actually happens, because a family is added to
/// the census in one commit and its producer is meant to arrive in another.
/// It does not prove the producer ever fires; that belongs to each producing
/// path's own test, and no single test could stand in for all of them.
///
/// The excused set is read from the ledger rather than hard-coded here, so the
/// two cannot drift: a family that stops being excused has to gain a producer
/// on the same commit or this fails.
#[test]
fn every_census_family_has_a_producer() {
    let instruments = instruments();
    let _producers = Producers::claim(&instruments, &GaugeSources::silent())
        .expect("every producer names a family the census declares");

    let unclaimed: BTreeSet<String> = instruments
        .unclaimed()
        .into_iter()
        .map(|family| family.to_string())
        .collect();
    let excused: BTreeSet<String> = UNPRODUCED.iter().map(|row| row.family.to_owned()).collect();

    let unfed: Vec<&String> = unclaimed.difference(&excused).collect();
    assert!(
        unfed.is_empty(),
        "the census declares families nothing produces and nothing excuses: {unfed:?}"
    );

    let over_excused: Vec<&String> = excused.difference(&unclaimed).collect();
    assert!(
        over_excused.is_empty(),
        "the ledger excuses families that DO have a producer, so the excuse is \
         stale and hides a real one: {over_excused:?}"
    );
}

/// Every excused family is one the census still declares.
///
/// The other direction of the same rot: a family dropped from the contract
/// leaves an excuse behind that reads, to anybody grepping, like a gap this
/// daemon still has.
#[test]
fn every_excused_family_is_still_declared() {
    let registry = Registry::declared().expect("the compiled-in census reads");
    for row in UNPRODUCED {
        assert!(
            registry.family(row.family).is_ok(),
            "`{}` is excused from production and the census no longer declares it",
            row.family
        );
    }
}

/// Every excuse says something.
///
/// A blank reason is worse than no ledger: it passes the test above, prints an
/// empty sentence at boot, and tells a reader the absence was considered when
/// nobody wrote down what the consideration was.
#[test]
fn every_excuse_carries_a_reason() {
    for row in UNPRODUCED {
        assert!(
            row.why.len() > 20,
            "`{}` is excused with nothing that explains it: {:?}",
            row.family,
            row.why
        );
    }
}

/// Claiming twice over one set is not an error, and claims nothing new.
///
/// The property boot relies on: a second claim would mean a second set of
/// instruments under the same names, and the SDK would merge them into one
/// stream with no complaint at all.
#[test]
fn claiming_twice_adds_no_families() {
    let instruments = instruments();
    let sources = GaugeSources::silent();
    let _first = Producers::claim(&instruments, &sources).expect("the first claim succeeds");
    let after_first = instruments.unclaimed();
    let _second = Producers::claim(&instruments, &sources).expect("the second claim succeeds");

    assert_eq!(
        instruments.unclaimed(),
        after_first,
        "a repeated claim changed what is outstanding"
    );
}
