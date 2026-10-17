//! Canonical history remains authoritative over provider replay metadata.

use super::{
    ANSWER, CALL_ID, Chunk, Fake, Message, Reply, UPDATE_PLAN, Wire, asked, chunks, json, streamed,
};

#[tokio::test]
async fn removing_a_tool_call_cannot_restore_its_native_provider_item() {
    let wire = Wire::Responses;
    let private = "removed-private-arguments";
    let mut fake = Fake::serve(vec![
        wire.call(CALL_ID, UPDATE_PLAN.name(), &json!({"expression":private})),
        wire.answer(ANSWER),
    ])
    .await;
    let first = chunks(streamed(&fake, wire, &asked()).await);
    assert!(
        first
            .iter()
            .any(|chunk| matches!(chunk, Chunk::Call(call) if call.id == CALL_ID))
    );
    let Some(Chunk::End(end)) = first.into_iter().last() else {
        panic!("the provider's tool turn must finish");
    };
    let messages = [
        Message::User("Start".into()),
        Message::Assistant {
            text: "Tool call removed".into(),
            calls: Vec::new(),
            replay: end.replay,
        },
        Message::User("Continue".into()),
    ];
    let next = chunks(streamed(&fake, wire, &messages).await);
    assert!(matches!(next.last(), Some(Chunk::End(_))));
    let seen = fake.seen();
    assert_eq!(seen.len(), 2);
    let input = seen[1].body["input"].as_array().unwrap();
    assert!(input.iter().all(|item| item["type"] != "function_call"));
    let sent = seen[1].body.to_string();
    assert!(sent.contains("Tool call removed"));
    assert!(!sent.contains(private) && !sent.contains(CALL_ID));
}

#[tokio::test]
async fn an_unknown_stream_event_keeps_the_answer_and_does_not_reopen_the_turn() {
    let wire = Wire::Responses;
    let Reply::Stream(mut events) = wire.answer(ANSWER) else {
        panic!("the provider answer streams");
    };
    events.insert(1, "event: response.future_event\ndata: {\"type\":\"response.future_event\",\"value\":\"private-diagnostic\"}\n\n".into());
    let mut fake = Fake::serve(vec![Reply::Stream(events)]).await;
    let output = chunks(streamed(&fake, wire, &asked()).await);
    assert!(
        output
            .iter()
            .any(|chunk| matches!(chunk, Chunk::Text { text, .. } if text == ANSWER))
    );
    assert!(matches!(
        output.as_slice(),
        [Chunk::Text { .. }, Chunk::Usage(_), Chunk::End(_)]
    ));
    assert!(!format!("{output:?}").contains("private-diagnostic"));
    assert_eq!(fake.seen().len(), 1);
}
