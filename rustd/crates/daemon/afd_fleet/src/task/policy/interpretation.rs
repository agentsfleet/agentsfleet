//! Validate the complete interpretation before target reads or objective allocations.

use af_fleet_runtime::config::decision::{DecisionPolicy, Question};
use af_wire::task::{TaskBindingItem, TaskBindingsInput, TaskInputAction};
use garde::Validate;

use super::{TaskPolicy, TaskSettings};
use crate::{Result, task::TaskRefusal};

#[derive(Validate)]
#[garde(context(TaskSettings as settings))]
struct Interpretation<'a> {
    #[garde(length(min = 1, max = settings.max_actions_per_message))]
    items: &'a [TaskBindingItem],
}

#[derive(Validate)]
#[garde(context(TaskSettings as settings))]
struct Title<'a> {
    #[garde(length(bytes, min = 1, max = settings.max_title_bytes))]
    text: &'a str,
}

#[derive(Validate)]
#[garde(context(TaskSettings as settings))]
struct Objective<'a> {
    #[garde(length(bytes, min = 1, max = settings.max_objective_bytes))]
    text: &'a str,
}

#[derive(Validate)]
#[garde(context(TaskSettings as settings))]
struct Completion<'a> {
    #[garde(length(max = settings.max_completion_questions))]
    statements: &'a [af_fleet_runtime::config::decision::CompletionStatement],
}

#[derive(Validate)]
#[garde(context(TaskSettings as settings))]
struct AuthoredQuestion<'a> {
    #[garde(length(bytes, min = 1, max = settings.max_question_text_bytes))]
    text: &'a str,
    #[garde(custom(authored_size))]
    document: &'a Question,
}

impl TaskPolicy {
    pub(in crate::task) fn interpretation<'m>(
        &self,
        input: &TaskBindingsInput,
        message: &'m str,
        decision: &DecisionPolicy,
    ) -> Result<Vec<&'m str>> {
        Interpretation {
            items: &input.items,
        }
        .validate_with(&self.settings)
        .map_err(|_report| TaskRefusal::LimitExhausted)?;
        let mut end = 0_i32;
        let mut objectives = Vec::with_capacity(input.items.len());
        for item in &input.items {
            let proposed = &item.proposed;
            if proposed.source_start < end || proposed.source_end <= proposed.source_start {
                return Err(TaskRefusal::InputAmbiguous.into());
            }
            let span = usize::try_from(proposed.source_start.unsigned_abs()).unwrap_or(usize::MAX)
                ..usize::try_from(proposed.source_end.unsigned_abs()).unwrap_or(usize::MAX);
            let objective = message.get(span).ok_or(TaskRefusal::InputAmbiguous)?;
            Title {
                text: &proposed.title,
            }
            .validate_with(&self.settings)
            .map_err(|_report| TaskRefusal::LimitExhausted)?;
            if proposed.action == TaskInputAction::NewWork {
                Objective { text: objective }
                    .validate_with(&self.settings)
                    .map_err(|_report| TaskRefusal::LimitExhausted)?;
            }
            end = proposed.source_end;
            objectives.push(objective);
        }
        if input
            .items
            .iter()
            .any(|item| item.proposed.action == TaskInputAction::NewWork)
        {
            self.completion(decision)?;
        }
        Ok(objectives)
    }

    fn completion(&self, policy: &DecisionPolicy) -> Result<()> {
        let statements = policy.completion().statements();
        Completion { statements }
            .validate_with(&self.settings)
            .map_err(|_report| TaskRefusal::LimitExhausted)?;
        for statement in statements {
            self.question(statement.question())?;
        }
        Ok(())
    }

    fn question(&self, question: &Question) -> Result<()> {
        AuthoredQuestion {
            text: question.text(),
            document: question,
        }
        .validate_with(&self.settings)
        .map_err(|_report| TaskRefusal::LimitExhausted)?;
        Ok(())
    }
}

fn authored_size(question: &Question, settings: &TaskSettings) -> garde::Result {
    if af_validate::serialized_within(question, settings.max_authored_question_bytes) {
        Ok(())
    } else {
        Err(garde::Error::new(
            "authored question exceeds the configured byte limit",
        ))
    }
}
