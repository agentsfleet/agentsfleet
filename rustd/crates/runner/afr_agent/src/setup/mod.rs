//! A setup run: a draft's turn on the runner's own `guffbolt` loop.
//!
//! ```text
//!  admit ─► connect ─► ContextLoad ─► the draft's saved summary, and the
//!                                     earlier turns it does not cover
//!  guffbolt::drive(SetupLoop, Plain), one tool: propose_setup
//!     window hook: fits [summary] + earlier turns + this turn, or compacts the
//!                  oldest turns ─► ContextSave of the hook's output
//!     propose_setup ─► Propose<FleetSetup>: kept, or "[code] reason" the
//!                      model corrects in the same run
//!  ─► SetupOutput { the answer, the last proposal kept, on the run's base }
//! ```
//!
//! A setup run is a fleet run with three differences: it builds no sandbox,
//! its only tool is `propose_setup`, and its report carries the proposal the
//! run kept. The rules that decide a proposal are `af_fleet_runtime::setup`'s,
//! which the daemon applies again before it writes the next candidate, so
//! this loop decides nothing the daemon does not.

mod context;
mod conversation;
mod store;

#[cfg(test)]
mod context_gap_tests;
#[cfg(test)]
mod context_tests;
#[cfg(test)]
mod fixture;
#[cfg(test)]
mod tests;

use std::borrow::Cow;
use std::fmt;
use std::time::Instant;

use af_providers::Connect;
use af_secrets::Scrub;
use af_wire::context::{ContextLoadResponse, ContextSaveRequest, ContextSaveResponse};
use af_wire::run::{RunPayload, SetupProposal};
use afr_tools::setup::{Candidate, FleetSetup};
use guffbolt::tools::propose::Propose;
use guffbolt::{DriveState, Ending, Plain};
use tokio_util::sync::CancellationToken;

pub(crate) use self::store::usable;

use self::context::Saved;
use self::conversation::{SetupLoop, Shared};
use crate::engine::{EventSink, Meter};
use crate::error::{Result, setup_assignment};
use crate::events::Live;
use crate::{Completed, ExecutionResult, Failure, ResultOutcome};

/// What a setup run stopped early reports as its detail.
const DETAIL_STOPPED: &str = "the setup run was stopped before it answered";
/// What a setup run fails with when its loop waits on a question: its one
/// tool asks none, so this is a loop that cannot end as asked.
const DETAIL_WAITING: &str = "the setup run asked a question no setup run delivers";
/// The run's end, logged once with how it ended and whether it kept a
/// proposal.
const EVENT_ENDED: &str = "setup_run_ended";

/// Everything one setup run is given.
pub struct SetupRun<'run> {
    /// The draft's run: its message, its earlier turns, its model and its
    /// setup part.
    pub run: &'run RunPayload<'run>,
    /// Where the draft's compacted context is read and saved, under the
    /// run's fence.
    pub context: &'run dyn SetupContext,
    /// Where the answer's text deltas go.
    pub events: &'run dyn EventSink,
    /// The tokens the run spends, read into every renewal.
    pub meter: &'run Meter,
    /// Cancelled when the run ends early.
    pub stop: &'run CancellationToken,
}

impl fmt::Debug for SetupRun<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SetupRun")
            .field("run_id", &self.run.run_id)
            .finish_non_exhaustive()
    }
}

/// What a finished setup run hands back to the supervisor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupOutput {
    /// The answer and the run's numbers, as a fleet run's result carries
    /// them.
    pub result: ExecutionResult<'static>,
    /// The last proposal the run kept, on the base the run started from;
    /// none when it kept none.
    pub proposal: Option<SetupProposal<'static>>,
}

/// Where a setup run reads and saves its draft's compacted context: the
/// daemon's fenced context verb, which the supervisor speaks.
#[async_trait::async_trait]
pub trait SetupContext: Send + Sync + fmt::Debug {
    /// The draft's compacted context, read under the run's fence.
    ///
    /// # Errors
    /// [`crate::Error::setup_context`]: the read was refused, or never
    /// answered. The run then goes on from its earlier turns alone.
    async fn load(&self) -> Result<ContextLoadResponse>;

    /// Saves `request`'s checkpoint over the version it expects.
    ///
    /// # Errors
    /// [`crate::Error::setup_context`]: the save was refused, a superseded
    /// run's among them, or never answered.
    async fn save(&self, request: ContextSaveRequest) -> Result<ContextSaveResponse>;
}

/// Runs `setup` to its end over the model `connect` reaches.
pub(crate) async fn run(connect: &dyn Connect, setup: SetupRun<'_>) -> Result<SetupOutput> {
    let started = Instant::now();
    let run = setup.run;
    let work = run
        .owner
        .setup()
        .ok_or_else(|| setup_assignment("the run is not a draft's"))?;
    connect.admit(&run.policy)?;
    let provider = connect.connect(run)?;
    let scrub = Scrub::new(&run.policy)?;
    let message = af_wire::turn::message_of(&run.event.request_json).into_owned();
    let candidate = work
        .candidate
        .as_ref()
        .map(|head| -> Result<Candidate> {
            Ok(Candidate {
                id: head.id.to_string(),
                revision: head.revision,
                definition: serde_json::from_value(head.definition.clone())?,
            })
        })
        .transpose()?;
    let tool = Propose::new(FleetSetup::new(
        candidate,
        Some(message.clone()),
        work.save_requested,
    ));
    let saved = Saved::load(setup.context, run, &scrub).await;
    let opening = context::opening(&scrub, &message, work.candidate.as_ref())?;
    let shared = Shared {
        run,
        provider: provider.as_ref(),
        scrub: &scrub,
        live: Live::new(setup.events, &scrub, started),
        events: setup.events,
        meter: setup.meter,
        stop: setup.stop,
        tool: &tool,
    };
    let mut conversation = SetupLoop::new(shared, saved, opening);
    let ending = guffbolt::drive(&mut conversation, &mut Plain, &mut DriveState::default()).await;
    finish(ending, &tool, &scrub, setup.meter, started)
}

/// The output `ending` leaves: the answer, scrubbed, and the run's numbers,
/// with the proposal `tool` kept.
fn finish(
    ending: Ending<af_providers::Error>,
    tool: &Propose<FleetSetup>,
    scrub: &Scrub,
    meter: &Meter,
    started: Instant,
) -> Result<SetupOutput> {
    let ended = ending.kind();
    let (outcome, content) = match ending {
        Ending::Answered(text) => (
            ResultOutcome::Completed(Completed),
            scrub.text(&text).into_owned(),
        ),
        Ending::Failed(failure) => {
            let failed = Failure {
                class: failure.failure_class(),
                detail: failure.detail().into(),
            };
            (ResultOutcome::Failed(failed), String::new())
        }
        Ending::Stopped => (failed(DETAIL_STOPPED), String::new()),
        Ending::Waiting(_) => (failed(DETAIL_WAITING), String::new()),
    };
    let proposal = tool
        .kept()
        .map(|kept| -> Result<SetupProposal<'static>> {
            Ok(SetupProposal {
                base_candidate_id: kept.base.candidate_id.map(Cow::Owned),
                base_revision: kept.base.revision,
                definition: Cow::Owned(serde_json::to_string(&kept.document)?),
            })
        })
        .transpose()?;
    let kept = proposal.is_some();
    tracing::info!(event = EVENT_ENDED, ending = ended, kept);
    let usage = meter.read();
    Ok(SetupOutput {
        result: ExecutionResult {
            outcome,
            content: content.into(),
            token_count: usage.total(),
            wall_seconds: started.elapsed().as_secs(),
            input_tokens: usage.input,
            cached_input_tokens: usage.cached_input,
            output_tokens: usage.output,
        },
        proposal,
    })
}

/// A failure the run classifies no further, read as `detail`.
fn failed(detail: &'static str) -> ResultOutcome<'static> {
    ResultOutcome::Failed(Failure {
        class: None,
        detail: detail.into(),
    })
}
