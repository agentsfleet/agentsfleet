//! Fixed roles and unique statement identities are proved once at setup parse.

use std::collections::BTreeSet;

use super::{
    ASSOCIATION_CRITERIA, CompletionQuestions, CompletionStatement, DecisionPolicy, INTENT_OPTIONS,
    NEXT_ACTION_OPTIONS, NOUL_CRITERIA, Question, RoutingQuestions, bounds, raw,
};
use crate::error::{Error, ErrorKind, Result, missing};

const REASON_CRITERIA: &str = "decision question criteria must exactly match its native role";
const REASON_DUPLICATE: &str = "completion statements must have unique stable IDs";

impl TryFrom<raw::Policy> for DecisionPolicy {
    type Error = Error;

    fn try_from(value: raw::Policy) -> Result<Self> {
        let routing = value
            .routing
            .ok_or_else(|| missing("decision_policy.routing"))?;
        let completion = value
            .completion
            .ok_or_else(|| missing("decision_policy.completion"))?;
        Ok(Self {
            routing: RoutingQuestions {
                intent: question(
                    routing.intent,
                    "decision_policy.routing.intent",
                    &INTENT_OPTIONS,
                )?,
                association: question(
                    routing.association,
                    "decision_policy.routing.association",
                    &ASSOCIATION_CRITERIA,
                )?,
                multiple_requests: question(
                    routing.multiple_requests,
                    "decision_policy.routing.multiple_requests",
                    &NOUL_CRITERIA,
                )?,
            },
            completion: CompletionQuestions {
                statements: statements(completion.statements)?,
                next_action: question(
                    completion.next_action,
                    "decision_policy.completion.next_action",
                    &NEXT_ACTION_OPTIONS,
                )?,
            },
        })
    }
}

fn question(value: Option<raw::Question>, field: &'static str, keys: &[&str]) -> Result<Question> {
    let value = value.ok_or_else(|| missing(field))?;
    bounds::question(&value)?;
    let criteria = value
        .criteria
        .ok_or_else(|| missing("decision_policy.question.criteria"))?;
    if criteria.len() != keys.len() || keys.iter().any(|key| !criteria.contains_key(*key)) {
        return Err(ErrorKind::InvalidDefinition {
            reason: REASON_CRITERIA,
        }
        .into());
    }
    Ok(Question {
        question: value
            .question
            .ok_or_else(|| missing("decision_policy.question.question"))?
            .into(),
        criteria: criteria
            .into_iter()
            .map(|(key, value)| (key.into(), value.into()))
            .collect(),
    })
}

fn statements(values: Option<Vec<raw::Statement>>) -> Result<Box<[CompletionStatement]>> {
    let values = values.ok_or_else(|| missing("decision_policy.completion.statements"))?;
    bounds::statements(&values)?;
    let mut seen = BTreeSet::new();
    for value in &values {
        bounds::question(value)?;
        if let Some(id) = &value.id
            && !seen.insert(id)
        {
            return Err(ErrorKind::InvalidDefinition {
                reason: REASON_DUPLICATE,
            }
            .into());
        }
    }
    values
        .into_iter()
        .map(|value| {
            Ok(CompletionStatement {
                id: value
                    .id
                    .ok_or_else(|| missing("decision_policy.completion.statement.id"))?
                    .into(),
                question: question(
                    Some(raw::Question {
                        question: value.question,
                        criteria: value.criteria,
                    }),
                    "decision_policy.completion.statement",
                    &NOUL_CRITERIA,
                )?,
            })
        })
        .collect()
}
