//! A run offered the task tools is told the message they name and the fleet's
//! unfinished tasks; any other run is told neither.

use af_wire::durable_task::{DurableTaskInputCandidate, DurableTaskState};
use af_wire::run::RunPayload;
use afr_tools::catalog::{TASK_INPUT, TASK_START};

use super::prompt;
use crate::fixture::{decision_assignment, payload, unbounded};

/// The fixture run, its policy naming the task tools, judged by a decision
/// profile when `judged`.
fn tasking(judged: bool) -> RunPayload<'static> {
    let mut run = payload(&[TASK_START.name(), TASK_INPUT.name()], unbounded());
    run.decision = judged.then(|| Box::new(decision_assignment()));
    run
}

/// The task block names the person's message and its length in bytes, then
/// each unfinished task with its state and generation.
#[test]
fn a_run_offered_the_task_tools_is_told_the_message_and_the_unfinished_tasks() {
    let mut run = tasking(true);
    run.unfinished_tasks = vec![DurableTaskInputCandidate {
        durable_task_id: "0190d5c2-7e1a-7000-8000-00000000b0b2".to_owned(),
        title: "Fix the null-cart error".to_owned(),
        state: DurableTaskState::Waiting,
        generation: 3,
    }];

    let prompt = prompt(&run);

    assert_eq!(
        prompt.task_context.as_deref(),
        Some(
            "## Durable tasks\nThe current message is turn event-1. Use it as source_turn_id, \
         with source_start and source_end as UTF-8 byte \
         offsets into it. Offsets 0 and 21 take the whole message.\nUnfinished tasks:\n\
         - 0190d5c2-7e1a-7000-8000-00000000b0b2: Fix the null-cart error (waiting, generation 3)"
        )
    );
}

/// With no unfinished task the block says so rather than listing nothing.
#[test]
fn a_fleet_with_no_unfinished_task_is_told_so() {
    let prompt = prompt(&tasking(true));

    assert!(
        prompt
            .task_context
            .as_deref()
            .is_some_and(|context| context.ends_with("\nThe fleet has no unfinished task.")),
        "{}",
        prompt.instructions
    );
}

/// A run with no decision profile is offered no task tool, so it is told
/// nothing about tasks, whatever its policy names.
#[test]
fn a_run_without_a_decision_profile_is_told_nothing_about_tasks() {
    let prompt = prompt(&tasking(false));

    assert!(prompt.task_context.is_none());
    assert_eq!(prompt.instructions, prompt.fixed);
}
