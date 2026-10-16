//! Dimension 2.1 — every declared family is fed.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use std::collections::BTreeSet;

use opentelemetry::metrics::MeterProvider as _;
use opentelemetry_sdk::metrics::SdkMeterProvider;

use super::{GaugeSources, Producers};
use crate::metrics::instrument::Instruments;
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

/// Every family the census declares is claimed by a producer.
///
/// # What this proves, and what it does not
///
/// It proves a producer EXISTS for every family and that the claim runs —
/// which is the failure that actually happens, because a family is added to
/// the census in one commit and its producer is meant to arrive in another.
/// It does not prove the producer ever fires; that belongs to each producing
/// path's own test, and no single test could stand in for all of them.
///
/// A family whose mechanism this daemon does not run has no row in the
/// census, so there is no excused set: every unclaimed family is a failure.
#[test]
fn every_census_family_has_a_producer() {
    let instruments = instruments();
    let _producers = Producers::claim(&instruments, &GaugeSources::silent())
        .expect("every producer names a family the census declares");

    let unfed: BTreeSet<String> = instruments
        .unclaimed()
        .into_iter()
        .map(|family| family.to_string())
        .collect();
    assert!(
        unfed.is_empty(),
        "the census declares families nothing produces: {unfed:?}"
    );
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

/// The gauge sources render without unfolding the closures they hold.
///
/// `GaugeSources` is carried through boot and sits in `Debug` structs above it,
/// so a `{:?}` somebody adds later reaches it. What it holds are closures that
/// READ live process state — an in-flight request count, a stream count — and
/// neither has a printable value that would still be true by the time anyone
/// read the line. So the rendering names the type and stops, and the
/// `finish_non_exhaustive` ellipsis is what says the stop is deliberate.
#[test]
fn the_gauge_sources_render_without_unfolding_their_readers() {
    let rendered = format!("{:?}", GaugeSources::silent());

    assert!(
        rendered.starts_with("GaugeSources"),
        "the rendering names the type: {rendered}"
    );
    assert!(
        rendered.contains(".."),
        "and marks itself non-exhaustive rather than looking complete: {rendered}"
    );
}
