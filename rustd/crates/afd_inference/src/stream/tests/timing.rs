//! Dropping a pending poll must not renew a silent provider's lifetime.

use super::*;

#[tokio::test(start_paused = true)]
async fn cancelled_next_poll_preserves_the_original_inactivity_deadline() {
    let (_tx, body) = channel();
    let started = Instant::now();
    let (mut turn, physical) = open(body, TurnControl::default());
    tokio::time::timeout(Duration::from_secs(1), turn.next())
        .await
        .unwrap_err();
    tokio::time::advance(READ_TIMEOUT.checked_sub(Duration::from_secs(1)).unwrap()).await;
    assert!(matches!(
        turn.next().await,
        Some(Err(FailureCause::Deadline))
    ));
    assert_eq!(started.elapsed(), READ_TIMEOUT);
    assert_eq!(physical.sends.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn clean_eof_without_a_provider_end_keeps_usage_but_refuses_completion() {
    let wire = format!("{TEXT}{USAGE}");
    let (turn, physical) = open(
        Box::pin(stream::once(async move { Ok(Bytes::from(wire)) })),
        TurnControl::default(),
    );
    let result = turn.finish().await;
    assert_eq!(result.cause, Some(FailureCause::Provider));
    assert!(result.failure.is_some());
    assert_eq!(result.response.usage.input_tokens, Some(3));
    assert_eq!(result.response.usage.output_tokens, Some(2));
    assert!(matches!(
        *result.into_response().unwrap_err(),
        ProviderError::Truncated
    ));
    assert_eq!(physical.sends.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn a_failed_sdk_fold_keeps_partial_usage_and_emits_one_failure() {
    let wire = format!("{TEXT}{USAGE}{STOP}");
    let (turn, _) = open(
        Box::pin(stream::once(async move { Ok(Bytes::from(wire)) })),
        TurnControl::default(),
    );
    let partial = turn.finish().await.response;
    let captured = afd_core::test_util::trace::Capture::install();
    let (turn, physical) = open(Box::pin(stream::empty()), TurnControl::default());
    let result = finish_response(
        Err(ProviderError::Truncated),
        partial,
        turn.recovery,
        turn.trace,
    )
    .await;
    assert_eq!(result.cause, Some(FailureCause::Provider));
    assert!(matches!(
        result.failure.as_deref(),
        Some(ProviderError::Truncated)
    ));
    assert_eq!(result.response.usage.input_tokens, Some(3));
    assert_eq!(result.response.usage.output_tokens, Some(2));
    assert_eq!(
        captured.only("model_turn_failed").field("reason"),
        Some("provider_interrupted")
    );
    assert!(matches!(
        *result.into_response().unwrap_err(),
        ProviderError::Truncated
    ));
    assert_eq!(physical.sends.load(Ordering::Relaxed), 0);
}
