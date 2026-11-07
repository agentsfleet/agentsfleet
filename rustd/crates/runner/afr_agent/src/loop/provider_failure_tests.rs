#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test module: a failed precondition should fail the test loudly"
)]

//! How a provider that refuses, drops, or cannot be reached ends the run.

use crate::ResultOutcome;
use af_egress::testing::CountingMint;
use af_providers::Error;
use af_wire::report::FailureClass;
use tokio_util::sync::CancellationToken;

use super::tests::{drive, engine};
use crate::engine::{AgentEngine, AgentRun, Meter};
use crate::fixture::{Frames, Script, Unreachable, lease, say, unbounded};
use crate::testing::Discard;

#[tokio::test]
async fn a_provider_refusal_ends_the_run_as_the_fleets_error() {
    let script = Script::failing(Vec::new(), || Error::refused(401));
    let engine = engine(Vec::new(), &script);

    let (output, _frames) =
        drive(&engine, &lease(&[], unbounded()), &CancellationToken::new()).await;

    let ResultOutcome::Failed(failure) = output.result.outcome else {
        panic!("a refused turn fails the run");
    };
    assert_eq!(
        failure.class, None,
        "the fleet's error carries no failure reason"
    );
    assert!(failure.detail.contains("401"), "{}", failure.detail);
    assert!(
        !failure.detail.contains("UZ-"),
        "the detail is a sentence, not a code"
    );
}

#[tokio::test]
async fn a_lost_provider_connection_ends_the_run_as_transport_loss() {
    let lost = || Error::lost(std::io::Error::other("connection reset"));
    let script = Script::failing(vec![say("partial ")], lost);
    let engine = engine(Vec::new(), &script);

    let (output, _frames) =
        drive(&engine, &lease(&[], unbounded()), &CancellationToken::new()).await;

    let ResultOutcome::Failed(failure) = output.result.outcome else {
        panic!("a lost turn fails the run");
    };
    assert_eq!(failure.class, Some(FailureClass::TransportLoss));
    assert_eq!(output.result.content, "", "a failed run reports no answer");
}

#[tokio::test]
async fn a_provider_that_cannot_be_reached_is_admitted_then_an_engine_error() {
    let engine = super::Loop::new(afr_tools::Catalog::new(Vec::new()), Unreachable);
    let lease = lease(&[], unbounded());
    let frames = Frames::default();
    let sink = frames.sink();

    assert!(
        engine.admit(&lease.policy).is_ok(),
        "admission never dials the model; only the run connects"
    );
    let failure = engine
        .run(AgentRun {
            recovered_task: None,
            prepared_task: None,
            lease: &lease,
            memory: afr_memory::Seed::default(),
            executor: &afr_executor::ExecutorSlot::unavailable(
                afr_executor::Unavailable::NoSandbox,
            ),
            mint: &CountingMint::never(),
            verbs: &afr_tools::CLOSED,
            checkpoint: &Discard,
            context_checkpoints: &crate::testing::ContextMemory::default(),
            events: &sink,
            meter: &Meter::default(),
            stop: &CancellationToken::new(),
        })
        .await
        .unwrap_err();

    assert_eq!(
        failure.code(),
        af_core::error_code::INTERNAL_OPERATION_FAILED
    );
    assert_eq!(
        frames.taken(),
        [] as [af_wire::activity::ActivityFrame<'_>; 0]
    );
}

/// The turn outcomes `scoped` recorded, in order.
fn turn_outcomes(
    recorded: &std::sync::mpsc::Receiver<afr_telemetry::testing::Recorded>,
) -> Vec<afr_telemetry::labels::TurnOutcome> {
    recorded
        .try_iter()
        .filter_map(|recorded| match recorded {
            afr_telemetry::testing::Recorded::Turn(_provider, outcome, _elapsed) => Some(outcome),
            _other => None,
        })
        .collect()
}

/// A turn the provider fails is counted as failed, under the lease's
/// provider.
#[tokio::test]
async fn a_failed_turn_is_counted_as_failed() {
    use afr_telemetry::labels::TurnOutcome;
    use afr_telemetry::testing::{Tally, scoped};

    let script = Script::failing(Vec::new(), || Error::refused(401));
    let engine = engine(Vec::new(), &script);
    let (tally, recorded) = Tally::new();

    let _ended = scoped(
        tally,
        drive(&engine, &lease(&[], unbounded()), &CancellationToken::new()),
    )
    .await;

    assert_eq!(turn_outcomes(&recorded), vec![TurnOutcome::Failed]);
}

/// A provider whose turn streams nothing and never ends.
#[derive(Debug, Clone, Copy)]
struct Silent;

impl af_providers::Connect for Silent {
    fn admit(&self, _policy: &af_wire::policy::ExecutionPolicy<'_>) -> af_providers::Result<()> {
        Ok(())
    }

    fn connect(
        &self,
        _lease: &af_wire::lease::LeasePayload<'_>,
    ) -> af_providers::Result<Box<dyn af_providers::Provider>> {
        Ok(Box::new(Self))
    }
}

impl af_providers::Provider for Silent {
    fn stream<'a>(
        &'a self,
        _request: af_providers::Request<'a>,
    ) -> futures_util::stream::BoxStream<'a, af_providers::Result<af_providers::Chunk>> {
        use futures_util::StreamExt as _;
        futures_util::stream::pending().boxed()
    }

    fn accepts_images(&self) -> bool {
        false
    }
}

/// A turn the lease stops before the provider answers is counted as
/// stopped, never as failed.
#[tokio::test(start_paused = true)]
async fn a_stopped_turn_is_counted_as_stopped() {
    use afr_telemetry::labels::TurnOutcome;
    use afr_telemetry::testing::{Tally, scoped};

    let engine = super::Loop::new(afr_tools::Catalog::new(Vec::new()), Silent);
    let lease = lease(&[], unbounded());
    let stop = CancellationToken::new();
    let (tally, recorded) = Tally::new();
    let stopper = async {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        stop.cancel();
    };

    let (_ended, ()) = tokio::join!(scoped(tally, drive(&engine, &lease, &stop)), stopper);

    assert_eq!(turn_outcomes(&recorded), vec![TurnOutcome::Stopped]);
}

/// Drives a run whose model plays `turns` with an empty-tooled lease.
async fn outcome_of(turns: Vec<Vec<af_providers::Chunk>>) -> crate::engine::RunOutput {
    let script = crate::fixture::Script::new(turns);
    let engine = engine(Vec::new(), &script);
    let (output, _frames) =
        drive(&engine, &lease(&[], unbounded()), &CancellationToken::new()).await;
    output
}

#[tokio::test]
async fn a_first_turn_with_no_text_and_no_call_fails_the_run_instead_of_answering() {
    let output = outcome_of(vec![vec![]]).await;

    let ResultOutcome::Failed(failure) = output.result.outcome else {
        panic!("an empty first turn must not complete the run");
    };
    assert_eq!(
        failure.class, None,
        "the fleet's error carries no failure reason"
    );
    assert_eq!(
        failure.detail,
        "the model provider finished the turn with no answer"
    );
    assert_eq!(output.result.content, "", "a failed run reports no answer");
}

#[tokio::test]
async fn a_first_turn_of_only_whitespace_fails_the_run() {
    let output = outcome_of(vec![vec![say(" \n\t ")]]).await;

    let ResultOutcome::Failed(failure) = output.result.outcome else {
        panic!("whitespace is not an answer");
    };
    assert_eq!(
        failure.detail,
        "the model provider finished the turn with no answer"
    );
}

#[tokio::test]
async fn an_empty_first_turn_cut_at_its_limit_says_it_stopped_early() {
    let output = outcome_of(vec![vec![crate::fixture::ended(true)]]).await;

    let ResultOutcome::Failed(failure) = output.result.outcome else {
        panic!("a cut empty turn must not complete the run");
    };
    assert_eq!(
        failure.detail,
        "the model provider stopped the turn early with no answer"
    );
}

#[tokio::test]
async fn a_short_non_ascii_answer_still_completes() {
    let output = outcome_of(vec![vec![say("✓ 東京")]]).await;

    assert_eq!(
        output.result.outcome,
        ResultOutcome::Completed(crate::Completed)
    );
    assert_eq!(output.result.content, "✓ 東京");
}

#[tokio::test]
async fn a_closing_turn_without_text_after_a_tool_call_still_completes() {
    use afr_tools::catalog::UPDATE_PLAN;
    let script = crate::fixture::Script::new([
        vec![crate::fixture::call(
            "p1",
            UPDATE_PLAN.name(),
            serde_json::json!({"operation": "add"}),
        )],
        vec![],
    ]);
    let engine = engine(
        vec![crate::fixture::Canned::boxed(&UPDATE_PLAN, "done")],
        &script,
    );
    let (output, _frames) = drive(
        &engine,
        &lease(&[UPDATE_PLAN.name()], unbounded()),
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(
        output.result.outcome,
        ResultOutcome::Completed(crate::Completed),
        "a run that acted through a tool has done its work"
    );
    assert_eq!(output.result.content, "");
}
