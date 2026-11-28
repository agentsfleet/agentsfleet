//! `update_plan`: the model's plan steps, recorded as a call the thread
//! renders.
//!
//! The arguments are shaped as models are trained to write them. The answer lists
//! every step with its status, which is what the thread shows.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::tool::ToolOutput;
use crate::tools::{Context, Handler};

/// Where a step stands; [`Step::status`]'s doc says what each means, so the
/// schema stays a plain `enum`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Status {
    Pending,
    InProgress,
    Completed,
}

/// One step of the plan.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Step {
    /// What the step does.
    step: String,
    /// `pending` (not started), `in_progress` (being worked on now) or
    /// `completed`.
    status: Status,
}

/// `update_plan`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    /// Why the plan changed, when it did.
    #[serde(default)]
    explanation: Option<String>,
    /// Every step, in order; at most one in progress.
    plan: Vec<Step>,
}

/// Records the plan.
#[derive(Debug, Clone, Copy)]
pub struct UpdatePlan;

impl Handler for UpdatePlan {
    const NAME: &'static str = "update_plan";
    const DESCRIPTION: &'static str = "Record a plan the person can see and return its steps and statuses. Use it when several \
        dependent steps or a decision need tracking. For a short answer, reply directly.";
    const EXAMPLES: &'static [&'static str] =
        &[r#"{"plan":[{"step":"Read the failing test","status":"in_progress"}]}"#];
    type Arguments = Plan;

    fn run(
        &self,
        arguments: Plan,
        _context: Context<'_>,
    ) -> impl Future<Output = ToolOutput> + Send {
        // The answer spells a status through the declaration the argument was
        // read with, so the two cannot drift; a fieldless variant always
        // serializes to its word, so the empty fallback is never taken.
        let steps = arguments.plan.iter().map(|step| {
            let status = spelling(step.status);
            format!("[{status}] {}", step.step)
        });
        let lines: Vec<String> = arguments.explanation.into_iter().chain(steps).collect();
        std::future::ready(ToolOutput::succeeded(lines.join("\n")))
    }
}

/// `status` as the argument spells it.
fn spelling(status: Status) -> String {
    serde_json::to_value(status)
        .ok()
        .and_then(|spelled| spelled.as_str().map(str::to_owned))
        .unwrap_or_default()
}
