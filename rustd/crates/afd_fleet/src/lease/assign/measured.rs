//! What a lease poll examined, and what it spent reaching Postgres.
//!
//! A suite proving the READY-FIRST ordering has to assert on something a poll
//! did NOT do, and "no query was issued" leaves no row, no lease and no error
//! behind. The tally the poll already keeps for the operator gauges is the
//! evidence, so this hands the same numbers back to the caller instead of
//! only to `producers::fleet::lease_polled`.
//!
//! Compiled only under `test-util`: the daemon reads these numbers off the
//! meter, never off a return value, and a production caller that wanted them
//! here would be measuring the wrong seam.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;

use crate::error::Result;
use crate::lease::envelope::Acquired;
use crate::lease::store::Leases;

/// One poll's cost, in the two numbers whose ratio an operator reads.
#[derive(Debug, Clone, Copy)]
pub struct PollMeasurement {
    /// Fleets the readiness index offered this poll.
    pub candidates_scanned: u64,
    /// Statements this poll issued. Zero is the whole point of the empty
    /// path: see the ordering note at the top of `assign.rs`.
    pub database_roundtrips: u64,
}

impl Leases {
    /// [`Leases::select`], with the cost the same pass recorded.
    ///
    /// The outcome is returned whole rather than unwrapped, because the
    /// failing poll is the interesting one: a pass that reached Postgres and
    /// could not acquire a connection proves the reach, and its measurement
    /// says how many candidates sent it there.
    pub async fn select_measured(
        &self,
        runner_id: &Uuid7,
        now: UnixMillis,
    ) -> (Result<Option<Acquired>>, PollMeasurement) {
        let (selected, cost) = self.select_recording(runner_id, now).await;
        let measured = PollMeasurement {
            candidates_scanned: cost.candidates_scanned,
            database_roundtrips: cost.database_roundtrips,
        };
        (selected, measured)
    }
}
