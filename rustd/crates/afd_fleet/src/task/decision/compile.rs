//! Translate accepted owner text once; native aliases never rewrite its criteria.

use afd_fleet_runtime::config::decision::{
    ASSOCIATION_CRITERIA, DecisionPolicy, NOUL_CRITERIA, Question,
};
use afd_wire::decision::{ChoiceOption, DecisionQuestion, NamedQuestion, NoulCriteria};

use super::prepare::Candidate;
use crate::{Result, task::TaskRefusal};

pub(super) fn routing(
    policy: &DecisionPolicy,
    candidates: &[Candidate],
    whole_message: bool,
) -> Result<Vec<NamedQuestion>> {
    let routing = policy.routing();
    let mut questions = vec![choice("intent", routing.intent())];
    let mut options = Vec::with_capacity(candidates.len() + 2);
    for (key, description) in routing.association().criteria() {
        if key == ASSOCIATION_CRITERIA[0] {
            options.extend(candidates.iter().map(|candidate| ChoiceOption {
                value: candidate.alias.clone(),
                description: description.to_owned(),
            }));
        } else {
            options.push(ChoiceOption {
                value: key.to_owned(),
                description: description.to_owned(),
            });
        }
    }
    questions.push(NamedQuestion {
        name: "association".to_owned(),
        question: DecisionQuestion::Choice {
            instructions: routing.association().text().to_owned(),
            options,
        },
    });
    if whole_message {
        questions.push(noul(
            "multiple_requests".to_owned(),
            routing.multiple_requests(),
        )?);
    }
    Ok(questions)
}

pub(super) fn completion(policy: &DecisionPolicy) -> Result<Vec<NamedQuestion>> {
    policy
        .completion()
        .statements()
        .iter()
        .map(|statement| noul(format!("goal.{}", statement.id()), statement.question()))
        .collect()
}

pub(super) fn next_action(policy: &DecisionPolicy) -> NamedQuestion {
    choice("next_action", policy.completion().next_action())
}

fn choice(name: &str, question: &Question) -> NamedQuestion {
    NamedQuestion {
        name: name.to_owned(),
        question: DecisionQuestion::Choice {
            instructions: question.text().to_owned(),
            options: question
                .criteria()
                .map(|(value, description)| ChoiceOption {
                    value: value.to_owned(),
                    description: description.to_owned(),
                })
                .collect(),
        },
    }
}

fn noul(name: String, question: &Question) -> Result<NamedQuestion> {
    Ok(NamedQuestion {
        name,
        question: DecisionQuestion::Noul {
            instructions: question.text().to_owned(),
            criteria: NoulCriteria {
                yes: criterion(question, NOUL_CRITERIA[0])?,
                no: criterion(question, NOUL_CRITERIA[1])?,
            },
        },
    })
}

fn criterion(question: &Question, key: &str) -> Result<String> {
    question
        .criterion(key)
        .map(str::to_owned)
        .ok_or_else(|| TaskRefusal::DecisionUnavailable.into())
}
