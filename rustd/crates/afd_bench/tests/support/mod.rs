//! What every lane test needs: the rig's datastores, a serialising lock, and
//! a way to read a measurement out of a report.
//!
//! # One lane at a time
//!
//! The readiness index is one global hash, the outbound consumer name is
//! constant per process, and Redis's `used_memory` is one number for the
//! server. Two lanes measuring at once would each see the other's work in
//! its numbers, so every test takes [`LANE`] first — a slower suite, but one
//! whose assertions are about the lane under test and nothing else.

#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test support: an unmet precondition should fail the test loudly"
)]
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]

use tokio::sync::Mutex;

use afd_bench::datastores::Datastores;
use afd_bench::report::Report;

/// The knobs `make test-integration-rustd` exports; see `make/test-infra.mk`.
const DATABASE_KNOB: &str = "TEST_DATABASE_URL";
const REDIS_KNOB: &str = "TEST_REDIS_URL";
const CA_KNOB: &str = "TEST_REDIS_CA_CERT";

/// Held for the whole of a lane run.
///
/// tokio's mutex rather than std's, because the guard is held across every
/// `.await` in a lane and a std guard across an await point is the deadlock
/// clippy refuses for good reason.
pub(crate) static LANE: Mutex<()> = Mutex::const_new(());

/// The lane's Redis URL, for the outbound lane's dedicated reader.
pub(crate) fn redis_url() -> String {
    std::env::var(REDIS_KNOB).unwrap_or_else(|_unset| {
        panic!("{REDIS_KNOB} is unset — run through make test-integration-rustd")
    })
}

/// The lane's Redis certificate authority, when it serves TLS.
pub(crate) fn ca_cert() -> Option<String> {
    std::env::var(CA_KNOB).ok()
}

/// Both datastores, opened the way a lane opens them.
pub(crate) async fn datastores() -> Datastores {
    let database = std::env::var(DATABASE_KNOB).unwrap_or_else(|_unset| {
        panic!("{DATABASE_KNOB} is unset — run through make test-integration-rustd")
    });
    Datastores::open(&database, &redis_url(), ca_cert())
        .await
        .expect("the rig's datastores must be reachable")
}

/// A measurement a lane wrote, by key.
pub(crate) fn measurement(report: &Report, key: &str) -> f64 {
    *report.measurements.get(key).unwrap_or_else(|| {
        panic!(
            "the report must carry {key}; it has {:?}",
            report.measurements.keys()
        )
    })
}

/// A series a lane wrote, by key.
pub(crate) fn series<'a>(report: &'a Report, key: &str) -> &'a [f64] {
    report.series.get(key).unwrap_or_else(|| {
        panic!(
            "the report must carry the {key} series; it has {:?}",
            report.series.keys()
        )
    })
}
