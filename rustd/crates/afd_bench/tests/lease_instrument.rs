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

/// Round trips the fake poll claims to have issued.
const ROUNDTRIPS: u64 = 2;

/// Dragonfly commands the fake poll claims to have issued.
const COMMANDS: u64 = 3;

/// What an idle poll sends Postgres: the one claim statement.
const IDLE_STATEMENTS: u64 = 1;

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
    producers::fleet::lease_polled(ROUNDTRIPS, COMMANDS);
    let worked = instrument
        .read()
        .expect("the counters must be readable")
        .since(before_worked);

    assert_eq!(
        worked.polls, 1,
        "one call to the producer is one poll, read off the instrument the \
         daemon publishes rather than off a wrapper this crate wrote"
    );
    assert_eq!(worked.roundtrips, ROUNDTRIPS);
    assert_eq!(
        worked.dragonfly_commands, COMMANDS,
        "the poll's Dragonfly cost is published"
    );

    // A second install hands back the first instrument rather than building
    // a provider nothing records into: it reads exactly what the first reads.
    let again = LeaseInstrument::install().expect("a second install must succeed");
    assert_eq!(
        again.read().expect("readable"),
        instrument.read().expect("readable"),
        "a later install that built its own provider would read zero forever"
    );

    // What the lease path records when nothing is ready: the one claim
    // statement, which drives from the readiness index, and no Dragonfly
    // command. This is the number a million idle fleets must not multiply.
    let before_idle = instrument.read().expect("readable");
    producers::fleet::lease_polled(IDLE_STATEMENTS, 0);
    let idle = instrument.read().expect("readable").since(before_idle);

    assert_eq!(idle.polls, 1);
    assert_eq!(
        idle.roundtrips, IDLE_STATEMENTS,
        "an idle poll is one claim statement however many fleets exist"
    );
    assert_eq!(
        idle.dragonfly_commands, 0,
        "an idle poll asks nothing of Dragonfly"
    );
}

#[test]
fn test_a_delta_never_runs_backwards() {
    let later = PollCounters {
        polls: 5,
        roundtrips: 10,
        dragonfly_commands: 20,
    };
    let earlier = PollCounters {
        polls: 9,
        roundtrips: 18,
        dragonfly_commands: 36,
    };

    let delta = later.since(earlier);

    assert_eq!(
        delta,
        PollCounters::default(),
        "a reading that looks older than its baseline saturates to zero rather \
         than wrapping to a number somebody reports as throughput"
    );
}
