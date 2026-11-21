//! What a run poll examined.
//!
//! A suite proving the READY-FIRST ordering has to assert on something a poll
//! did NOT do, and "no query was issued" leaves no row, no run and no error
//! behind. The tally the poll already keeps for the operator gauges is the
//! evidence, so this hands the same numbers back to the caller instead of
//! only to `producers::fleet::run_polled`.
//!
//! Compiled only under `test-util`: the daemon reads these numbers off the
//! meter, never off a return value, and a production caller that wanted them
//! here would be measuring the wrong seam.

use af_core::id::Uuid7;

use crate::error::Result;
use crate::run::assignment::RecoveryScan;
use crate::run::envelope::Acquired;
use crate::run::store::Runs;

/// One poll's examined candidates. Statement cost is measured from
/// `pg_stat_statements` in the bench, which counts what every crate sends.
#[derive(Debug, Clone, Copy)]
pub struct PollMeasurement {
    /// Fleets the ledger scan assigned this poll.
    pub candidates_scanned: u64,
}

impl Runs {
    /// [`Runs::select`], with the cost the same pass recorded.
    ///
    /// The outcome is returned whole rather than unwrapped, because the
    /// failing poll is the interesting one: a pass that reached Postgres and
    /// could not acquire a connection proves the reach, and its measurement
    /// says how many candidates sent it there.
    pub async fn select_measured(
        &self,
        runner_id: &Uuid7,
        held: &[Uuid7],
    ) -> (Result<Option<Acquired>>, PollMeasurement) {
        self.select_counted(runner_id, held, None).await
    }

    /// Probe one real recovery page and expose its scan count to the fixture.
    pub async fn select_recovery_measured(
        &self,
        runner_id: &Uuid7,
        scan: &mut RecoveryScan,
    ) -> (Result<Option<Acquired>>, PollMeasurement) {
        scan.begin_attempt();
        self.select_counted(runner_id, &[], Some(scan)).await
    }

    async fn select_counted(
        &self,
        runner_id: &Uuid7,
        held: &[Uuid7],
        scan: Option<&mut RecoveryScan>,
    ) -> (Result<Option<Acquired>>, PollMeasurement) {
        let (selected, cost) = self.select_recording(runner_id, held, scan).await;
        let measured = PollMeasurement {
            candidates_scanned: cost.candidates_scanned,
        };
        (selected, measured)
    }
}
