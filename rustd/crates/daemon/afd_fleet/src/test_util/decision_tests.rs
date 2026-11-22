//! The synthetic receipt answers each question kind as its doc says.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the restriction set is for the daemon"
)]

use super::known_result;
use af_wire::decision::{
    DecisionAnswer, DecisionOutcome, DecisionPurpose, DecisionQuestion, DecisionRequest,
    NamedQuestion,
};
use std::collections::BTreeMap;

// Value: protects=a score question is answered at its lowest level with all
// certainty there; fails_when=the fixture answers a score off its rubric or
// spreads its probability; why_new=no fixture asked a score question;
// seam=the fixture's own answer builder.
#[test]
fn a_score_question_is_answered_at_its_lowest_level() {
    let request = DecisionRequest {
        call_id: "0190a8f0-0000-7000-8000-000000000000".to_owned(),
        purpose: DecisionPurpose::Goal,
        state: serde_json::json!({}),
        questions: vec![NamedQuestion {
            name: "progress".to_owned(),
            question: DecisionQuestion::Score {
                instructions: "How far along is the repair?".to_owned(),
                levels: vec!["none".to_owned(), "some".to_owned(), "done".to_owned()],
            },
        }],
    };

    let result = known_result(&request);

    let answers = match result.result {
        DecisionOutcome::Answered { answers } => Some(answers),
        _ => None,
    }
    .expect("the fixture always answers");
    assert_eq!(
        answers.get("progress"),
        Some(&DecisionAnswer::Score {
            score: 0.0,
            confidence: Some(1.0),
            probabilities: BTreeMap::from([(0, 1.0), (1, 0.0), (2, 0.0)]),
        })
    );
}
