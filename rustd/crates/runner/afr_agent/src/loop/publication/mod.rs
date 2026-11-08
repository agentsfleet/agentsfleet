//! Root-only publication derives its target and receipts from native task state.

mod process;

use super::Harness;
use af_providers::ToolSpec;
use af_wire::task::{TaskEffectInput, TaskFileChange, TaskTarget};
use afr_executor::Spawn;
use afr_tools::{Schema, ToolErrorCode, ToolOutput};
use serde::Deserialize;
use std::{sync::LazyLock, time::Duration};

pub(super) const NAME: &str = "propose_change";
/// The sandbox program used for both pre-validation and post-validation capture.
const CANDIDATE_SCRIPT: &str = include_str!("candidate.py");
static SCHEMA: LazyLock<Schema> = LazyLock::new(|| {
    Schema::of::<Invocation>(
        "Validate and publish the current local changes to this task's already authorized existing pull request. Supply the validation command as argv. The runner snapshots the complete candidate tree before and after validation, refuses changed/unsupported trees, and publishes with the native expected-head comparison. No merge or new PR. Wait for child agents and commands to finish first.",
    )
});

#[derive(Deserialize, schemars::JsonSchema, garde::Validate)]
#[serde(deny_unknown_fields)]
struct Invocation {
    /// Validation executable followed by its arguments, without shell interpolation.
    #[garde(length(min = 1, max = 32), inner(length(bytes, min = 1, max = 4096)))]
    validation: Vec<String>,
    /// Maximum validation runtime in seconds, from 1 through 600.
    #[garde(range(min = 1, max = 600))]
    timeout_seconds: u32,
}

#[derive(Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Candidate {
    tested_tree: String,
    changes: Vec<TaskFileChange>,
}

pub(super) fn spec() -> ToolSpec<'static> {
    ToolSpec {
        name: NAME,
        description: SCHEMA.description(),
        parameters: SCHEMA.parameters(),
    }
}

fn refused(detail: &str) -> ToolOutput {
    ToolOutput::failed(ToolErrorCode::AgentsfleetdRefused, detail)
}

// Tool output is the public boundary: retain the causal chain until this
// translation, and log only the fixed context so private candidate text stays private.
fn failed(
    code: ToolErrorCode,
    context: &'static str,
    source: impl std::error::Error + Send + Sync + 'static,
) -> ToolOutput {
    let error = crate::decision::boundary(context, source);
    tracing::debug!(event = "task_publication_refused", error_code = %error.code(), reason = %error);
    ToolOutput::failed(code, context)
}

fn invalid(arguments: &serde_json::Value) -> Result<Invocation, ToolOutput> {
    let input: Invocation = serde_json::from_value(arguments.clone()).map_err(|source| {
        failed(
            ToolErrorCode::InvalidArguments,
            "Provide a bounded validation command and timeout.",
            source,
        )
    })?;
    garde::Validate::validate(&input).map_err(|source| {
        failed(
            ToolErrorCode::InvalidArguments,
            "Validation arguments exceed their bounds.",
            source,
        )
    })?;
    Ok(input)
}

impl Harness<'_, '_> {
    pub(super) fn publication_allowed(&self) -> bool {
        self.depth == 0
            && (self.selection.tool("shell").is_some()
                || self.selection.tool("exec_command").is_some())
            && self.task.as_ref().is_some_and(|prepared| {
                prepared.allow_actions
                    && prepared
                        .inputs
                        .current_task
                        .as_ref()
                        .is_some_and(|task| matches!(task.target, TaskTarget::ExistingPr { .. }))
            })
    }

    pub(super) async fn publish_task(&self, arguments: &serde_json::Value) -> ToolOutput {
        match self.publication(arguments).await {
            Ok(output) | Err(output) => output,
        }
    }

    async fn publication(&self, arguments: &serde_json::Value) -> Result<ToolOutput, ToolOutput> {
        if !self.publication_allowed() {
            return Err(ToolOutput::failed(
                ToolErrorCode::NotOffered,
                "Publication is unavailable for this task.",
            ));
        }
        let input = invalid(arguments)?;
        if self
            .shared
            .registry
            .list()
            .iter()
            .any(|child| child.status.as_str() == "running")
        {
            return Err(refused(
                "Wait for child agents to finish before validating and publishing.",
            ));
        }
        let executor = self
            .shared
            .executor
            .peek()
            .ok_or_else(|| refused("Publication requires the task workspace."))?;
        // No background session may keep editing while this root validates.
        if self.shared.lease.sessions.close_all(executor).await > 0 {
            return Err(refused(
                "Open commands were stopped. Review their output, then request validation again.",
            ));
        }
        let task = self
            .task
            .as_ref()
            .and_then(|prepared| prepared.inputs.current_task.as_ref())
            .ok_or_else(|| refused("Native task identity is absent."))?;
        let effects = self
            .shared
            .lease
            .verbs
            .task_effects(af_wire::task::TaskEffectsQuery {
                fencing_token: self.shared.fencing_token,
                task_id: task.task_id.to_string(),
                ordinal_after: self.reconciled_effect_ordinal,
                starting_after: None,
                limit: 1,
            })
            .await
            .map_err(|source| {
                failed(
                    ToolErrorCode::AgentsfleetdRefused,
                    "Publication receipts could not be reconciled.",
                    crate::decision::native(source),
                )
            })?;
        if !effects.effects.is_empty() {
            return Err(refused(
                "A previous publication must be reconciled at the next completed action boundary before another attempt.",
            ));
        }
        let TaskTarget::ExistingPr {
            repository,
            head,
            tree,
            ..
        } = &task.target
        else {
            return Err(refused("This task has no repository target."));
        };
        let name = repository
            .split_once('/')
            .map(|(_, name)| name)
            .ok_or_else(|| refused("Native repository identity is invalid."))?;
        let (before, output) = self
            .validated_candidate(executor, name, tree, &input)
            .await?;
        let effect = self.submit_candidate(task, head, before).await?;
        let confirmed =
            effect.state == af_wire::task::TaskEffectState::Confirmed && effect.commit.is_some();
        let output = serde_json::json!({ "publication": effect,
            "validation": { "argv": input.validation, "exit_code": 0, "output": String::from_utf8_lossy(&output) } }).to_string();
        Ok(if confirmed {
            ToolOutput::succeeded(output)
        } else {
            refused(&output)
        })
    }

    async fn submit_candidate(
        &self,
        task: &af_wire::task::TaskContext<'_>,
        head: &str,
        before: Candidate,
    ) -> Result<af_wire::task::TaskEffect, ToolOutput> {
        let request = TaskEffectInput {
            operation_id: crate::decision::identity().map_err(|source| {
                failed(
                    ToolErrorCode::AgentsfleetdRefused,
                    "Publication identity is unavailable.",
                    source,
                )
            })?,
            fencing_token: self.shared.fencing_token,
            task: af_wire::task::DecisionTaskVersion {
                task_id: task.task_id.to_string(),
                generation: task.generation,
            },
            source_item_id: task.originating_item_id.to_string(),
            covered_input_sequence: self.incorporated_input_sequence,
            expected_head: head.to_owned(),
            tested_tree: before.tested_tree,
            changes: before.changes,
        };
        garde::Validate::validate(&request).map_err(|source| {
            failed(
                ToolErrorCode::AgentsfleetdRefused,
                "The candidate exceeds supported publication paths or content bounds.",
                source,
            )
        })?;
        let effect = self.shared.lease.verbs.task_publish(request).await
            .map_err(|source| failed(ToolErrorCode::AgentsfleetdRefused, "Publication did not return a confirmed outcome. Reconcile its native receipt before another attempt.", crate::decision::native(source)))?;
        Ok(effect)
    }

    async fn validated_candidate(
        &self,
        executor: &dyn afr_executor::Executor,
        name: &str,
        tree: &str,
        input: &Invocation,
    ) -> Result<(Candidate, Vec<u8>), ToolOutput> {
        let before = self.candidate(executor, name, tree).await?;
        if before.changes.is_empty() {
            return Err(refused("There are no local changes to publish."));
        }
        let (program, args) = input
            .validation
            .split_first()
            .ok_or_else(|| refused("Validation command is empty."))?;
        let validation = Spawn::program(program)
            .args(args)
            .cwd(name)
            .env("NO_COLOR", "1")
            .env("TERM", "dumb")
            .env("LANG", "C.UTF-8");
        let output = process::run(
            executor,
            validation,
            Duration::from_secs(u64::from(input.timeout_seconds)),
            &self.stop,
            32 * 1024,
            false,
        )
        .await?;
        let after = self.candidate(executor, name, tree).await?;
        if before != after {
            return Err(refused(
                "Validation changed the candidate tree. Review those changes and validate again before publication.",
            ));
        }
        Ok((before, output))
    }

    async fn candidate(
        &self,
        executor: &dyn afr_executor::Executor,
        repository: &str,
        tree: &str,
    ) -> Result<Candidate, ToolOutput> {
        let spawn = Spawn::program("python3")
            .args(["-I", "-S", "-c", CANDIDATE_SCRIPT, tree])
            .cwd(repository);
        let bytes = process::run(
            executor,
            spawn,
            Duration::from_secs(60),
            &self.stop,
            2 * 1024 * 1024,
            true,
        )
        .await?;
        serde_json::from_slice(&bytes).map_err(|source| {
            failed(
                ToolErrorCode::AgentsfleetdRefused,
                "The sandbox did not return a complete supported candidate tree.",
                source,
            )
        })
    }
}

#[cfg(test)]
mod tests;
