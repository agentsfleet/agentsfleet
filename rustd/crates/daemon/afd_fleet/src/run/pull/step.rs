//! Continue-or-stop value for the run admission sequence.

use af_core::id::Uuid7;
use afd_gate::gate::Waiting;

use crate::run::answer::no_work;
use crate::run::envelope::Acquired;

/// The single stable reason logged by both waiting-for-approval paths.
const AWAITING_APPROVAL: &str = "a human owes an answer";

/// Continue, leave work available, or park the admission until a gate answer.
pub(in crate::run) enum Step<T> {
    /// Carry on, with this.
    Go(T),
    /// Stop without issuing work.
    Stop,
    /// Stop and wait for a person.
    Park,
}

impl<T> Step<T> {
    /// The value to carry on with, or this step's ending retyped for the
    /// caller one level up — a park stays a park on its way out.
    pub(in crate::run) fn proceed<U>(self) -> core::result::Result<T, Step<U>> {
        match self {
            Self::Go(value) => Ok(value),
            Self::Stop => Err(Step::Stop),
            Self::Park => Err(Step::Park),
        }
    }
}

/// A run written and rendered: the bytes a runner reads.
///
/// The claim is the run's now, so it is the one ending that keeps it.
pub(in crate::run) struct Assigned<T>(pub(in crate::run) T);

/// A gate's wait, as an ending.
///
/// A question on the record parks: its answer, or its expiry, re-marks the
/// fleet. A wait that could not read whether a question exists stops instead,
/// and the next poll reads again — nothing would re-mark a fleet on its behalf.
///
pub(super) fn waited<T>(runner_id: &Uuid7, waiting: Waiting) -> Step<T> {
    no_work(runner_id, AWAITING_APPROVAL);
    match waiting {
        Waiting::Parked | Waiting::Pending => Step::Park,
        Waiting::Unreadable => Step::Stop,
    }
}

/// The pass outlived its claim, so the run was not written.
const EVENT_CLAIM_LOST: &str = "run_claim_lost";

/// The no-work answer for a pass whose claim was superseded or lapsed before
/// its run was written.
///
/// A stop, so the fleet keeps its mark and whoever holds the slot now serves
/// the entry. The release the stop owes is fenced on this pass's token, so it
/// cannot free the slot the new holder won.
pub(in crate::run) fn claim_lost<T>(runner_id: &Uuid7, acquired: &Acquired) -> Step<T> {
    // Hoisted: the `log` bridge duplicates field expressions and llvm-cov
    // scores the dead copy.
    let runner = runner_id.as_str();
    let fleet_id = acquired.fleet_id.as_str();
    let agentsfleet_turn_id = acquired.turn_id.as_str();
    let fence = acquired.fence.as_i64();
    {
        let (event, runner_id, fencing_token, message) = (
            &EVENT_CLAIM_LOST,
            &runner,
            &fence,
            format_args!("the pass outlived its claim; no run was written"),
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
    no_work(runner_id, CLAIM_LOST_REASON);
    Step::Stop
}

/// The no-work reason a lost claim answers with.
const CLAIM_LOST_REASON: &str = "the claim lapsed before the run was written";

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the restriction set is for the daemon"
    )]

    use af_core::id::{ENTROPY_LEN, Uuid7};
    use afd_gate::gate::Waiting;
    use jiff::Timestamp;

    use super::{Step, waited};

    fn runner() -> Uuid7 {
        Uuid7::encode(Timestamp::constant(1_767_225_600, 0), [3; ENTROPY_LEN])
            .expect("a fixed instant encodes")
    }

    /// A question on the record parks, because its answer or its expiry
    /// re-marks the fleet.
    #[test]
    fn a_wait_on_a_recorded_question_parks() {
        for waiting in [Waiting::Parked, Waiting::Pending] {
            let step: Step<()> = waited(&runner(), waiting);
            assert!(matches!(step, Step::Park), "{waiting:?} must park");
        }
    }

    /// A wait that could not read whether a question exists keeps the mark:
    /// nothing would re-mark the fleet on its behalf.
    #[test]
    fn an_unreadable_wait_stops_without_parking() {
        let step: Step<()> = waited(&runner(), Waiting::Unreadable);
        assert!(matches!(step, Step::Stop));
    }

    /// A pass whose run insert lost its fence answers no-work, as a stop:
    /// the newer holder keeps its claim.
    #[cfg(feature = "test-util")]
    #[test]
    fn a_lost_claim_answers_no_work_as_a_stop() {
        use crate::run::test_dead;

        let runner = runner();
        let step = super::claim_lost::<()>(&runner, &test_dead::acquired());
        assert!(
            matches!(step, Step::Stop),
            "a lost claim is the no-work answer, and never a run"
        );
    }

    /// An ending keeps its kind on the way up; a park never becomes a stop.
    #[test]
    fn proceed_hands_back_the_value_or_the_same_ending() {
        assert!(matches!(Step::Go(7).proceed::<()>(), Ok(7)));
        let stopped = Step::<u8>::Stop.proceed::<()>();
        assert!(matches!(stopped, Err(Step::Stop)));
        let parked = Step::<u8>::Park.proceed::<()>();
        assert!(matches!(parked, Err(Step::Park)));
    }
}
