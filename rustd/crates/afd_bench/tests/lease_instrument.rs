//! The daemon's lease counters, read back in this process.
//!
//! # Why an integration binary and not the unit suite
//!
//! `producers::install` writes a process-wide `OnceLock`. Installing from the
//! unit suite would flip `installed()` to `Some` for every other test in that
//! binary, mid-run and in whatever order the harness chose. Here the process is
//! this file's own, so the install is total and ordered — the same reason
//! `afd_observability`'s own `producers_drive.rs` lives beside its unit tests
//! rather than inside them.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_bench::instrument::{LeaseInstrument, PollCounters};
use afd_observability::producers;

/// Candidates the fake poll claims to have examined.
const CANDIDATES: u64 = 11;

/// Round trips the fake poll claims to have issued.
const ROUNDTRIPS: u64 = 2;

/// Both counter readings live in ONE test on purpose.
///
/// The instruments are process-global — `producers::install` writes a
/// `OnceLock` and every producer records into the same set — while the harness
/// runs tests on parallel threads. Two tests each asserting an exact delta
/// around their own producer call would see each other's increments, which is
/// not a flaky test so much as a true statement about the counter. The same
/// property binds a lane: it reads a delta around its own window and must be
/// the only thing polling in that process.
#[test]
fn test_the_lease_counters_are_read_back_from_the_daemons_own_instrument() {
    let instrument = LeaseInstrument::install().expect("the census must install");

    let before_worked = instrument.read().expect("the counters must be readable");
    producers::fleet::lease_polled(CANDIDATES, ROUNDTRIPS);
    let worked = instrument
        .read()
        .expect("the counters must be readable")
        .since(before_worked);

    assert_eq!(
        worked.polls, 1,
        "one call to the producer is one poll, read off the instrument the \
         daemon publishes rather than off a wrapper this crate wrote"
    );
    assert_eq!(worked.candidates, CANDIDATES);
    assert_eq!(worked.roundtrips, ROUNDTRIPS);

    // What the lease path records when the readiness index is empty: it
    // returns before touching Postgres, so the poll counts and the round trips
    // do not. This is the number a million idle fleets multiply.
    let before_idle = instrument.read().expect("readable");
    producers::fleet::lease_polled(0, 0);
    let idle = instrument.read().expect("readable").since(before_idle);

    assert_eq!(idle.polls, 1);
    assert_eq!(
        idle.roundtrips, 0,
        "an idle poll costing a round trip would make idle cost scale with \
         fleets rather than with runners"
    );
    assert!((idle.roundtrips_per_poll() - 0.0).abs() < f64::EPSILON);
}

#[test]
fn test_a_delta_never_runs_backwards() {
    let later = PollCounters {
        polls: 5,
        candidates: 50,
        roundtrips: 10,
    };
    let earlier = PollCounters {
        polls: 9,
        candidates: 90,
        roundtrips: 18,
    };

    let delta = later.since(earlier);

    assert_eq!(
        delta,
        PollCounters::default(),
        "a reading that looks older than its baseline saturates to zero rather \
         than wrapping to a number somebody reports as throughput"
    );
}

#[test]
fn test_roundtrips_per_poll_is_the_ratio_an_operator_reads() {
    let counters = PollCounters {
        polls: 4,
        candidates: 40,
        roundtrips: 10,
    };

    assert!((counters.roundtrips_per_poll() - 2.5).abs() < f64::EPSILON);
}
