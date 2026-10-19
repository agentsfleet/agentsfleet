//! A scripted engine: a fixed turn, for the lanes that prove the supervisor.
//!
//! The supervisor's lanes need a turn whose every tool call, frame and memory
//! item is known in advance, so a sandbox or wire fault cannot hide behind a
//! model's choices. Each [`Step`] is one thing a real turn does: run a process
//! as a tool call, stream text, remember an item, or spend one provider
//! request's tokens.

use std::borrow::Cow;
use std::time::Instant;

use crate::{Completed, ExecutionResult, ResultOutcome};
use afd_core::clock::saturating_millis_signed;
use afd_wire::activity::{
    ActivityFrame, FleetResponseChunk, StreamTextKind, ToolCallCompleted, ToolCallStarted,
};
use afd_wire::memory::MemoryDelta;
use afd_wire::policy::ExecutionPolicy;
use afd_wire::tool_trace::ToolCallStatus;
use afr_executor::{Ending, Executor, Spawn};
use afr_providers::Usage;

use crate::engine::{AgentEngine, AgentRun, EventSink, Needs, RunOutput};
use crate::error::Result;

/// The redacted arguments a scripted tool call reports; a script carries no
/// secrets, and the frame needs a value.
const ARGS_REDACTED: &str = "{}";

/// One thing a scripted turn does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Runs a process in the sandbox, reported as a call to `tool`. A script
    /// holding one is admitted as needing a sandbox, so the run always has one.
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
    /// Spends one provider request's tokens: one line on the run's meter.
    Spend(Usage),
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
    chunks: u64,
}

#[async_trait::async_trait]
impl AgentEngine for ScriptedEngine {
    /// Admits any policy: a script names its own tools. It needs a sandbox
    /// when one of its steps runs a process.
    fn admit(&self, _policy: &ExecutionPolicy<'_>) -> Result<Needs> {
        let sandbox = self
            .steps
            .iter()
            .any(|step| matches!(step, Step::Run { .. }));
        Ok(Needs { sandbox })
    }

    async fn run(&self, run: AgentRun<'_>) -> Result<RunOutput> {
        let started = Instant::now();
        let mut turn = Turn::default();
        for (index, step) in self.steps.iter().enumerate() {
            match step {
                Step::Run { tool, spawn } => {
                    if let Some(executor) = run.executor {
                        run_tool(executor, run.events, tool, &call_id(index), spawn).await?;
                    }
                }
                Step::Say(text) => say(run.events, &mut turn, text),
                Step::Remember(delta) => turn.memory.push(delta.clone()),
                Step::Spend(usage) => run.meter.add(*usage),
            }
        }
        Ok(finish(turn, started))
    }
}

/// A call id unique within the turn.
fn call_id(index: usize) -> String {
    format!("call_{index}")
}

/// Runs one tool call to its end.
async fn run_tool(
    executor: &dyn Executor,
    events: &dyn EventSink,
    tool: &'static str,
    call: &str,
    spawn: &Spawn,
) -> Result<()> {
    let started = Instant::now();
    events.emit(ActivityFrame::ToolCallStarted(ToolCallStarted {
        name: Cow::Borrowed(tool),
        args_redacted: Cow::Borrowed(ARGS_REDACTED),
        call_id: Cow::Owned(call.to_owned()),
    }));
    let ending = executor
        .spawn(spawn)
        .await?
        .ended(|_stream, _data| ())
        .await;
    let elapsed = saturating_millis_signed(started.elapsed());
    events.emit(ActivityFrame::ToolCallCompleted(ToolCallCompleted {
        name: Cow::Borrowed(tool),
        ms: elapsed,
        call_id: Cow::Owned(call.to_owned()),
        status: status_of(ending),
        output_head: None,
        output_tail: None,
        output_line_count: None,
        exit_code: match ending {
            Some(Ending::Exited(code)) => Some(code),
            _ => None,
        },
    }));
    Ok(())
}

/// How a scripted call ended, in the words a completion frame reports.
const fn status_of(ending: Option<Ending>) -> ToolCallStatus {
    match ending {
        Some(Ending::Exited(0)) => ToolCallStatus::Succeeded,
        None => ToolCallStatus::Interrupted,
        Some(_) => ToolCallStatus::Failed,
    }
}

/// Streams one chunk of answer text.
fn say(events: &dyn EventSink, turn: &mut Turn, text: &str) {
    events.emit(ActivityFrame::FleetResponseChunk(FleetResponseChunk {
        text: Cow::Owned(text.to_owned()),
        text_kind: StreamTextKind::Answer,
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
    RunOutput {
        result: ExecutionResult {
            outcome: ResultOutcome::Completed(Completed),
            content: Cow::Owned(turn.answer),
            token_count: 0,
            wall_seconds: started.elapsed().as_secs(),
            input_tokens: 0,
            cached_input_tokens: 0,
            output_tokens: 0,
        },
        memory: turn.memory,
        trace: None,
        records: Vec::new(),
    }
}
