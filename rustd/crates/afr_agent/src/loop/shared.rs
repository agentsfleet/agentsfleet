//! What every loop of one run shares.
//!
//! The root loop and every child a nested tool starts read the same lease,
//! ledger, meter, provider and system prompt, so a child holds nothing of its
//! own but its conversation and its selection, and the report, the fence and
//! the sandbox stay single (`docs/architecture/runner_execution.md` §"Tool
//! catalog").

use std::time::Instant;

use afd_core::clock::SystemClock;
use afd_wire::policy::ContextBudget;
use afr_egress::Egress;
use afr_executor::Executor;
use afr_memory::Hydrated;
use afr_providers::Provider;
use afr_secrets::Scrub;
use afr_telemetry::labels::Provider as ProviderLabel;
use afr_tools::sandbox::checkouts;
use afr_tools::{Lease, Selection};
use tokio_util::sync::CancellationToken;

use crate::engine::{AgentRun, Checkpoint, EventSink, Meter};
use crate::ledger::Ledger;
use crate::nested::Registry;

/// One run's state, as every loop of it sees it.
pub(crate) struct Shared<'run> {
    pub(crate) lease_id: &'run str,
    /// The fleet the lease runs, which keys the provider's prompt cache.
    pub(crate) fleet_id: &'run str,
    pub(crate) model: &'run str,
    /// The provider, as the turn-duration family labels it.
    pub(crate) provider_label: ProviderLabel,
    pub(crate) provider: &'run dyn Provider,
    /// Cancelled when the lease ends early; every child's token descends
    /// from it.
    pub(crate) stop: &'run CancellationToken,
    pub(crate) checkpoint: &'run dyn Checkpoint,
    pub(crate) context: &'run ContextBudget<'run>,
    /// Every tool the lease was offered; a child's selection narrows it.
    pub(crate) selection: &'run Selection<'run>,
    pub(crate) executor: Option<&'run dyn Executor>,
    pub(crate) scrub: &'run Scrub,
    pub(crate) events: &'run dyn EventSink,
    /// What every call of the lease shares.
    pub(crate) lease: Lease<'run>,
    /// Every call of the run, the children's included.
    pub(crate) ledger: Ledger<'run>,
    pub(crate) meter: &'run Meter,
    /// Every child of the run: its state, its inbox and its stop.
    pub(crate) registry: Registry<'run>,
    /// The system prompt every loop of the run opens with.
    pub(crate) instructions: String,
    pub(crate) started: Instant,
}

impl<'run> Shared<'run> {
    /// What `run` shares between its loops, driving `provider` over the
    /// tools `selection` offers under `instructions`, masking through
    /// `scrub`.
    pub(crate) fn new(
        run: &AgentRun<'run>,
        selection: &'run Selection<'run>,
        provider: &'run dyn Provider,
        scrub: &'run Scrub,
        registry: Registry<'run>,
        instructions: String,
    ) -> Self {
        let policy = &run.lease.policy;
        let started = Instant::now();
        Self {
            lease_id: &run.lease.lease_id,
            fleet_id: &run.lease.event.fleet_id,
            model: &policy.context.model,
            provider_label: ProviderLabel::of(&policy.provider),
            provider,
            stop: run.stop,
            checkpoint: run.checkpoint,
            context: &policy.context,
            selection,
            executor: run.executor,
            scrub,
            events: run.events,
            // The supervisor refused a lease whose binding does not parse
            // before this turn began, so none reaches here.
            lease: Lease::new(
                Box::new(Hydrated::new(run.memory)),
                Egress::new(&run.lease.lease_id, policy, run.mint, &SystemClock),
                run.verbs,
            )
            .with_checkouts(checkouts(policy).unwrap_or_default())
            .with_image_input(provider.accepts_images()),
            ledger: Ledger::new(&run.lease.lease_id, run.events, scrub),
            meter: run.meter,
            registry,
            instructions,
            started,
        }
    }
}
