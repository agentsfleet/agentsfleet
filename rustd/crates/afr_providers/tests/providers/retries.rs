//! What a retried turn counts: the runner's own retry family, labelled with
//! the provider the turn reached and why it was sent again.

use afr_telemetry::labels::{Provider, RetryReason, TurnOutcome};
use afr_telemetry::testing::{Recorded, Tally, scoped};

use afd_wire::report::ResultOutcome;

use super::support::wires::Wire;
use super::support::{Fake, Reply, engine, lease, run};
use super::{ANSWER, ATTEMPTS, RETRY_AFTER};

/// One 429 then success counts one retry, rate limited, under the provider's
/// own name, and one completed turn: the retry is a send, not a second turn.
#[tokio::test]
async fn test_provider_retry_is_counted() {
    let wire = Wire::Messages;
    let fake = Fake::serve(vec![
        Reply::Status {
            status: 429,
            retry_after: Some(RETRY_AFTER),
        },
        wire.answer(ANSWER),
    ])
    .await;
    let provider = wire.provider();
    let leased = lease(&provider, &[], "hello");
    let (tally, recorded) = Tally::new();

    let (output, _frames) = scoped(tally, run(&engine(&fake), &leased)).await;

    assert_eq!(output.result.content, ANSWER);
    let label = Provider::of(&provider);
    assert_eq!(
        label.as_str(),
        "anthropic",
        "the registry's provider, by the registry's name"
    );
    let recorded: Vec<Recorded> = recorded.try_iter().collect();
    let retries: Vec<&Recorded> = recorded
        .iter()
        .filter(|recorded| matches!(recorded, Recorded::Retry(..)))
        .collect();
    assert_eq!(
        retries,
        vec![&Recorded::Retry(label, RetryReason::RateLimited)]
    );
    let turns = recorded
        .iter()
        .filter(|recorded| matches!(recorded, Recorded::Turn(seen, TurnOutcome::Completed, _) if *seen == label))
        .count();
    assert_eq!(turns, 1, "{recorded:?}");
}

/// The retries `recorded` holds for `label` with `reason`.
fn retries(recorded: &[Recorded], label: Provider, reason: RetryReason) -> usize {
    recorded
        .iter()
        .filter(|seen| **seen == Recorded::Retry(label, reason))
        .count()
}

/// A stream cut before it showed anything is opened again, and each
/// reopening is one retry, `stream_reopened`, on every wire; the turn that
/// never finished is counted failed.
#[tokio::test]
async fn each_reopened_stream_is_one_retry() {
    for wire in Wire::ALL {
        let Reply::Stream(mut events) = wire.answer(ANSWER) else {
            unreachable!("an answer streams");
        };
        let silent = match wire {
            Wire::Messages | Wire::Responses => 1,
            Wire::Chat => 0,
        };
        events.truncate(silent);
        let fake = Fake::serve(vec![Reply::Stream(events); ATTEMPTS]).await;
        let provider = wire.provider();
        let leased = lease(&provider, &[], "hello");
        let (tally, recorded) = Tally::new();

        let (output, _frames) = scoped(tally, run(&engine(&fake), &leased)).await;

        assert!(
            matches!(output.result.outcome, ResultOutcome::Failed(_)),
            "{wire:?}"
        );
        let label = Provider::of(&provider);
        let recorded: Vec<Recorded> = recorded.try_iter().collect();
        assert_eq!(
            retries(&recorded, label, RetryReason::StreamReopened),
            ATTEMPTS - 1,
            "{wire:?}: every send after the first is one reopening: {recorded:?}"
        );
        assert!(
            recorded
                .iter()
                .any(|seen| matches!(seen, Recorded::Turn(turned, TurnOutcome::Failed, _) if *turned == label)),
            "{wire:?}: {recorded:?}"
        );
    }
}

/// A send that cannot connect is retried as a transport failure, with no
/// status to name a rate limit or a server error.
#[tokio::test]
async fn a_send_that_cannot_connect_is_retried_as_transport() {
    let wire = Wire::Messages;
    let mut fake = Fake::serve(Vec::new()).await;
    // Nothing listens on port 1, so every send is refused before it is sent.
    fake.base = "http://127.0.0.1:1".to_owned();
    let provider = wire.provider();
    let leased = lease(&provider, &[], "hello");
    let (tally, recorded) = Tally::new();

    let (output, _frames) = scoped(tally, run(&engine(&fake), &leased)).await;

    assert!(matches!(output.result.outcome, ResultOutcome::Failed(_)));
    let recorded: Vec<Recorded> = recorded.try_iter().collect();
    let label = Provider::of(&provider);
    assert!(
        retries(&recorded, label, RetryReason::Transport) >= 1,
        "a refused connection is retried as transport: {recorded:?}"
    );
    assert_eq!(
        retries(&recorded, label, RetryReason::RateLimited)
            + retries(&recorded, label, RetryReason::ServerError),
        0,
        "and never as a status it never got"
    );
}
