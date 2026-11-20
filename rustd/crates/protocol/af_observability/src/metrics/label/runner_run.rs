//! What `agentsfleet_runner_runs_total` labels a run with on its runner.

use crate::metrics::label::closed_set;

closed_set! {
    /// A run taken by its runner, or how a run it held ended.
    ///
    /// Acquired is the denominator: every acquired run ends in exactly one of
    /// the other three, so a gap between them is runs still live.
    RunnerRun {
        /// The run was issued to the runner.
        Acquired => "acquired",
        /// The runner reported the run finished cleanly.
        Succeeded => "succeeded",
        /// The runner reported the run failed, or a person stopped it.
        Failed => "failed",
        /// The runner went silent, and a later assignment or the retention
        /// sweep expired the run.
        Expired => "expired",
    }
}
