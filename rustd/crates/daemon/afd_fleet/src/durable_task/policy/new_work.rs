//! A binding's deployment bounds, checked before any target read or write.

use af_fleet_runtime::config::decision::{CompletionStatement, Question};
use af_wire::durable_task::DurableTaskBindingItem;
use garde::Validate;

use super::{DurableTaskPolicy, DurableTaskSettings};
use crate::{Result, durable_task::DurableTaskRefusal};

#[derive(Validate)]
#[garde(context(DurableTaskSettings as settings))]
struct Items<'a> {
    #[garde(length(min = 1, max = settings.max_actions_per_message))]
    items: &'a [DurableTaskBindingItem],
}

#[derive(Validate)]
#[garde(context(DurableTaskSettings as settings))]
struct NewWork<'a> {
    #[garde(length(bytes, min = 1, max = settings.max_title_bytes))]
    title: &'a str,
    #[garde(length(bytes, min = 1, max = settings.max_objective_bytes))]
    objective: &'a str,
    #[garde(length(min = 1, max = settings.max_completion_questions))]
    statements: &'a [CompletionStatement],
}

#[derive(Validate)]
#[garde(context(DurableTaskSettings as settings))]
struct AuthoredQuestion<'a> {
    #[garde(length(bytes, min = 1, max = settings.max_question_text_bytes))]
    text: &'a str,
    #[garde(custom(authored_size))]
    document: &'a Question,
}

impl DurableTaskPolicy {
    /// Refuses a plan with more items than one message may carry.
    pub(in crate::durable_task) fn items(&self, items: &[DurableTaskBindingItem]) -> Result<()> {
        Items { items }
            .validate_with(&self.settings)
            .map_err(|_report| DurableTaskRefusal::LimitExhausted.into())
    }

    /// Refuses new work whose title, objective or statements pass the
    /// deployment's bounds.
    pub(in crate::durable_task) fn new_work(
        &self,
        title: &str,
        objective: &str,
        statements: &[CompletionStatement],
    ) -> Result<()> {
        NewWork {
            title,
            objective,
            statements,
        }
        .validate_with(&self.settings)
        .map_err(|_report| DurableTaskRefusal::LimitExhausted)?;
        self.statements(statements)
    }

    /// Refuses statements past the deployment's count or text bounds: a new
    /// task's and a person's edit alike.
    pub(in crate::durable_task) fn statements(
        &self,
        statements: &[CompletionStatement],
    ) -> Result<()> {
        if statements.len() > self.settings.max_completion_questions {
            return Err(DurableTaskRefusal::LimitExhausted.into());
        }
        for statement in statements {
            AuthoredQuestion {
                text: statement.question().text(),
                document: statement.question(),
            }
            .validate_with(&self.settings)
            .map_err(|_report| DurableTaskRefusal::LimitExhausted)?;
        }
        Ok(())
    }
}

fn authored_size(question: &Question, settings: &DurableTaskSettings) -> garde::Result {
    if af_validate::serialized_within(question, settings.max_authored_question_bytes) {
        Ok(())
    } else {
        Err(garde::Error::new(
            "authored question exceeds the configured byte limit",
        ))
    }
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "a refused fixture fails the proof loudly"
)]
mod tests {
    use af_core::error_code;
    use af_fleet_runtime::config::decision::CompletionStatement;

    use super::super::{DurableTaskPolicy, DurableTaskSettings};

    fn statements(count: usize) -> Box<[CompletionStatement]> {
        let one = r#"{"id":"s","question":"Are the checks green?","criteria":{"true":"yes","false":"no"}}"#;
        let list: Vec<String> = (0..count)
            .map(|index| one.replace(r#""id":"s""#, &format!(r#""id":"s{index}""#)))
            .collect();
        CompletionStatement::parse_all(&format!("[{}]", list.join(",")))
            .expect("statements the parser reads")
    }

    fn policy(settings: DurableTaskSettings) -> DurableTaskPolicy {
        DurableTaskPolicy::new(
            &garde::Unvalidated::new(settings)
                .validate()
                .expect("bounds"),
        )
    }

    #[test]
    fn new_work_inside_the_bounds_is_accepted() {
        let policy = DurableTaskPolicy::default();

        let accepted = policy.new_work("Fix the cart", "fix the cart", &statements(2));

        assert_eq!(accepted.err().map(|refused| refused.code()), None);
    }

    #[test]
    fn a_title_objective_or_statement_list_past_its_bound_is_refused() {
        let policy = policy(DurableTaskSettings {
            max_title_bytes: 4,
            max_objective_bytes: 4,
            max_completion_questions: 1,
            max_question_text_bytes: 8,
            ..DurableTaskSettings::default()
        });
        for (title, objective, count) in [
            ("Fix the cart", "fix", 1),
            ("Fix", "fix the cart", 1),
            ("Fix", "fix", 2),
            ("Fix", "", 1),
            ("Fix", "fix", 1),
        ] {
            let refused = policy
                .new_work(title, objective, &statements(count))
                .expect_err("one bound is broken");
            assert_eq!(
                refused.code(),
                error_code::DURABLE_TASK_LIMIT_EXHAUSTED,
                "{title}/{objective}/{count}"
            );
        }
    }
}
