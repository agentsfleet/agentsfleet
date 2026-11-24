//! What a goal check asks the decision model: the task's own completion
//! statements, and the one next-action question every check adds.
//!
//! The next-action wording is a constant: the question and the four criteria
//! of `docs/architecture/goal_evaluation.md`'s repair example. A fleet's model
//! writes the statements when it starts a task, and a person may edit them;
//! nobody writes the next action.

use af_fleet_runtime::config::decision::{CompletionStatement, NOUL_CRITERIA, Question};
use af_wire::decision::{ChoiceOption, DecisionQuestion, NamedQuestion, NoulCriteria};
use af_wire::durable_task::DurableTaskStatement;

use crate::{Result, durable_task::DurableTaskRefusal};

/// What the next-action question asks.
const NEXT_ACTION: &str = "Choose the next action from the accepted goal and current evidence. \
Required checks must finish successfully before completion.";

/// The four next actions, each with what makes it the right one.
const NEXT_ACTIONS: [(&str, &str); 4] = [
    (
        "CONTINUE",
        "Concrete corrective work remains and current authority and limits permit it.",
    ),
    (
        "WAIT",
        "A known required check is still running; no additional patch is currently justified.",
    ),
    (
        "DONE",
        "All accepted goal conditions have current evidence, including every required check at the current head.",
    ),
    (
        "BLOCKED",
        "A missing prerequisite, unresolved contradiction, permission or exhausted limit prevents authorized progress.",
    ),
];

/// `statements` read through the parser a stored statement list is read
/// with, so a list the verb admits is one the goal check reads back.
///
/// # Errors
/// A rejection for a statement list the parser refuses: none, a duplicated
/// id, or a bound a statement broke.
pub(in crate::durable_task) fn statements(
    statements: &[DurableTaskStatement],
) -> Result<Box<[CompletionStatement]>> {
    let document = serde_json::to_string(statements).map_err(af_core::json::Unreadable::from)?;
    CompletionStatement::parse_all(&document)
        .map_err(|_refused| crate::error::rejected(REFUSED_STATEMENTS))
}

/// The questions a task stores and a goal check asks, one per statement.
///
/// # Errors
/// A statement missing a criterion, which the parser does not admit.
pub(in crate::durable_task) fn questions(
    statements: &[CompletionStatement],
) -> Result<Vec<NamedQuestion>> {
    statements
        .iter()
        .map(|statement| noul(format!("goal.{}", statement.id()), statement.question()))
        .collect()
}

/// Why a statement list was refused.
const REFUSED_STATEMENTS: &str =
    "statements must be at least one, each with a unique id, a question and both criteria";

/// The constant question every goal check ends with.
pub(super) fn next_action() -> NamedQuestion {
    NamedQuestion {
        name: "next_action".to_owned(),
        question: DecisionQuestion::Choice {
            instructions: NEXT_ACTION.to_owned(),
            options: NEXT_ACTIONS
                .iter()
                .map(|(value, description)| ChoiceOption {
                    value: (*value).to_owned(),
                    description: (*description).to_owned(),
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
        .ok_or_else(|| DurableTaskRefusal::DecisionUnavailable.into())
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "a refused fixture fails the proof loudly"
)]
mod tests {
    use af_wire::decision::{DecisionQuestion, NoulCriteria};
    use af_wire::durable_task::DurableTaskStatement;

    use super::{next_action, questions, statements};

    fn completion(
        list: &[DurableTaskStatement],
    ) -> crate::Result<Vec<af_wire::decision::NamedQuestion>> {
        questions(&statements(list)?)
    }

    fn statement(id: &str) -> DurableTaskStatement {
        DurableTaskStatement {
            id: id.to_owned(),
            question: "Are the checks green?".to_owned(),
            criteria: NoulCriteria {
                yes: "Every required check passed.".to_owned(),
                no: "A required check failed or is missing.".to_owned(),
            },
        }
    }

    #[test]
    fn a_tasks_statements_become_its_named_goal_questions() {
        let questions = completion(&[statement("pr_green")]).expect("one statement");

        assert_eq!(questions.len(), 1);
        let named = questions.first().expect("one question");
        assert_eq!(named.name, "goal.pr_green");
        assert!(matches!(
            &named.question,
            DecisionQuestion::Noul { instructions, criteria }
                if instructions == "Are the checks green?"
                    && criteria.yes == "Every required check passed."
        ));
    }

    #[test]
    fn no_statement_or_a_repeated_id_is_refused() {
        for statements in [vec![], vec![statement("same"), statement("same")]] {
            let refused = completion(&statements).expect_err("an unusable statement list");
            assert_eq!(
                refused.code(),
                af_core::error_code::INVALID_REQUEST,
                "{statements:?}"
            );
        }
    }

    #[test]
    fn the_next_action_is_a_choice_of_the_four_documented_actions() {
        let values: Vec<String> = match next_action().question {
            DecisionQuestion::Choice { options, .. } => {
                options.into_iter().map(|option| option.value).collect()
            }
            DecisionQuestion::Noul { .. } | DecisionQuestion::Score { .. } => Vec::new(),
        };

        assert_eq!(values, ["CONTINUE", "WAIT", "DONE", "BLOCKED"]);
    }
}
