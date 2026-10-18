//! Rig's standard mocks: unknown items stay invisible and terminal failures survive.

use afd_wire::report::FailureClass;
use rig_core::test_utils::{MOCK_MODEL, MOCK_PROVIDER, MockCompletionModel, MockStreamEvent};

use super::*;

const ANSWER: &str = "The build passed.";
const FAILURE: &str = "retained terminal failure";

fn provider(events: Vec<MockStreamEvent>) -> Turns {
    let model = MockCompletionModel::from_stream_turns([events]).erase();
    Turns::new(model, Wire::Responses, "lease", MOCK_PROVIDER)
}

fn request() -> Request<'static> {
    Request {
        model: MOCK_MODEL,
        instructions: "Read the build result.",
        messages: &[],
        tools: &[],
        hosted: &[],
        cache_key: "fleet",
    }
}

#[tokio::test]
async fn an_unmodeled_stream_item_never_becomes_answer_text() {
    let turns = provider(vec![
        MockStreamEvent::unknown(serde_json::json!({"provider_private_item": true})),
        MockStreamEvent::text(ANSWER),
        MockStreamEvent::final_response_with_default_usage(),
    ]);
    let chunks: Vec<_> = turns.stream(request()).collect().await;

    assert!(matches!(chunks.as_slice(),
        [Ok(Chunk::Text { text, .. }), Ok(Chunk::Usage(_)), Ok(Chunk::End(_))]
        if text == ANSWER));
}

#[tokio::test]
async fn an_ended_stream_still_reports_its_recorded_failure() -> Result<()> {
    let turns = provider(vec![MockStreamEvent::error(FAILURE)]);
    let mut stream = turns.open(&request())?;
    assert!(matches!(stream.next().await, Some(Err(_))));
    let reading = Reading {
        attempt: 1,
        stream,
        shown: true,
    };

    assert!(matches!(reading.step(&turns).await,
        Step::Break((Err(error), Pass::Done))
        if error.failure_class() == Some(FailureClass::TransportLoss)
            && !error.detail().contains(FAILURE)));
    Ok(())
}
