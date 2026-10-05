//! What a retried turn counts: the runner's own retry family, labelled with
//! the provider the turn reached and why it was sent again.

use afr_telemetry::labels::{Provider, RetryReason, TurnOutcome};
use afr_telemetry::testing::{Recorded, Tally, scoped};

use super::support::wires::Wire;
use super::support::{Fake, Reply, engine, lease, run};
use super::{ANSWER, RETRY_AFTER};

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
        "the registry's provider, by its well-known name"
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
