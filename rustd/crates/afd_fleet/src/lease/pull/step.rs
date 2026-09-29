//! Continue-or-stop value for the lease admission sequence.

use afd_core::id::Uuid7;
use afd_gate::gate::Waiting;

use crate::error::Result;
use crate::lease::answer::no_work;

/// The single stable reason logged by both waiting-for-approval paths.
const AWAITING_APPROVAL: &str = "a human owes an answer";

/// Either the pass continues, or it already has its answer.
///
/// Every ending has the same serialized shape. Carrying that shape as a value
/// keeps each decision local and prevents a caller from forgetting which stop
/// already wrote a terminal row.
///
/// The two endings differ in what they leave the fleet's readiness mark. Both
/// free the claim. A stop keeps the mark, so the next poll comes back; a park
/// clears it, because a person owes an answer and the answer re-marks the
/// fleet — polling it every second meanwhile would re-ask the same question.
pub(in crate::lease) enum Step<T> {
    /// Carry on, with this.
    Go(T),
    /// Stop; these are the bytes.
    Stop(String),
    /// Stop and wait for a person; these are the bytes.
    Park(String),
}

impl<T> Step<T> {
    /// The value to carry on with, or this step's ending retyped for the
    /// caller one level up — a park stays a park on its way out.
    pub(in crate::lease) fn proceed<U>(self) -> core::result::Result<T, Step<U>> {
        match self {
            Self::Go(value) => Ok(value),
            Self::Stop(answer) => Err(Step::Stop(answer)),
            Self::Park(answer) => Err(Step::Park(answer)),
        }
    }
}

/// A lease written and rendered: the bytes a runner reads.
///
/// The claim is the lease's now, so it is the one ending that keeps it.
pub(in crate::lease) struct Leased(pub(in crate::lease) String);

/// A gate's wait, as an ending.
///
/// A question on the record parks: its answer, or its expiry, re-marks the
/// fleet. A wait that could not read whether a question exists stops instead,
/// and the next poll reads again — nothing would re-mark a fleet on its behalf.
///
/// # Errors
/// Reports an answer that would not render.
pub(super) fn waited<T>(runner_id: &Uuid7, waiting: Waiting) -> Result<Step<T>> {
    let answer = no_work(runner_id, AWAITING_APPROVAL)?;
    Ok(match waiting {
        Waiting::Parked | Waiting::Pending => Step::Park(answer),
        Waiting::Unreadable => Step::Stop(answer),
    })
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the restriction set is for the daemon"
    )]

    use afd_core::clock::UnixMillis;
    use afd_core::id::{ENTROPY_LEN, Uuid7};
    use afd_gate::gate::Waiting;

    use super::{Step, waited};

    fn runner() -> Uuid7 {
        Uuid7::encode(UnixMillis::from_millis(1_767_225_600_000), [3; ENTROPY_LEN])
            .expect("a fixed instant encodes")
    }

    /// A question on the record parks, because its answer or its expiry
    /// re-marks the fleet.
    #[test]
    fn a_wait_on_a_recorded_question_parks() {
        for waiting in [Waiting::Parked, Waiting::Pending] {
            let step: Step<()> = waited(&runner(), waiting).expect("the answer renders");
            assert!(matches!(step, Step::Park(_)), "{waiting:?} must park");
        }
    }

    /// A wait that could not read whether a question exists keeps the mark:
    /// nothing would re-mark the fleet on its behalf.
    #[test]
    fn an_unreadable_wait_stops_without_parking() {
        let step: Step<()> = waited(&runner(), Waiting::Unreadable).expect("the answer renders");
        assert!(matches!(step, Step::Stop(_)));
    }

    /// An ending keeps its kind on the way up; a park never becomes a stop.
    #[test]
    fn proceed_hands_back_the_value_or_the_same_ending() {
        assert!(matches!(Step::Go(7).proceed::<()>(), Ok(7)));
        let stopped = Step::<u8>::Stop("stop".to_owned()).proceed::<()>();
        assert!(matches!(stopped, Err(Step::Stop(answer)) if answer == "stop"));
        let parked = Step::<u8>::Park("park".to_owned()).proceed::<()>();
        assert!(matches!(parked, Err(Step::Park(answer)) if answer == "park"));
    }
}
