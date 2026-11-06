//! Serialized-byte limits include escaping and complete combined questions.

use afd_wire::decision::{ChoiceOption, DecisionApi, DecisionOutcome, DecisionQuestion};
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

    complete_questions_keep_their_bound();
    assert_eq!(
        server.unconsumed(),
        0,
        "rejected data never becomes another request"
    );
    server.shutdown().await;
    encoded_question_refuses_before_connect().await;
}

async fn encoded_question_refuses_before_connect() {
    use afr_telemetry::{
        labels::decision::DecisionOutcome as MeasuredOutcome,
        testing::{Recorded, Tally, scoped},
    };
    let mut server = Server::start(vec![]).await;
    let request = expanded_score_request();
    let (tally, received) = Tally::new();
    let fault = scoped(
        tally,
        DecisionClient::with_client(
            server.client.clone(),
            fixture::profile(&server, DecisionApi::Openai),
            fixture::credential(),
        )
        .evaluate(&request),
    )
    .await
    .expect_err("native encoding proves the complete question bound");
    assert_eq!(
        fault.code(),
        afd_core::error_code::AGENTSFLEET_INVALID_CONFIG
    );
    assert!(fault.decision_not_sent());
    assert_eq!(
        (fault.decision_status(), fault.decision_retry_after()),
        (None, None)
    );
    assert!(matches!(
        received.try_iter().collect::<Vec<_>>().as_slice(),
        [Recorded::Decision(_, _, MeasuredOutcome::NotSent, _)]
    ));
    assert_eq!((server.connections(), server.unconsumed()), (0, 0));
    server.shutdown().await;
}

fn expanded_score_request() -> garde::Valid<afd_wire::decision::DecisionRequest> {
    let mut request = fixture::request().into_inner();
    request.questions.truncate(1);
    let question = request
        .questions
        .first_mut()
        .expect("complete score question");
    "risk".clone_into(&mut question.name);
    let mut levels = vec!["\u{001f}".repeat(512); 5];
    levels.push("r".repeat(422));
    question.question = DecisionQuestion::Score {
        instructions: "q".repeat(512),
        levels,
    };
    assert_eq!(
        serde_json::to_vec(question)
            .expect("complete compiled question")
            .len(),
        16_384
    );
    Unvalidated::new(request)
        .validate()
        .expect("typed request fits before native label expansion")
}

fn complete_questions_keep_their_bound() {
    let mut request = fixture::request().into_inner();
    let options = (0..16)
        .map(|index| format!("t{index}"))
        .chain(["new_work", "unclear"].map(str::to_owned))
        .map(|value| ChoiceOption {
            value,
            description: "r".repeat(512),
        })
        .collect();
    let question = &mut request
        .questions
        .first_mut()
        .expect("complete question set")
        .question;
    *question = DecisionQuestion::Choice {
        instructions: "Which task does this message refer to?".to_owned(),
        options,
    };
    let bytes = serde_json::to_vec(question)
        .expect("complete compiled question")
        .len();
    assert!(
        bytes > 1024 && bytes <= 16_384,
        "sixteen tasks and both reserved aliases remain intact"
    );
    let mut accepted = Unvalidated::new(request)
        .validate()
        .expect("a compiled association question can exceed the authored question limit")
        .into_inner();
    if let DecisionQuestion::Choice { options, .. } = &mut accepted
        .questions
        .first_mut()
        .expect("same complete question set")
        .question
    {
        for option in options {
            option.description = "\n".repeat(512);
        }
    }
    assert!(
        Unvalidated::new(accepted).validate().is_err(),
        "individually valid fields exceed the combined serialized question bound after escaping"
    );
}
