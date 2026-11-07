use std::time::Duration;

use afd_core::clock;
use afd_core::timing::RUNNER_OFFLINE_AFTER;
use afd_wire::runner::RunnerLiveness;
use jiff::Timestamp;

use super::decode::derive_liveness;
use crate::sql::NEVER_SEEN;

/// The database clock every case is judged at.
const CHECKED_AT: Timestamp = Timestamp::constant(1_760_000_000, 0);

/// The finest step an instant takes in the column.
const ONE_MILLISECOND: Duration = Duration::from_millis(1);

#[test]
fn liveness_keeps_never_seen_busy_fresh_and_stale_states_distinct() {
    let fresh = clock::before(CHECKED_AT, RUNNER_OFFLINE_AFTER);
    let stale = clock::before(fresh, ONE_MILLISECOND);
    assert_eq!(
        derive_liveness(NEVER_SEEN, false, CHECKED_AT),
        RunnerLiveness::Registered
    );
    assert_eq!(
        derive_liveness(stale, true, CHECKED_AT),
        RunnerLiveness::Busy
    );
    assert_eq!(
        derive_liveness(fresh, false, CHECKED_AT),
        RunnerLiveness::Online
    );
    assert_eq!(
        derive_liveness(stale, false, CHECKED_AT),
        RunnerLiveness::Offline
    );
}

#[test]
fn a_future_heartbeat_is_fresh() {
    assert_eq!(
        derive_liveness(Timestamp::MAX, false, CHECKED_AT),
        RunnerLiveness::Online
    );
}

#[test]
fn a_never_seen_runner_is_registered_even_while_leased() {
    // The sentinel outranks a lease: a runner that never beat cannot be
    // running anything, so "busy" there would describe a row nothing can make.
    assert_eq!(
        derive_liveness(NEVER_SEEN, true, CHECKED_AT),
        RunnerLiveness::Registered
    );
}
