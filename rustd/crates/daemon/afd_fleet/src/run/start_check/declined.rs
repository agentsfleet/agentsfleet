//! The two answers a check that did not start the run can give.
//!
//! Split from [`StartCheck`] because a caller acting on a stop has two arms
//! to act on, not three: a type that also carried `Start` made every stop
//! handler carry an arm for a pass that had already let the event through.

use super::{Billed, Refusal, StartCheck, Transient};

/// Why a check stopped short of starting the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Declined {
    /// End the event with this label.
    Refuse(Refusal),
    /// Leave the delivery leasable; the next poll tries again.
    Retry(Transient),
}

impl StartCheck {
    /// The billing a start carries, or the stop it declined with.
    ///
    /// # Errors
    /// Every arm but [`StartCheck::Start`], as its [`Declined`] twin.
    pub fn started(self) -> Result<Billed, Declined> {
        match self {
            Self::Start(billed) => Ok(billed),
            Self::Refuse(refusal) => Err(Declined::Refuse(refusal)),
            Self::Retry(transient) => Err(Declined::Retry(transient)),
        }
    }
}

impl From<Declined> for StartCheck {
    fn from(declined: Declined) -> Self {
        match declined {
            Declined::Refuse(refusal) => Self::Refuse(refusal),
            Declined::Retry(transient) => Self::Retry(transient),
        }
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the restriction set is for the daemon"
    )]

    use super::{Declined, Refusal, StartCheck, Transient};

    /// Every stop the pass can answer, one per arm.
    fn every_stop() -> [Declined; 2] {
        [
            Declined::Refuse(Refusal::labelled("fixture_label")),
            Declined::Retry(Transient { at: "fixture_gate" }),
        ]
    }

    /// A stop survives the trip through the public decision and back: no arm
    /// is renamed into another on the way, which would turn a refusal into a
    /// retry.
    #[test]
    fn a_stop_round_trips_through_the_start_check_unchanged() {
        for stop in every_stop() {
            assert_eq!(
                StartCheck::from(stop).started(),
                Err(stop),
                "{stop:?} changed arm on the way through"
            );
        }
    }

    /// Only a start yields billing; each stop's arm is its own twin.
    #[test]
    fn only_a_start_yields_billing() {
        let billed = super::Billed {
            tenant_id: af_core::id::Uuid7::encode(
                jiff::Timestamp::constant(1_767_225_600, 0),
                [9; af_core::id::ENTROPY_LEN],
            )
            .expect("a fixed, in-range instant encodes"),
            posture: afd_billing::rates::Posture::Platform,
            provider: "anthropic".into(),
            model: "claude-fixture".into(),
        };
        assert_eq!(StartCheck::Start(billed.clone()).started(), Ok(billed));
        assert!(matches!(
            StartCheck::Retry(Transient { at: "fixture_gate" }).started(),
            Err(Declined::Retry(Transient { at: "fixture_gate" }))
        ));
    }
}
