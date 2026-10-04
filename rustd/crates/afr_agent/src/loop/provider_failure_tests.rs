#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test module: a failed precondition should fail the test loudly"
)]

//! How a provider that refuses, drops, or cannot be reached ends the run.

use afd_wire::report::{FailureClass, ResultOutcome};
use afr_egress::testing::CountingMint;
use afr_providers::Error;
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
async fn a_provider_that_cannot_be_reached_is_an_engine_error() {
    let engine = super::Loop::new(afr_tools::Catalog::new(Vec::new()), Unreachable);
    let lease = lease(&[], unbounded());
    let frames = Frames::default();
    let sink = frames.sink();

    let failure = engine
        .run(AgentRun {
            lease: &lease,
            memory: afr_memory::Seed::default(),
            executor: None,
            mint: &CountingMint::never(),
            checkpoint: &Discard,
            events: &sink,
            meter: &Meter::default(),
            stop: &CancellationToken::new(),
        })
        .await
        .unwrap_err();

    assert_eq!(
        failure.code(),
        afd_core::error_code::INTERNAL_OPERATION_FAILED
    );
    assert!(frames.taken().is_empty());
}
