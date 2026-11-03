//! `TypeSafe`, Perplexity and Clef share named System One question primitives.

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
    let questions: BTreeMap<_, _> = request
        .questions
        .iter()
        .map(|named| {
            let value = match &named.question {
                DecisionQuestion::Noul {
                    instructions,
                    criteria,
                } => json!({"type":"noul", "instructions":instructions, "criteria":criteria}),
                DecisionQuestion::Choice {
                    instructions,
                    options,
                } => {
                    let criteria: BTreeMap<_, _> = options
                        .iter()
                        .map(|option| (&option.value, &option.description))
                        .collect();
                    json!({"type":"choice", "instructions":instructions, "criteria":criteria})
                }
                DecisionQuestion::Score {
                    instructions,
                    levels,
                } => json!({"type":"score", "instructions":instructions, "criteria":levels}),
            };
            (&named.name, value)
        })
        .collect();
    json!({"model":profile.model, "state":request.state, "questions":questions})
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum RawAnswer {
    Noul {
        noul: f64,
    },
    Choice {
        choice: String,
        confidence: Option<f64>,
        probabilities: BTreeMap<String, f64>,
    },
    Score {
        score: f64,
        confidence: Option<f64>,
        probabilities: BTreeMap<String, f64>,
        legend: BTreeMap<String, Value>,
    },
    Refusal,
}

pub(super) fn answers(request: &Valid<DecisionRequest>, value: &Value) -> DecisionOutcome {
    let Some(values) = value
        .as_object()
        .filter(|values| values.len() == request.questions.len())
    else {
        return invalid();
    };
    let mut answers = BTreeMap::new();
    for named in &request.questions {
        let Some(value) = values.get(&named.name) else {
            return invalid();
        };
        let Ok(raw) = RawAnswer::deserialize(value) else {
            return invalid();
        };
        if matches!(raw, RawAnswer::Refusal) {
            return DecisionOutcome::Refused;
        }
        let Some(answer) = raw.normalize(&named.question) else {
            return invalid();
        };
        if !validate::answer(&named.question, &answer) {
            return invalid();
        }
        answers.insert(named.name.clone(), answer);
    }
    DecisionOutcome::Answered { answers }
}

impl RawAnswer {
    fn normalize(self, question: &DecisionQuestion) -> Option<DecisionAnswer> {
        match self {
            Self::Noul { noul } => Some(DecisionAnswer::Noul { noul }),
            Self::Choice {
                choice,
                confidence,
                probabilities,
            } => Some(DecisionAnswer::Choice {
                choice,
                confidence,
                probabilities,
            }),
            Self::Score {
                score,
                confidence,
                probabilities,
                legend,
            } => {
                let DecisionQuestion::Score { levels, .. } = question else {
                    return None;
                };
                let probabilities = indexed(probabilities)?;
                let legend = indexed(legend)?;
                legend_matches(levels, &legend).then_some(DecisionAnswer::Score {
                    score,
                    confidence,
                    probabilities,
                })
            }
            Self::Refusal => None,
        }
    }
}

fn indexed<T>(values: BTreeMap<String, T>) -> Option<BTreeMap<u32, T>> {
    let mut indexed = BTreeMap::new();
    for (key, value) in values {
        let index = key.parse::<u32>().ok()?;
        if key != index.to_string() {
            return None;
        }
        indexed.insert(index, value);
    }
    Some(indexed)
}

fn legend_matches(levels: &[String], legend: &BTreeMap<u32, Value>) -> bool {
    legend.len() == levels.len()
        && levels.iter().enumerate().all(|(index, level)| {
            u32::try_from(index)
                .ok()
                .and_then(|index| legend.get(&index))
                .and_then(Value::as_str)
                == Some(level.as_str())
        })
}

fn invalid() -> DecisionOutcome {
    DecisionOutcome::Invalid {
        reason: DecisionInvalid::Answers,
    }
}
