//! `OpenAI` Decisions uses named arrays and predicate rather than Noul.

use std::collections::BTreeMap;

use afd_wire::decision::{
    DecisionAnswer, DecisionInvalid, DecisionOutcome, DecisionProfile, DecisionQuestion,
    DecisionRequest,
};
use garde::Valid;
use serde::Deserialize;
use serde_json::{Value, json};

use super::super::validate;

pub(super) fn encode(profile: &Valid<DecisionProfile>, request: &Valid<DecisionRequest>) -> Value {
    let questions: Vec<_> = request.questions.iter().map(|named| match &named.question {
        DecisionQuestion::Noul { instructions, criteria } => json!({
            "type":"predicate", "name":named.name,
            "instructions":format!("{instructions}\nTrue: {}\nFalse: {}", criteria.yes, criteria.no),
        }),
        DecisionQuestion::Choice { instructions, options } => json!({
            "type":"choice", "name":named.name, "instructions":instructions, "choices":options,
        }),
        DecisionQuestion::Score { instructions, levels } => {
            let levels: Vec<_> = levels.iter().enumerate().map(|(index, description)| json!({"label":index.to_string(), "description":description})).collect();
            json!({"type":"score", "name":named.name, "instructions":instructions, "levels":levels})
        }
    }).collect();
    let input = if request.state.is_string() {
        request.state.clone()
    } else {
        json!([{"role":"user", "content":[{"type":"input_json", "data":request.state}]}])
    };
    json!({"model":profile.model, "input":input, "questions":questions})
}

#[derive(Deserialize)]
struct RawNamed {
    name: String,
    #[serde(flatten)]
    answer: RawAnswer,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum RawAnswer {
    Predicate {
        probability: f64,
    },
    Choice {
        choice: String,
        confidence: Option<f64>,
        probabilities: Vec<ChoiceProbability>,
    },
    Score {
        score: f64,
        confidence: Option<f64>,
        probabilities: Vec<ScoreProbability>,
    },
    Refusal,
}

#[derive(Deserialize)]
struct ChoiceProbability {
    value: String,
    probability: f64,
}
#[derive(Deserialize)]
struct ScoreProbability {
    value: u32,
    label: String,
    probability: f64,
}

pub(super) fn answers(request: &Valid<DecisionRequest>, value: &Value) -> DecisionOutcome {
    let Ok(values) = Vec::<RawNamed>::deserialize(value) else {
        return invalid();
    };
    if values.len() != request.questions.len() {
        return invalid();
    }
    let mut answers = BTreeMap::new();
    for named in values {
        let Some(question) = request
            .questions
            .iter()
            .find(|question| question.name == named.name)
        else {
            return invalid();
        };
        if matches!(named.answer, RawAnswer::Refusal) {
            return DecisionOutcome::Refused;
        }
        let Some(answer) = normalize(named.answer) else {
            return invalid();
        };
        if !validate::answer(&question.question, &answer)
            || answers.insert(named.name, answer).is_some()
        {
            return invalid();
        }
    }
    DecisionOutcome::Answered { answers }
}

fn normalize(raw: RawAnswer) -> Option<DecisionAnswer> {
    match raw {
        RawAnswer::Predicate { probability } => Some(DecisionAnswer::Noul { noul: probability }),
        RawAnswer::Choice {
            choice,
            confidence,
            probabilities,
        } => {
            let count = probabilities.len();
            let probabilities: BTreeMap<_, _> = probabilities
                .into_iter()
                .map(|point| (point.value, point.probability))
                .collect();
            (probabilities.len() == count).then_some(DecisionAnswer::Choice {
                choice,
                confidence,
                probabilities,
            })
        }
        RawAnswer::Score {
            score,
            confidence,
            probabilities,
        } => {
            if probabilities
                .iter()
                .any(|point| point.label != point.value.to_string())
            {
                return None;
            }
            let count = probabilities.len();
            let probabilities: BTreeMap<_, _> = probabilities
                .into_iter()
                .map(|point| (point.value, point.probability))
                .collect();
            (probabilities.len() == count).then_some(DecisionAnswer::Score {
                score,
                confidence,
                probabilities,
            })
        }
        RawAnswer::Refusal => None,
    }
}

fn invalid() -> DecisionOutcome {
    DecisionOutcome::Invalid {
        reason: DecisionInvalid::Answers,
    }
}
