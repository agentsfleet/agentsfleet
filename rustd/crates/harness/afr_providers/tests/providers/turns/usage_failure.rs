//! A paid partial turn never becomes a successful answer or another physical request.

use super::*;

#[tokio::test]
async fn known_input_usage_leaves_before_failure_without_reopening() {
    let wire = Wire::Messages;
    let Reply::Stream(mut frames) = wire.answer(ANSWER) else {
        unreachable!("script streams")
    };
    frames.truncate(1);
    let mut fake = Fake::serve(vec![Reply::Stream(frames), wire.answer(ANSWER)]).await;
    let items = streamed(&fake, wire, &asked()).await;
    let [Ok(Chunk::Usage(usage)), Err(failure)] = items.as_slice() else {
        panic!("usage then failure, never End: {items:?}")
    };
    assert_eq!(
        usage.input,
        super::super::PROMPT_TOKENS - super::super::CACHED_TOKENS
    );
    assert_eq!(usage.cached_input, super::super::CACHED_TOKENS);
    assert_eq!(usage.output, 0);
    assert_eq!(
        failure.failure_class(),
        Some(afd_wire::report::FailureClass::TransportLoss)
    );
    assert_eq!(fake.seen().len(), 1);
}

#[tokio::test]
async fn known_completion_usage_survives_a_transport_close_without_an_end_frame() {
    let wire = Wire::Chat;
    let Reply::Stream(mut frames) = wire.answer(ANSWER) else {
        unreachable!("script streams")
    };
    // Keep text and usage, but remove both successful termination markers.
    frames.remove(1);
    frames.pop();
    let mut fake = Fake::serve(vec![Reply::Stream(frames)]).await;
    let items = streamed(&fake, wire, &asked()).await;
    assert!(
        matches!(items.as_slice(), [Ok(Chunk::Text {..}), Ok(Chunk::Usage(usage)), Err(_)] if usage.output == 3)
    );
    assert_eq!(fake.seen().len(), 1);
}

#[tokio::test]
async fn reasoning_usage_without_text_prevents_reopening_a_truncated_turn() {
    let wire = Wire::Chat;
    let frames = vec![
        "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\"},\"finish_reason\":null}]}\n\n".into(),
        "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"choices\":[],\"usage\":{\"completion_tokens_details\":{\"reasoning_tokens\":7}}}\n\n".into(),
    ];
    let mut fake = Fake::serve(vec![Reply::Stream(frames), wire.answer(ANSWER)]).await;
    let items = streamed(&fake, wire, &asked()).await;
    let [Ok(Chunk::Usage(_)), Err(failure)] = items.as_slice() else {
        panic!("reported spend then failure, never End: {items:?}")
    };
    assert_eq!(
        failure.failure_class(),
        Some(afd_wire::report::FailureClass::TransportLoss)
    );
    assert_eq!(
        fake.seen().len(),
        1,
        "reported spend must prevent a second send"
    );
}
