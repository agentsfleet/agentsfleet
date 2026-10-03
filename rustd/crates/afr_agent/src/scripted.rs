//! A scripted engine: a fixed turn, for the lanes that prove the supervisor.
//!
//! The supervisor's lanes need a turn whose every tool call, frame and memory
//! item is known in advance, so a sandbox or wire fault cannot hide behind a
//! model's choices. Each [`Step`] is one thing a real turn does: run a process
//! as a tool call, stream text, or remember an item.

use std::borrow::Cow;
use std::time::Instant;

use afd_core::clock::saturating_millis_signed;
use afd_wire::activity::{
    ActivityFrame, FleetResponseChunk, StreamTextKind, ToolCallCompleted, ToolCallStarted,
};
use afd_wire::memory::MemoryDelta;
use afd_wire::report::{Completed, ExecutionResult, Failure, ResultOutcome};
use afr_executor::{Ending, Executor, Spawn};

use crate::engine::{AgentEngine, AgentRun, EventSink, RunOutput};
use crate::error::Result;

/// The redacted arguments a scripted tool call reports; a script carries no
/// secrets, and the frame needs a value.
const ARGS_REDACTED: &str = "{}";

/// Why a step that runs a process failed on a lease with no sandbox.
const NO_SANDBOX: &str = " needs a sandbox and the lease has none";

/// One thing a scripted turn does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Runs a process in the sandbox, reported as a call to `tool`.
    Run {
        /// The tool name its frames carry.
        tool: &'static str,
        /// The process to start.
        spawn: Spawn,
    },
    /// Streams answer text.
    Say(String),
    /// Records a memory item for the fenced push.
    Remember(MemoryDelta<'static>),
}

/// A turn that runs its steps in order and reports what they did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptedEngine {
    steps: Vec<Step>,
}

impl ScriptedEngine {
    /// A turn of `steps`, run in order.
    #[must_use]
    pub fn new(steps: impl IntoIterator<Item = Step>) -> Self {
        Self {
            steps: steps.into_iter().collect(),
        }
    }
}

/// What the steps so far have produced.
#[derive(Debug, Default)]
struct Turn {
    answer: String,
    memory: Vec<MemoryDelta<'static>>,
    failure: Option<String>,
    chunks: u64,
}

#[async_trait::async_trait]
impl AgentEngine for ScriptedEngine {
    async fn run(&self, run: AgentRun<'_>) -> Result<RunOutput> {
        let started = Instant::now();
        let mut turn = Turn::default();
        for (index, step) in self.steps.iter().enumerate() {
            match step {
                Step::Run { tool, spawn } => {
                    let failed = match run.executor {
                        Some(executor) => {
                            run_tool(executor, run.events, tool, &call_id(index), spawn).await?
                        }
                        None => Some(format!("{tool}{NO_SANDBOX}")),
                    };
                    if let Some(detail) = failed {
                        turn.failure.get_or_insert(detail);
                    }
                }
                Step::Say(text) => say(run.events, &mut turn, text),
                Step::Remember(delta) => turn.memory.push(delta.clone()),
            }
        }
        Ok(finish(turn, started))
    }
}

/// A call id unique within the turn.
fn call_id(index: usize) -> String {
    format!("call_{index}")
}

/// Runs one tool call to its end; `Some(detail)` when the process failed.
async fn run_tool(
    executor: &dyn Executor,
    events: &dyn EventSink,
    tool: &'static str,
    call: &str,
    spawn: &Spawn,
) -> Result<Option<String>> {
    let started = Instant::now();
    events.emit(ActivityFrame::ToolCallStarted(ToolCallStarted {
        name: Cow::Borrowed(tool),
        args_redacted: Cow::Borrowed(ARGS_REDACTED),
        call_id: Some(Cow::Owned(call.to_owned())),
    }));
    let process = executor.spawn(spawn).await?;
    let ending = process
        .ended(|_stream, _data| ())
        .await
        .unwrap_or(Ending::Interrupted);
    let elapsed = saturating_millis_signed(started.elapsed());
    events.emit(ActivityFrame::ToolCallCompleted(ToolCallCompleted {
        name: Cow::Borrowed(tool),
        ms: elapsed,
        call_id: Some(Cow::Owned(call.to_owned())),
    }));
    Ok((ending != Ending::Exited(0)).then(|| format!("{tool} ended {ending:?}")))
}

/// Streams one chunk of answer text.
fn say(events: &dyn EventSink, turn: &mut Turn, text: &str) {
    events.emit(ActivityFrame::FleetResponseChunk(FleetResponseChunk {
        text: Cow::Owned(text.to_owned()),
        text_kind: Some(StreamTextKind::Answer),
        first_chunk_after_ms: None,
        stream_start: turn.chunks == 0,
        stream_contiguous: true,
        stream_seq: turn.chunks,
    }));
    turn.chunks += 1;
    turn.answer.push_str(text);
}

/// The report half of a finished turn.
fn finish(turn: Turn, started: Instant) -> RunOutput {
    let outcome = turn
        .failure
        .map_or(ResultOutcome::Completed(Completed {}), |detail| {
            ResultOutcome::Failed(Failure {
                class: None,
                detail: Cow::Owned(detail),
            })
        });
    RunOutput {
        result: ExecutionResult {
            outcome,
            content: Cow::Owned(turn.answer),
            token_count: 0,
            wall_seconds: started.elapsed().as_secs(),
            memory_peak_bytes: 0,
            cpu_throttled_ms: 0,
            input_tokens: 0,
            cached_input_tokens: 0,
            output_tokens: 0,
        },
        memory: turn.memory,
    }
}

#[cfg(test)]
#[path = "scripted/tests.rs"]
mod tests;
