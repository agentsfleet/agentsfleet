//! The verb's first decision, which must be made before any datastore is asked.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the restriction set is for the daemon"
)]

use crate::run::test_dead;

/// A degraded runner is answered no work without a datastore being asked:
/// the plane here has none that answers, so any read would be a fault.
#[tokio::test]
async fn should_answer_a_degraded_runner_without_touching_a_datastore() {
    let mut issued = 0;
    let answer = test_dead::plane()
        .assign_fleet_with(
            &mut crate::run::AssignRequest {
                runner: &test_dead::id(9),
                held: &[],
                degraded: true,
                now: test_dead::AT,
                recovery: None,
            },
            None,
            &mut af_observability::verb_span::VerbClock::untimed(),
            |_run, _metadata| {
                issued += 1;
            },
        )
        .await
        .expect("a degraded runner is a decision, not a datastore read");
    assert!(answer.is_none());
    assert_eq!(issued, 0, "a degraded runner cannot receive an assignment");
}

/// A failed terminal settlement stays retryable instead of claiming success.
#[tokio::test]
async fn terminal_settlement_propagates_a_database_outage() {
    let refused = test_dead::plane()
        .finished(&test_dead::acquired(), &test_dead::id(9))
        .await
        .err()
        .expect("the durable settlement failed");
    assert!(refused.is_datastore_unavailable(), "{refused}");
}

/// A fleet whose unreadable row cannot even be recorded is answered no work,
/// and neither the row's reason nor the write's failure carries a code.
#[tokio::test]
async fn an_unreadable_config_refusal_reason_carries_no_code_prefix() {
    use af_core::test_util::trace::Capture;

    let log = Capture::install();
    let fault = crate::Error::from(crate::error::ErrorKind::FleetRowUnreadable);

    test_dead::plane()
        .refuse_unreadable_config(&test_dead::acquired(), &test_dead::id(9), &fault)
        .await
        .expect("an unrecorded refusal is logged, not raised");

    let line = log.only(super::refuse::EVENT_CONFIG_REFUSAL_UNRECORDED);
    assert_eq!(line.field("config_reason"), Some(fault.told().as_str()));
    assert_eq!(
        line.field("config_error_code"),
        Some(fault.code().as_str()),
        "the config fault's own code rides beside its reason"
    );
    let reason = line.field("reason").unwrap_or_default();
    assert!(
        !reason.is_empty() && !reason.contains("[UZ-") && !reason.contains('\n'),
        "the write's failure is its sentence alone, its code the line's `error_code`: {reason}"
    );
}

// Value: protects=a gate that could not decide ends the pass with no work and
// leaves the delivery leasable, writing nothing; fails_when=a retry is ended as a
// refusal, which writes the terminal row this dead pool would refuse, or is
// answered without naming the gate; why_new=the retry arm had no case;
// seam=`ended`, the one place every declined start check lands.
#[tokio::test]
async fn a_gate_that_could_not_decide_answers_no_work_and_writes_nothing() {
    use crate::run::start_check::{Declined, Transient};
    use af_core::test_util::trace::Capture;

    let log = Capture::install();
    let step = test_dead::plane()
        .ended(
            &test_dead::acquired(),
            Declined::Retry(Transient {
                at: "approval_gate",
            }),
            &test_dead::id(9),
        )
        .await
        .expect("a retry is an answer, not a write");

    assert!(matches!(step, super::step::Step::Stop));
    let line = log.only(crate::run::answer::EVENT_NO_WORK);
    assert_eq!(line.field("reason"), Some("approval_gate"));
    assert_eq!(line.field("runner_id"), Some(test_dead::id(9).as_str()));
}

// Value: protects=an assignment whose fleet row reads as no longer active is
// answered no work, and nothing is written for it; fails_when=the inactive
// arm refuses the event, which writes the terminal row this dead pool would
// refuse, or issues it; why_new=no test handed the pass an inactive fleet row;
// seam=`resolve_installed` over datastores nobody answers on.
#[tokio::test]
async fn a_fleet_no_longer_active_is_answered_no_work() {
    use af_core::test_util::trace::Capture;

    let mut acquired = test_dead::acquired();
    "archived".clone_into(&mut acquired.opened.setup.status);
    let log = Capture::install();

    let step = test_dead::plane()
        .resolve_installed(&acquired, &test_dead::id(9))
        .await
        .expect("an inactive fleet is an answer, not a write");

    assert!(matches!(step, super::step::Step::Stop));
    let line = log.only(crate::run::answer::EVENT_NO_WORK);
    assert_eq!(line.field("reason"), Some(super::refuse::FLEET_ARCHIVED));
}
