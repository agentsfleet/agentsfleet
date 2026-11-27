//! What every loop of one run shares, and the conversation each child's
//! turn runs over.
//!
//! The root loop and every child `guffbolt` drives read the same run,
//! ledger, meter, provider and system prompt, so a child holds nothing of
//! its own but its conversation, and the report, the fence and the sandbox
//! stay single. `guffbolt` keeps each child's transcript, inbox, narrowed
//! tools and stop; this run lends it the loop a child's turn runs as
//! ([`Raise`]).

use std::time::Instant;

use af_core::clock::SystemClock;
use af_egress::Egress;
use af_guest_wire::Sizes;
use af_providers::Provider;
use af_providers::observe::labels::Provider as ProviderLabel;
use af_secrets::Scrub;
use af_wire::policy::ContextBudget;
use afr_guest_client::AgentSource;
use afr_memory::Loaded;
use afr_tools::sandbox::checkouts;
use afr_tools::{Run, RunTools, Selection};
use guffbolt::agents::{Agents, Caps, Raise, Seat};
use guffbolt::tools::Limits;
use guffbolt::tools::process::sessions::SESSIONS_PER_RUN_MAX;
use tokio_util::sync::CancellationToken;

use super::Harness;
use crate::engine::{AgentRun, Checkpoint, EventSink, Meter};
use crate::ledger::Ledger;

/// How far one run's children may go unless the runner chooses otherwise
/// ([`RunLimits::caps`]): two loops deep, and six open at once; a run may
/// start any number over its life. A restore checks a checkpoint's children
/// against this depth, the deepest any run may go.
pub(crate) const CAPS: Caps = Caps { depth: 2, open: 6 };

/// What every run's loop holds it to: its built-in tools' bounds, the
/// sessions it keeps open and how far its children go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunLimits {
    /// The bounds `guffbolt`'s built-in tools hold a run's calls to.
    pub tools: Limits,
    /// The most interactive sessions one run keeps open.
    pub max_sessions: usize,
    /// How deep a run's children go, and how many are open at once; the
    /// depth no deeper than [`CAPS`]'s, which a restore checks against.
    pub caps: Caps,
    /// What every run's guest agent holds to: the sandbox's guest agent is started
    /// with the same, so the tools word their limits by its figures.
    pub sizes: Sizes,
}

impl RunLimits {
    /// The bounds every run held before the runner could choose them.
    pub const DEFAULT: Self = Self {
        tools: Limits::DEFAULT,
        max_sessions: SESSIONS_PER_RUN_MAX,
        caps: CAPS,
        sizes: Sizes::DEFAULT,
    };
}

impl Default for RunLimits {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// One run's state, as every loop of it sees it.
pub(crate) struct Shared<'run> {
    pub(crate) payload: &'run af_wire::run::RunPayload<'run>,
    pub(crate) recovered_durable_task: Option<&'run crate::DurableTaskRecovery>,
    pub(crate) recovered_turn: Option<&'run crate::TurnRecovery>,
    pub(crate) prepared_durable_task: Option<&'run crate::PreparedDurableTask>,
    pub(crate) decision: Option<&'run af_wire::decision::DecisionAssignment>,
    pub(crate) fencing_token: u64,
    pub(crate) run_id: &'run str,
    /// The fleet the run runs, which keys the provider's prompt cache.
    pub(crate) fleet_id: &'run str,
    pub(crate) model: &'run str,
    /// The provider, as the turn-duration family labels it.
    pub(crate) provider_label: ProviderLabel,
    pub(crate) provider: &'run Provider,
    /// Cancelled when the run ends early; every child's token descends
    /// from it.
    pub(crate) stop: &'run CancellationToken,
    pub(crate) checkpoint: &'run dyn Checkpoint,
    pub(crate) recovery: Option<&'run dyn crate::RecoveryBoundarySink>,
    /// Where the root takes the messages a person sends while it works.
    pub(crate) inbox: &'run dyn crate::Inbox,
    pub(crate) model_context_tokens: u32,
    pub(crate) output_tokens: u32,
    pub(crate) inference: Option<&'run af_wire::inference::ModelInference>,
    pub(crate) context: &'run ContextBudget<'run>,
    /// Every tool the run was offered; a child's selection narrows it.
    pub(crate) selection: &'run Selection<'run>,
    /// The tools the run built over its own backends, which a selection's
    /// backed entries name. Owned here, not borrowed: a boxed tool may read
    /// what it borrowed as it drops, so it cannot sit in a local the run also
    /// borrows for as long as it lasts.
    pub(crate) backed: RunTools<'run>,
    pub(crate) agent: &'run dyn AgentSource,
    pub(crate) scrub: &'run Scrub,
    pub(crate) events: &'run dyn EventSink,
    /// What every call of the run shares.
    pub(crate) run: &'run Run<'run>,
    /// Every call of the run, the children's included.
    pub(crate) ledger: Ledger<'run>,
    pub(crate) meter: &'run Meter,
    /// Every child of the run: its state, its inbox, its stop and its
    /// transcript between turns.
    pub(crate) registry: Agents,
    /// The system prompt every loop of the run opens with.
    pub(crate) instructions: String,
    /// The fixed text the system prompt opens with: all a compaction's
    /// pinned digest covers, so pinned memory or access changing between
    /// runs keeps the thread's saved summary readable.
    pub(crate) fixed: String,
    pub(crate) started: Instant,
}

/// What every call of `run` shares, its memory beginning as `memory`. The
/// supervisor refused a run whose binding does not parse before this turn
/// began, so none reaches here.
pub(crate) fn run_state<'run>(
    run: &AgentRun<'run>,
    memory: Loaded<'run>,
    provider: &Provider,
    limits: RunLimits,
) -> Run<'run> {
    let policy = &run.run.policy;
    Run::new(
        memory,
        Egress::new(&run.run.run_id, policy, run.mint, &SystemClock),
        run.verbs,
    )
    .with_checkouts(checkouts(policy).unwrap_or_default())
    .with_image_input(provider.accepts_images())
    .with_limits(limits.tools, limits.max_sessions)
    .with_sizes(limits.sizes)
}

impl<'run> Shared<'run> {
    /// What `run` shares between its loops, driving `provider` over the
    /// tools `selection` offers, those `backed` holds among them, under
    /// `prompt`'s system prompt, masking through `scrub`; every call shares
    /// `state`.
    pub(crate) fn new(
        run: &AgentRun<'run>,
        (selection, backed): (&'run Selection<'run>, RunTools<'run>),
        provider: &'run Provider,
        scrub: &'run Scrub,
        registry: Agents,
        prompt: crate::prompt::Prompt,
        state: &'run Run<'run>,
    ) -> Self {
        let policy = &run.run.policy;
        let started = Instant::now();
        Self {
            payload: run.run,
            recovered_durable_task: run.recovered_durable_task,
            recovered_turn: run.recovered_turn,
            prepared_durable_task: run.prepared_durable_task,
            decision: run.run.decision.as_deref(),
            fencing_token: run.run.fencing_token,
            run_id: &run.run.run_id,
            fleet_id: &run.run.event.fleet_id,
            model: &policy.context.model,
            provider_label: ProviderLabel::of(&policy.provider),
            provider,
            stop: run.stop,
            checkpoint: run.checkpoint,
            recovery: run.recovery,
            inbox: run.inbox,
            model_context_tokens: policy.model_context_tokens,
            output_tokens: policy
                .inference
                .as_ref()
                .map_or(1, |settings| settings.max_output_tokens),
            inference: policy.inference.as_ref(),
            context: &policy.context,
            selection,
            backed,
            agent: run.agent,
            scrub,
            events: run.events,
            run: state,
            ledger: Ledger::new(&run.run.run_id, &run.run.event.turn_id, run.events, scrub)
                .numbering(&state.calls),
            meter: run.meter,
            registry,
            instructions: prompt.instructions,
            fixed: prompt.fixed,
            started,
        }
    }
}

/// Every loop of the run shares this, so a child's turn is the run's own
/// loop: the root's provider, sandbox, memory, ledger and meter, over the
/// transcript `guffbolt` keeps for it.
impl<'run> Raise for Shared<'run> {
    type Child<'s>
        = Harness<'s, 'run>
    where
        Self: 's;

    fn agents(&self) -> &Agents {
        &self.registry
    }

    fn child(&self, seat: Seat) -> Harness<'_, 'run> {
        Harness::child(self, seat)
    }
}
