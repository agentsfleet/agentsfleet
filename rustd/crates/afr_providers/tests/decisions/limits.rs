//! Serialized-byte limits include escaping and complete combined questions.

use afd_wire::decision::{DecisionApi, DecisionOutcome, DecisionQuestion, NoulCriteria};
use afr_providers::decision::{DecisionClient, DecisionEvaluator as _};
use garde::Unvalidated;
use serde_json::Value;

use super::fixture::{self, Reply, Server};

pub(super) async fn escaped_state_and_complete_questions_keep_their_bounds() {
    let mut server = Server::start(vec![Reply::Json(fixture::native_reply())]).await;
    // 8,191 escaped newlines plus the two JSON quotes occupy exactly 16 KiB.
    let state = "\n".repeat(8_191);
    let mut request = fixture::request().into_inner();
    request.state = Value::String(state.clone());
    let request = Unvalidated::new(request)
        .validate()
        .expect("exact state bound");
    let result = DecisionClient::with_client(
        server.client.clone(),
        fixture::profile(&server, DecisionApi::Typesafe),
        fixture::credential(),
    )
    .evaluate(&request)
    .await
    .expect("complete bounded state is sent");
    assert!(matches!(result.result, DecisionOutcome::Answered { .. }));
    let seen = server.next().await;
    assert_eq!(
        seen.body["state"].as_str(),
        Some(state.as_str()),
        "no truncation"
    );

    let mut oversized = request.into_inner();
    oversized.state = Value::String(format!("{state}\n"));
    assert!(
        Unvalidated::new(oversized).validate().is_err(),
        "escaping counts against the limit"
    );

    let mut oversized_question = fixture::request().into_inner();
    oversized_question.questions[0].question = DecisionQuestion::Noul {
        instructions: "q".repeat(512),
        criteria: NoulCriteria {
            yes: "y".repeat(512),
            no: "n".repeat(512),
        },
    };
    assert!(
        Unvalidated::new(oversized_question).validate().is_err(),
        "individually valid fields exceed the combined question bound"
    );
    assert_eq!(
        server.unconsumed(),
        0,
        "rejected data never becomes another request"
    );
    server.shutdown().await;
}
