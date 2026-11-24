//! Shared steps a decision case takes outside the decision verbs: picking a
//! choice answer, and switching the fixture runner's durable tasks.

use af_core::clock;
use afd_runner::RunnerEdit;

use super::super::fixture::Case;

/// Picks `selected` for the choice question `name`, with certainty.
pub(super) fn choose(
    answers: &mut std::collections::BTreeMap<String, af_wire::decision::DecisionAnswer>,
    name: &str,
    selected: &str,
) {
    let (choice, confidence, probabilities) = match answers.get_mut(name) {
        Some(af_wire::decision::DecisionAnswer::Choice {
            choice,
            confidence,
            probabilities,
        }) => Some((choice, confidence, probabilities)),
        _ => None,
    }
    .expect("configured choice answer");
    selected.clone_into(choice);
    *confidence = Some(1.0);
    for (option, probability) in probabilities {
        *probability = if option == selected { 1.0 } else { 0.0 };
    }
}

/// Switches durable tasks on or off for the fixture's runner, as a
/// platform admin's edit does.
pub(super) async fn durable_tasks_enabled(case: &Case, enabled: bool) {
    let runners = case.held.fixtures.runners();
    let row = runners
        .self_record(&case.held.runner)
        .await
        .expect("saved assignment");
    let mut policy = row.assignment.decode().expect("valid saved assignment");
    policy.durable_tasks_enabled = enabled;
    runners
        .platform()
        .edit(
            &case.held.runner,
            RunnerEdit {
                assigned_policy: Some(&policy),
                labels: None,
            },
            clock::now(),
        )
        .await
        .expect("native durable policy edit");
}
