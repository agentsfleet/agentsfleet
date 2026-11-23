//! A setup run that fails names why on its turn: a run too large for the
//! runner frame keeps `frame_too_large`, so the person is told to shorten the
//! message, and any other failure stays the provider's.

use std::borrow::Cow;

use af_wire::report::{FailureClass, Outcome, ReportCheckpoint, ReportRequest};

use super::{Case, Taken, processed};

/// The report a failed run sends: no answer, the class, and no proposal.
fn failed(taken: &Taken, class: FailureClass) -> ReportRequest<'_> {
    ReportRequest {
        outcome: Outcome::FleetError,
        failure_reason: Some(class),
        failure_detail: Cow::Borrowed("[UZ-RUN-032] fixture"),
        response_text: Cow::Borrowed(""),
        checkpoint: ReportCheckpoint {
            last_turn_id: Cow::Borrowed(&taken.turn),
            last_response: Cow::Borrowed(""),
        },
        ..processed(taken, None)
    }
}

// Value: protects=a setup message too large for a run says so, and is not blamed on the
// provider; fails_when=the report's class is dropped on the way to the turn;
// why_new=the setup report hard-coded `provider_failed`; seam=report_draft over live
// Postgres, with the report the daemon writes for a run it could not frame.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_setup_run_too_large_for_its_frame_fails_its_turn_as_frame_too_large() {
    let case = Case::queued().await;
    let taken = case.take_on_host().await;

    case.report(&failed(&taken, FailureClass::FrameTooLarge))
        .await
        .expect("a failed run settles");

    let turn = case.turn(&taken.turn_id()).await;
    assert_eq!(turn.status, "failed", "{turn:?}");
    assert_eq!(turn.failure_reason.as_deref(), Some("frame_too_large"));
    assert_eq!(turn.answer, None, "{turn:?}");
    assert_eq!(case.candidates().await.0, 1, "no candidate written");
    case.cleanup().await;
}

// Value: protects=only a frame failure gets its own reason; fails_when=every failure
// reads as frame_too_large; why_new=the twin of the case above; seam=report_draft over
// live Postgres.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_setup_run_failing_for_another_reason_fails_its_turn_as_provider_failed() {
    let case = Case::queued().await;
    let taken = case.take_on_host().await;

    case.report(&failed(&taken, FailureClass::RunnerCrash))
        .await
        .expect("a failed run settles");

    let turn = case.turn(&taken.turn_id()).await;
    assert_eq!(turn.status, "failed", "{turn:?}");
    assert_eq!(turn.failure_reason.as_deref(), Some("provider_failed"));
    case.cleanup().await;
}
