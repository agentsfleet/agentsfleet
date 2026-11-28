//! `task_start` and `task_input`: the fleet's own model binds one span of a
//! person's message to durable work, a new task or an unfinished one.
//!
//! ```text
//!  task_start {title, source_turn_id, source_start, source_end, target, statements}
//!     ─► one new-work item ─┐
//!  task_input {durable_task_id, source_turn_id, source_start, source_end}
//!     ─► one steer item ────┴─► POST L/durable_tasks/bindings {source_turn_id, plan_digest, items: [it]}
//!                                 ─► the task it started or joined, or "[kind] sentence"
//! ```
//!
//! Neither tool makes a decision call: the daemon checks that the source turn
//! is a person's message the run holds, takes the objective as the span of
//! it, and starts the task with its first run queued after this one, which
//! stays an ordinary run. The plan digest is the binding's own
//! (`DurableTaskBindingsInput::interpretation`), so the same call again is
//! answered with the first task and a different one under the same message is
//! refused.

use af_wire::decision::NoulCriteria;
use af_wire::durable_task::{
    DurableTaskBindingItem, DurableTaskBindingsInput, DurableTaskBindingsResponse,
    DurableTaskInputAction, DurableTaskRequestedTarget, DurableTaskStatement,
};
use garde::Validate as _;
use guffbolt::ToolOutput;
use schemars::JsonSchema;
use serde::Deserialize;

use super::answered_with;
use crate::catalog::{Entry, TASK_INPUT, TASK_START};
use crate::handler::Handler;
use crate::runtime::{ToolContext, ToolErrorCode};

/// What a task works on.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Target {
    /// Work through the HTTP tools, with no repository checked out.
    ApiOnly,
    /// Work on one open pull request the person named.
    ExistingPr {
        /// The repository, as `owner/name`.
        repository: String,
        /// The pull request's number.
        number: u32,
    },
}

/// One yes-or-no statement the task is judged done against.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Statement {
    /// A stable key, such as `checks-green`: letters, digits, `_`, `.` and
    /// `-`, unique within the task.
    id: String,
    /// The question, such as "Do PR 88's required checks pass on its head?".
    question: String,
    /// What evidence means yes.
    yes: String,
    /// What evidence means no.
    no: String,
}

/// `task_start`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct NewTask {
    /// A short name a person reads in the fleet's task list.
    title: String,
    /// The person's message the task comes from, as the prompt names it.
    source_turn_id: String,
    /// Where the task's objective starts in that message, in UTF-8 bytes.
    source_start: i32,
    /// Where it ends, exclusive, in UTF-8 bytes.
    source_end: i32,
    /// What the task works on.
    target: Target,
    /// What must hold for the task to be done: at least one.
    statements: Vec<Statement>,
}

/// `task_input`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Steering {
    /// The unfinished task, as the prompt lists it.
    durable_task_id: String,
    /// The person's message that steers it, as the prompt names it.
    source_turn_id: String,
    /// Where the steering text starts in that message, in UTF-8 bytes.
    source_start: i32,
    /// Where it ends, exclusive, in UTF-8 bytes.
    source_end: i32,
}

/// Starts a durable task from a span of the person's message.
#[derive(Debug)]
pub(crate) struct TaskStart;

#[async_trait::async_trait]
impl Handler for TaskStart {
    const GUIDANCE: &'static str = "Use task_start when the person's newest message asks for work that must continue beyond this run.";
    const ENTRY: &'static Entry = &TASK_START;
    const DESCRIPTION: &'static str = "Call this tool only when the person's newest message \
        asks for work that must keep going past this run, such as fixing something until a \
        check passes. Do not call it for a question, a greeting or an approval; answer those in \
        plain text. Starts a task from the span of their message that asks for it, judged done \
        against the statements you write; its first run follows this one.";
    type Arguments = NewTask;

    async fn run(&self, arguments: NewTask, context: ToolContext<'_, '_>) -> ToolOutput {
        let item = DurableTaskBindingItem::NewWork {
            source_start: arguments.source_start,
            source_end: arguments.source_end,
            title: arguments.title,
            target: match arguments.target {
                Target::ApiOnly => DurableTaskRequestedTarget::ApiOnly,
                Target::ExistingPr { repository, number } => {
                    DurableTaskRequestedTarget::ExistingPr { repository, number }
                }
            },
            statements: arguments.statements.into_iter().map(statement).collect(),
        };
        bind(context, arguments.source_turn_id, item).await
    }
}

/// Joins a span of the person's message to an unfinished task.
#[derive(Debug)]
pub(crate) struct TaskInput;

#[async_trait::async_trait]
impl Handler for TaskInput {
    const ENTRY: &'static Entry = &TASK_INPUT;
    const DESCRIPTION: &'static str = "Call this tool only when the person's newest message \
        steers one of the fleet's unfinished tasks the prompt lists. Do not call it for a \
        question, a greeting or an approval; answer those in plain text. Hands the span of their \
        message that steers it to that task, which reads it at its next step.";
    type Arguments = Steering;

    async fn run(&self, arguments: Steering, context: ToolContext<'_, '_>) -> ToolOutput {
        let item = DurableTaskBindingItem::Steer {
            source_start: arguments.source_start,
            source_end: arguments.source_end,
            durable_task_id: arguments.durable_task_id,
        };
        bind(context, arguments.source_turn_id, item).await
    }
}

/// A statement as the binding carries it.
fn statement(statement: Statement) -> DurableTaskStatement {
    DurableTaskStatement {
        id: statement.id,
        question: statement.question,
        criteria: NoulCriteria {
            yes: statement.yes,
            no: statement.no,
        },
    }
}

/// Posts `item` under `source_turn_id` as the binding's one item, once the
/// bounds the wire declares hold.
async fn bind(
    context: ToolContext<'_, '_>,
    source_turn_id: String,
    item: DurableTaskBindingItem,
) -> ToolOutput {
    let input = match planned(source_turn_id, item) {
        Ok(input) => input,
        Err(refused) => return refused,
    };
    answered_with(
        context.run.verbs.durable_task_bindings(input).await,
        |response| ToolOutput::succeeded(sentence(&response)),
    )
}

/// The binding of `item` under `source_turn_id`, its plan digest computed as
/// the daemon computes it, or the bound it breaks.
fn planned(
    source_turn_id: String,
    item: DurableTaskBindingItem,
) -> Result<DurableTaskBindingsInput, ToolOutput> {
    let refused = |reason: String| ToolOutput::failed(ToolErrorCode::InvalidArguments, &reason);
    // The run's fence is the supervisor's to add: a tool never holds it.
    let mut input = DurableTaskBindingsInput {
        fencing_token: 0,
        source_turn_id,
        plan_digest: String::new(),
        items: vec![item],
    };
    let plan = serde_json::to_vec(&input.interpretation())
        .map_err(|source| refused(source.to_string()))?;
    input.plan_digest = af_core::etag::sha256_hex(&plan);
    input
        .validate()
        .map_err(|report| refused(report.to_string()))?;
    Ok(input)
}

/// What the model reads once the daemon bound the item.
fn sentence(response: &DurableTaskBindingsResponse<'_>) -> String {
    let Some(bound) = response.items.first() else {
        return "agentsfleetd bound nothing for this call".to_owned();
    };
    let task = bound.durable_task_id.as_deref().unwrap_or("the task");
    match (bound.action, response.replayed) {
        (DurableTaskInputAction::NewWork, false) => format!(
            "Started task {task}. Its first run follows this one, and it is judged done against \
             its statements; this run goes on as before."
        ),
        (DurableTaskInputAction::NewWork, true) => {
            format!("This message already started task {task}; it is the same task.")
        }
        (DurableTaskInputAction::Steer, false) => {
            format!("Handed this message to task {task}; it reads it at its next step.")
        }
        (DurableTaskInputAction::Steer, true) => {
            format!("This message was already handed to task {task}.")
        }
    }
}

#[cfg(test)]
mod tests;
