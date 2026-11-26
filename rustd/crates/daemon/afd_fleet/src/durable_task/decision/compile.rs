//! What a goal check asks the decision model: the task's own completion
//! statements, and the one next-action question every check adds.
//!
//! Each target has one next-action question with four options
//! (`docs/architecture/durable_tasks.md` §"Goal evaluation"). A fleet's model
//! writes the statements when it starts a task, and a person may edit them;
//! nobody writes the next action.

use af_fleet_runtime::config::decision::{CompletionStatement, NOUL_CRITERIA, Question};
use af_wire::decision::{ChoiceOption, DecisionQuestion, NamedQuestion, NoulCriteria};
use af_wire::durable_task::{DurableTaskStatement, DurableTaskTarget};

use crate::{Result, durable_task::DurableTaskRefusal};

/// What the next-action question asks for a bound pull request.
const PR_NEXT_ACTION: &str = "Choose the next action from the accepted goal and current evidence. \
Required checks must finish successfully before completion.";
const API_NEXT_ACTION: &str = "Choose the next action based on the accepted goal statements and current evidence. \
If the evidence satisfies every accepted goal statement, choose DONE. This task has no bound pull request. \
Its required completion checks are the accepted goal statements.";
const PR_DONE: &str = "All accepted goal conditions have current evidence, including every required check at the current head.";
const API_DONE: &str = "Current evidence satisfies every accepted goal statement.";

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

/// The question every goal check ends with, using the task's native target.
pub(super) fn next_action(target: &DurableTaskTarget) -> NamedQuestion {
    let (instructions, done) = match target {
        DurableTaskTarget::ApiOnly => (API_NEXT_ACTION, API_DONE),
        DurableTaskTarget::ExistingPr { .. } => (PR_NEXT_ACTION, PR_DONE),
    };
    NamedQuestion {
        name: "next_action".to_owned(),
        question: DecisionQuestion::Choice {
            instructions: instructions.to_owned(),
            options: [
                ("CONTINUE", "An accepted goal statement is unmet and authorized work can satisfy it."),
                ("WAIT", "A required validation check is running on the bound pull request head."),
                ("DONE", done),
                ("BLOCKED", "A missing prerequisite, unresolved contradiction, permission or exhausted limit prevents authorized progress."),
            ]
            .into_iter()
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
        .ok_or_else(|| DurableTaskRefusal::DecisionUnavailable.into())
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "a refused fixture fails the proof loudly"
)]
mod tests {
    use af_wire::decision::{ChoiceOption, DecisionQuestion, NoulCriteria};
    use af_wire::durable_task::{DurableTaskStatement, DurableTaskTarget};

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
    fn an_api_only_goal_uses_its_statements_as_completion_evidence() {
        let (instructions, options) = next_action_choices(&DurableTaskTarget::ApiOnly);
        let values: Vec<&str> = options.iter().map(|option| option.value.as_str()).collect();
        assert_eq!(values, ["CONTINUE", "WAIT", "DONE", "BLOCKED"]);
        assert!(instructions.contains("This task has no bound pull request."));
        assert_eq!(
            done_option(&options).description,
            "Current evidence satisfies every accepted goal statement."
        );
    }

    #[test]
    fn a_bound_pull_request_requires_validation_at_its_current_head() {
        let target = DurableTaskTarget::ExistingPr {
            repository: "example/fixture".to_owned(),
            repository_id: 101,
            number: 1,
            branch: "fix".to_owned(),
            base: "main".to_owned(),
            head: "0".repeat(40),
            tree: "1".repeat(40),
            url: "https://github.com/example/fixture/pull/1".to_owned(),
            workflows: Vec::new(),
        };
        let (instructions, options) = next_action_choices(&target);
        assert!(
            instructions.contains("Required checks must finish successfully before completion.")
        );
        assert!(
            done_option(&options)
                .description
                .contains("every required check at the current head")
        );
    }

    fn next_action_choices(target: &DurableTaskTarget) -> (String, Vec<ChoiceOption>) {
        match next_action(target).question {
            DecisionQuestion::Choice {
                instructions,
                options,
            } => Some((instructions, options)),
            DecisionQuestion::Noul { .. } | DecisionQuestion::Score { .. } => None,
        }
        .expect("a goal check must ask for a choice")
    }

    fn done_option(options: &[ChoiceOption]) -> &ChoiceOption {
        options
            .iter()
            .find(|option| option.value == "DONE")
            .expect("the goal can be done")
    }
}
