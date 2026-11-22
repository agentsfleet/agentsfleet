//! A decision is reserved and recorded only under the run's own fence, by the
//! runner the run was issued to.

use af_core::error_code;
use af_wire::durable_task::{DecisionAssessment, DecisionReceiptInput};
use afd_fleet::durable_task::{DurableTaskLifecycle as _, DurableTaskRun};

use super::{
    super::fixture::{Case, exercise, id},
    fixture::{Decisions, known, request},
};

// Value: protects=a stale or foreign caller can neither open nor settle a paid
// decision; fails_when=the reservation's or the receipt's fence comparison, or
// the receipt's original-owner check, is dropped; why_new=the three refusals had
// no case; seam=the production reserve and receipt verbs over live Postgres, the
// foreign caller being the case's spare runner; no_claim=the run's own receipt,
// which the financial suites cover.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_decision_needs_the_runs_fence_and_its_own_runner() {
    exercise(async |case: &Case| {
        let fixture = Decisions::create(case).await;
        let mut stale = fixture
            .reservation(DecisionAssessment::Message, None)
            .into_inner();
        stale.fencing_token += 1;
        let stale = garde::Unvalidated::new(stale)
            .validate()
            .expect("a well-formed reservation");
        let refused = fixture
            .tasks
            .reserve_decision(case.run(), &stale)
            .await
            .expect_err("a stale fence opens no decision");
        assert_eq!(refused.code(), error_code::RUN_STALE_FENCING_TOKEN);
        assert_eq!(fixture.counts().await, (0, 0, 0), "nothing was reserved");

        let (_, body) = fixture
            .reserve(&fixture.reservation(DecisionAssessment::Message, None))
            .await;
        let call = request(&body);
        let receipt = |fencing_token| {
            garde::Unvalidated::new(DecisionReceiptInput {
                fencing_token,
                receipt: known(&call),
            })
            .validate()
            .expect("a bounded receipt")
        };
        let run = case.run();
        let refused = fixture
            .tasks
            .record_decision(run, &id(&call.call_id), &receipt(run.fencing_token + 1))
            .await
            .expect_err("a stale fence settles nothing");
        assert_eq!(refused.code(), error_code::RUN_STALE_FENCING_TOKEN);
        let foreign = DurableTaskRun {
            runner_id: &case.held.spare,
            ..run
        };
        let refused = fixture
            .tasks
            .record_decision(foreign, &id(&call.call_id), &receipt(run.fencing_token))
            .await
            .expect_err("another runner settles nothing");
        assert_eq!(refused.code(), error_code::RUN_STALE_FENCING_TOKEN);
        assert_eq!(
            fixture.counts().await,
            (1, 1, 0),
            "the call stands, uncharged"
        );
        fixture.model.finish().await.expect("model cleanup");
    })
    .await;
}
