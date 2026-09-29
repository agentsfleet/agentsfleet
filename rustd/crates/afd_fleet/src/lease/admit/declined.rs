//! The three answers a pass that did not admit can give.
//!
//! Split from [`Admission`] because a caller acting on a stop has three arms
//! to act on, not four: a type that also carried `Admit` made every stop
//! handler carry an arm for a pass that had already let the event through.

use afd_gate::gate::Waiting;

use super::{Admission, Billed, Refusal, Transient};

/// Why a pass stopped short of admitting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Declined {
    /// End the event with this label.
    Refuse(Refusal),
    /// Leave the delivery leasable; the next poll tries again.
    Retry(Transient),
    /// Leave the delivery leasable; a person owes an answer.
    Await(Waiting),
}

impl Admission {
    /// The billing an admission carries, or the stop it declined with.
    ///
    /// # Errors
    /// Every arm but [`Admission::Admit`], as its [`Declined`] twin.
    pub fn admitted(self) -> core::result::Result<Billed, Declined> {
        match self {
            Self::Admit(billed) => Ok(billed),
            Self::Refuse(refusal) => Err(Declined::Refuse(refusal)),
            Self::Retry(transient) => Err(Declined::Retry(transient)),
            Self::Await(waiting) => Err(Declined::Await(waiting)),
        }
    }
}

impl From<Declined> for Admission {
    fn from(declined: Declined) -> Self {
        match declined {
            Declined::Refuse(refusal) => Self::Refuse(refusal),
            Declined::Retry(transient) => Self::Retry(transient),
            Declined::Await(waiting) => Self::Await(waiting),
        }
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the restriction set is for the daemon"
    )]

    use afd_gate::gate::Waiting;

    use super::{Admission, Declined, Refusal, Transient};

    /// Every stop the pass can answer, one per arm.
    fn every_stop() -> [Declined; 3] {
        [
            Declined::Refuse(Refusal::labelled("fixture_label")),
            Declined::Retry(Transient { at: "fixture_gate" }),
            Declined::Await(Waiting::Pending),
        ]
    }

    /// A stop survives the trip through the public decision and back: no arm
    /// is renamed into another on the way, which would turn a person's pending
    /// answer into a retry or a refusal into a retry.
    #[test]
    fn a_stop_round_trips_through_the_admission_unchanged() {
        for stop in every_stop() {
            assert_eq!(
                Admission::from(stop.clone()).admitted(),
                Err(stop.clone()),
                "{stop:?} changed arm on the way through"
            );
        }
    }

    /// Only an admission yields billing; each stop's arm is its own twin.
    #[test]
    fn only_an_admission_yields_billing() {
        let billed = super::Billed {
            tenant_id: afd_core::id::Uuid7::encode(
                afd_core::clock::UnixMillis::from_millis(1_767_225_600_000),
                [9; afd_core::id::ENTROPY_LEN],
            )
            .expect("a fixed, in-range instant encodes"),
            posture: afd_billing::rates::Posture::Platform,
            provider: "anthropic".into(),
            model: "claude-fixture".into(),
            drained: afd_billing::Nanos::ZERO,
        };
        assert_eq!(Admission::Admit(billed.clone()).admitted(), Ok(billed));
        assert!(matches!(
            Admission::Retry(Transient { at: "fixture_gate" }).admitted(),
            Err(Declined::Retry(Transient { at: "fixture_gate" }))
        ));
    }
}
