//! The instrument layer refuses what the contract and the caller disagree on.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use opentelemetry::metrics::MeterProvider as _;
use opentelemetry_sdk::metrics::SdkMeterProvider;

use super::{Instruments, series_ceilings};
use crate::error::Error;
use crate::metrics::declared::{fleet, http};
use crate::metrics::family::{CounterKind, Declared, GaugeKind, HistogramKind};
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

/// A family nothing declares is refused by name.
#[test]
fn an_undeclared_family_cannot_be_claimed() {
    let instruments = instruments();
    let invented: Declared<CounterKind> = Declared::new("agentsfleet_invented_total");

    let refusal = instruments
        .counter_u64(&invented)
        .expect_err("the census declares no such family");
    assert!(
        matches!(refusal, Error::UnknownFamily { ref family } if &**family == "agentsfleet_invented_total"),
        "the refusal must name the family: {refusal}"
    );
}

/// A family claimed as the wrong KIND is refused.
///
/// The compiler settles this for a caller naming a declared constant — a
/// `Declared<GaugeKind>` cannot be passed where a counter is wanted. What this
/// covers is the other half: a constant declared with the wrong marker, which
/// compiles everywhere and would export a point-in-time reading as a total.
#[test]
fn a_family_claimed_as_the_wrong_kind_is_refused() {
    let instruments = instruments();
    let miscast: Declared<CounterKind> = Declared::new(http::API_IN_FLIGHT_REQUESTS.wire_name());

    let refusal = instruments
        .counter_u64(&miscast)
        .expect_err("the census declares this a gauge");
    assert!(
        matches!(refusal, Error::KindMismatch { .. }),
        "a gauge claimed as a counter must be refused: {refusal}"
    );
}

/// A family claimed in the wrong NUMBER is refused.
///
/// Distinct from the kind check and worth its own arm: both sides agree it is
/// a counter and disagree about what it counts in, which exports whole counts
/// as a floating-point series — a graph that looks entirely reasonable.
#[test]
fn a_family_claimed_in_the_wrong_number_is_refused() {
    let instruments = instruments();

    let refusal = instruments
        .counter_f64(&fleet::SIGNUP_BOOTSTRAPPED_TOTAL)
        .expect_err("the census declares this counts in u64");
    assert!(
        matches!(refusal, Error::NumberMismatch { .. }),
        "a u64 family claimed as f64 must be refused: {refusal}"
    );
}

/// A histogram takes its bucket bounds from the contract, not the call site.
#[test]
fn a_histogram_is_built_and_claimed() {
    let instruments = instruments();
    let outstanding = instruments.unclaimed().len();

    let _histogram = instruments
        .histogram_f64(&fleet::REPAIR_PRODUCTION_TO_QUEUE_SECONDS)
        .expect("the census declares this a histogram in f64");

    assert_eq!(
        instruments.unclaimed().len(),
        outstanding - 1,
        "claiming an instrument records the claim"
    );
}

/// A gauge is registered and claimed, and its callback is never called here.
#[test]
fn a_gauge_is_registered_and_claimed() {
    let instruments = instruments();
    let outstanding = instruments.unclaimed().len();

    instruments
        .gauge_u64(&http::WORKER_RUNNING, Vec::new)
        .expect("the census declares this a gauge");

    assert_eq!(instruments.unclaimed().len(), outstanding - 1);
}

/// A gauge claimed as the wrong kind is refused before it is registered.
#[test]
fn a_counter_claimed_as_a_gauge_is_refused() {
    let instruments = instruments();
    let miscast: Declared<GaugeKind> = Declared::new(fleet::SIGNUP_BOOTSTRAPPED_TOTAL.wire_name());

    let refusal = instruments
        .gauge_u64(&miscast, Vec::new)
        .expect_err("the census declares this a counter");
    assert!(matches!(refusal, Error::KindMismatch { .. }));
}

/// A counter claimed as a histogram is refused.
#[test]
fn a_counter_claimed_as_a_histogram_is_refused() {
    let instruments = instruments();
    let miscast: Declared<HistogramKind> =
        Declared::new(fleet::SIGNUP_BOOTSTRAPPED_TOTAL.wire_name());

    let refusal = instruments
        .histogram_f64(&miscast)
        .expect_err("the census declares this a counter");
    assert!(matches!(refusal, Error::KindMismatch { .. }));
}

/// Every declared ceiling builds a stream the SDK accepts.
///
/// Built eagerly for exactly this reason: inside a view closure a refusal is
/// indistinguishable from "this view does not apply", and the family would
/// then export under SDK defaults with nobody told.
#[test]
fn every_declared_ceiling_builds_a_stream() {
    let registry = Registry::declared().expect("the compiled-in census reads");
    let _view =
        series_ceilings(&registry).expect("every declared ceiling is one the SDK will accept");
}
