//! Continue-or-stop value for the run admission sequence.

use af_core::id::Uuid7;

use crate::run::answer::no_work;
use crate::run::envelope::Acquired;

/// Continue, or stop and leave the work available.
pub(in crate::run) enum Step<T> {
    /// Carry on, with this.
    Go(T),
    /// Stop without issuing work.
    Stop,
}

impl<T> Step<T> {
    /// The value to carry on with, or this step's ending retyped for the
    /// caller one level up.
    pub(in crate::run) fn proceed<U>(self) -> Result<T, Step<U>> {
        match self {
            Self::Go(value) => Ok(value),
            Self::Stop => Err(Step::Stop),
        }
    }
}

/// A run written and rendered: the bytes a runner reads.
///
/// The assignment is the run's now, so it is the one ending that keeps it.
pub(in crate::run) struct Assigned<T>(pub(in crate::run) T);

/// The pass outlived its assignment, so the run was not written.
const EVENT_ASSIGNMENT_LOST: &str = "run_assignment_lost";

/// The no-work answer for a pass whose assignment was superseded or lapsed before
/// its run was written.
///
/// A stop, so the fleet keeps its mark and whoever holds the thread now serves
/// the entry. The release the stop owes is fenced on this pass's token, so it
/// cannot free the thread the new holder won.
pub(in crate::run) fn assignment_lost<T>(runner_id: &Uuid7, acquired: &Acquired) -> Step<T> {
    // Hoisted: the `log` bridge duplicates field expressions and llvm-cov
    // scores the dead copy.
    let runner = runner_id.as_str();
    let fleet_id = acquired.fleet_id.as_str();
    let agentsfleet_turn_id = acquired.turn_id.as_str();
    let fence = acquired.fence.as_i64();
    {
        let (event, runner_id, fencing_token, message) = (
            &EVENT_ASSIGNMENT_LOST,
            &runner,
            &fence,
            format_args!("the pass outlived its assignment; no run was written"),
        );
        tracing::info!(
            message,
            event,
            runner_id,
            fleet_id,
            agentsfleet_turn_id,
            fencing_token
        );
    };
    no_work(runner_id, ASSIGNMENT_LOST_REASON);
    Step::Stop
}

/// The no-work reason a lost assignment answers with.
const ASSIGNMENT_LOST_REASON: &str = "the assignment lapsed before the run was written";

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the restriction set is for the daemon"
    )]

    use super::Step;

    /// A pass whose run insert lost its fence answers no-work, as a stop:
    /// the newer holder keeps its assignment.
    #[cfg(feature = "test-util")]
    #[test]
    fn a_lost_assignment_answers_no_work_as_a_stop() {
        use af_core::id::{ENTROPY_LEN, Uuid7};
        use jiff::Timestamp;

        use crate::run::test_dead;

        let runner = Uuid7::encode(Timestamp::constant(1_767_225_600, 0), [3; ENTROPY_LEN])
            .expect("a fixed instant encodes");
        let step = super::assignment_lost::<()>(&runner, &test_dead::acquired());
        assert!(
            matches!(step, Step::Stop),
            "a lost assignment is the no-work answer, and never a run"
        );
    }

    /// An ending keeps its kind on the way up.
    #[test]
    fn proceed_hands_back_the_value_or_the_same_ending() {
        assert!(matches!(Step::Go(7).proceed::<()>(), Ok(7)));
        let stopped = Step::<u8>::Stop.proceed::<()>();
        assert!(matches!(stopped, Err(Step::Stop)));
    }
}
