//! Continue-or-stop value for the run admission sequence.

use af_core::id::Uuid7;

use crate::run::answer::no_work;
use crate::run::envelope::Acquired;
use crate::run::issue::Refused;

/// Continue, or stop and leave the work available.
pub(in crate::run) enum Step<T> {
    /// Carry on, with this.
    Go(T),
    /// Stop without issuing work.
    Stop,
    /// Stop: the issue refused a run the gates admitted for want of room, time
    /// or the placement choice it read. The turn is still its fleet's head, can
    /// run, and no pass is owed it, so once the assignment is let go the fleet's
    /// wake is rung.
    Lost,
}

impl<T> Step<T> {
    /// The value to carry on with, or this step's ending retyped for the
    /// caller one level up.
    pub(in crate::run) fn proceed<U>(self) -> Result<T, Step<U>> {
        match self {
            Self::Go(value) => Ok(value),
            Self::Stop => Err(Step::Stop),
            Self::Lost => Err(Step::Lost),
        }
    }
}

/// A run written and rendered: the bytes a runner reads.
///
/// The assignment is the run's now, so it is the one ending that keeps it.
pub(in crate::run) struct Assigned<T>(pub(in crate::run) T);

/// The issue refused an assigned run, so none was written.
const EVENT_ASSIGNMENT_LOST: &str = "run_assignment_lost";

/// The no-work answer for a pass whose issue refused its run, naming the guard
/// that refused it and, when the insert was refused, the host's census.
///
/// [`Step::Lost`] when another pass can run the turn; a plain [`Step::Stop`]
/// when this host may not take it or the work cannot run, where a ring would
/// only hand the same refusal another pass.
pub(in crate::run) fn assignment_lost<T>(
    runner_id: &Uuid7,
    acquired: &Acquired,
    refused: Refused,
) -> Step<T> {
    // Hoisted: the `log` bridge duplicates field expressions and llvm-cov
    // scores the dead copy.
    let runner = runner_id.as_str();
    let fleet_id = acquired.fleet_id.as_str();
    let agentsfleet_turn_id = acquired.turn_id.as_str();
    let fence = acquired.fence.as_i64();
    let reason = refused.as_str();
    let census = refused.census();
    let occupied = census.map(|counted| counted.occupied);
    let held = census.map(|counted| counted.held);
    let worker_count = census.map(|counted| counted.worker_count);
    {
        let (event, runner_id, fencing_token, message) = (
            &EVENT_ASSIGNMENT_LOST,
            &runner,
            &fence,
            format_args!("the issue refused the assigned run; no run was written"),
        );
        tracing::info!(
            message,
            event,
            runner_id,
            fleet_id,
            agentsfleet_turn_id,
            fencing_token,
            reason,
            occupied,
            held,
            worker_count
        );
    };
    no_work(runner_id, ASSIGNMENT_LOST_REASON);
    if refused.rerings() {
        Step::Lost
    } else {
        Step::Stop
    }
}

/// The no-work reason a lost assignment answers with.
const ASSIGNMENT_LOST_REASON: &str = "the issue refused the assigned run";

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the restriction set is for the daemon"
    )]

    use super::Step;

    /// The line names the guard and the census it read, so a lost assignment
    /// can be told from a lapse without the database row.
    #[cfg(feature = "test-util")]
    #[test]
    fn a_lost_assignment_names_the_guard_that_refused_it() {
        use af_core::id::{ENTROPY_LEN, Uuid7};
        use af_core::test_util::trace::Capture;
        use jiff::Timestamp;

        use crate::run::issue::{Census, Refused};
        use crate::run::test_dead;

        let log = Capture::install();
        let runner = Uuid7::encode(Timestamp::constant(1_767_225_600, 0), [3; ENTROPY_LEN])
            .expect("a fixed instant encodes");
        let full = Refused::CensusFull(Census {
            occupied: 3,
            held: 1,
            worker_count: 4,
        });
        let step = super::assignment_lost::<()>(&runner, &test_dead::acquired(), full);
        let line = log.only(super::EVENT_ASSIGNMENT_LOST);
        drop(log);
        // A connection that no longer proves its host rings nothing: the
        // same connection's next pass would be refused the same way. Its line
        // names that guard, and carries no census, because none was read.
        let log = Capture::install();
        let unproven =
            super::assignment_lost::<()>(&runner, &test_dead::acquired(), Refused::HostUnproven);
        let other = log.only(super::EVENT_ASSIGNMENT_LOST);
        drop(log);

        assert!(
            matches!((step, unproven), (Step::Lost, Step::Stop)),
            "a lost assignment is never a run, and rings only when another pass can run it"
        );
        assert_eq!(
            [
                line.field("reason"),
                line.field("occupied"),
                line.field("held"),
                line.field("worker_count"),
            ],
            [Some("census_full"), Some("3"), Some("1"), Some("4")]
        );
        assert_eq!(
            [
                other.field("reason"),
                other.field("occupied"),
                other.field("held"),
                other.field("worker_count"),
            ],
            [Some("host_unproven"), None, None, None]
        );
    }

    /// An ending keeps its kind on the way up.
    #[test]
    fn proceed_hands_back_the_value_or_the_same_ending() {
        assert!(matches!(Step::Go(7).proceed::<()>(), Ok(7)));
        let stopped = Step::<u8>::Stop.proceed::<()>();
        assert!(matches!(stopped, Err(Step::Stop)));
        let lost = Step::<u8>::Lost.proceed::<()>();
        assert!(matches!(lost, Err(Step::Lost)));
    }
}
