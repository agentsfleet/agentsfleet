//! `update_plan`: the model's plan steps, recorded as a call the thread
//! renders.
//!
//! The arguments are Codex's (`codex-rs/protocol/src/plan_tool.rs`), so a
//! model trained on Codex's harness writes them unprompted. The answer lists
//! every step with its status, which is what the thread shows.

use schemars::JsonSchema;
use serde::Deserialize;

use crate::catalog::{Entry, UPDATE_PLAN};
use crate::handler::Handler;
use crate::runtime::{ToolContext, ToolOutput};

/// Where a step stands; [`Step::status`]'s doc says what each means, so the
/// schema stays a plain `enum`.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Status {
    Pending,
    InProgress,
    Completed,
}

impl Status {
    /// The spelling the answer carries, the same as the argument's.
    const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
        }
    }
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
pub(crate) struct Plan {
    /// Why the plan changed, when it did.
    #[serde(default)]
    explanation: Option<String>,
    /// Every step, in order; at most one in progress.
    plan: Vec<Step>,
}

/// Records the plan.
#[derive(Debug)]
pub(crate) struct UpdatePlan;

#[async_trait::async_trait]
impl Handler for UpdatePlan {
    const ENTRY: &'static Entry = &UPDATE_PLAN;
    const DESCRIPTION: &'static str = "Record your plan as steps with their status, and update it as you work.";
    type Arguments = Plan;

    async fn run(&self, arguments: Plan, _context: ToolContext<'_, '_>) -> ToolOutput {
        let steps = arguments
            .plan
            .iter()
            .map(|step| format!("[{}] {}", step.status.as_str(), step.step));
        let lines: Vec<String> = arguments.explanation.into_iter().chain(steps).collect();
        ToolOutput::succeeded(lines.join("\n"))
    }
}

#[cfg(test)]
#[path = "plan/tests.rs"]
mod tests;
