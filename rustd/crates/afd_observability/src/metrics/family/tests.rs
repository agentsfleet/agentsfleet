//! Dimension 3.1, type-system half — the census and a family's traits agree.
//!
//! The registry tests next door prove the contract READS correctly. These prove
//! the other direction: that a Rust type claiming a kind is held against what
//! the census declares, and refused when the two were edited apart.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use super::{Counter, Gauge, Histogram, Metric};
use crate::error::Error;
use crate::metrics::registry::{Kind, Registry};

/// A counter the census really declares. One binding, because the same name is
/// claimed by two types below and a second spelling could drift from the first.
const REJECTIONS: &str = "agentsfleet_api_backpressure_rejections_total";

/// A gauge the census really declares.
const IN_FLIGHT: &str = "agentsfleet_api_in_flight_requests";

fn declared() -> Registry {
    Registry::declared().expect("the compiled-in census reads")
}

/// A family whose type claims counter and whose census row agrees.
struct BackpressureRejections;
impl Metric for BackpressureRejections {
    fn name(&self) -> &'static str {
        REJECTIONS
    }
}
impl Counter for BackpressureRejections {}

/// The same name, but its type claims a histogram — the drift this catches.
struct BackpressureRejectionsMisdeclared;
impl Metric for BackpressureRejectionsMisdeclared {
    fn name(&self) -> &'static str {
        REJECTIONS
    }
}
impl Histogram for BackpressureRejectionsMisdeclared {}

/// A type naming a family the census never declared.
struct Invented;
impl Metric for Invented {
    fn name(&self) -> &'static str {
        "agentsfleet.nothing.declares.this"
    }
}
impl Counter for Invented {}

/// A family whose type and census row agree resolves to its census entry.
#[test]
fn test_a_family_whose_type_matches_the_census_resolves() {
    let registry = declared();
    let entry = registry
        .counter(&BackpressureRejections)
        .expect("the census declares this family a counter");
    assert_eq!(&*entry.name, BackpressureRejections.name());
    assert_eq!(entry.kind, Kind::Counter);
}

/// A type claiming a kind the census contradicts is refused, naming both sides.
///
/// This is the failure the trait layer exists to make findable. It cannot be a
/// caller passing the wrong argument — the `M: Histogram` bound is settled by
/// the compiler — so it means the contract and the code were edited apart, and
/// the consequence would be an instrument built with the wrong aggregation.
#[test]
fn test_a_type_contradicting_the_census_is_refused() {
    let Err(Error::KindMismatch {
        declared: on_disk,
        claimed,
        ..
    }) = declared().histogram(&BackpressureRejectionsMisdeclared)
    else {
        unreachable!("a type claiming a kind the census contradicts must not resolve");
    };
    assert_eq!(on_disk, "counter");
    assert_eq!(claimed, "histogram");
}

/// A type naming a family nothing declares is refused before any kind check —
/// an invented name is a different defect from a mismatched kind, and reads as
/// one.
#[test]
fn test_a_type_naming_an_undeclared_family_is_refused() {
    assert!(matches!(
        declared().counter(&Invented),
        Err(Error::UnknownFamily { .. })
    ));
}

/// The kind check DISCRIMINATES — it is not a rubber stamp.
///
/// A real gauge family, resolved through each of the three resolvers: the gauge
/// one succeeds and the other two refuse, naming what the census actually says.
/// Without this, a check that returned `Ok` unconditionally would pass every
/// other test in this module.
#[test]
fn test_the_kind_check_refuses_the_two_kinds_a_family_is_not() {
    let registry = declared();

    let entry = registry
        .gauge(&InFlightRequests)
        .expect("the census declares this family a gauge");
    assert_eq!(entry.kind, Kind::Gauge);

    for wrong in [
        registry.counter(&InFlightRequests).err(),
        registry.histogram(&InFlightRequests).err(),
    ] {
        let Some(Error::KindMismatch {
            declared: on_disk, ..
        }) = wrong
        else {
            unreachable!("a gauge resolved as a counter or histogram must be refused");
        };
        assert_eq!(on_disk, "gauge");
    }
}

/// A real gauge family, carrying every kind trait so one type can be offered to
/// all three resolvers above. Only a test does this: a production family
/// implements exactly the one trait its census row declares, which is the whole
/// point of carrying kind in the type.
struct InFlightRequests;
impl Metric for InFlightRequests {
    fn name(&self) -> &'static str {
        IN_FLIGHT
    }
}
impl Counter for InFlightRequests {}
impl Histogram for InFlightRequests {}
impl Gauge for InFlightRequests {}
