//! One connection owner orchestrates existing runner, fleet and authoring services.
#![cfg_attr(not(test), deny(unused_crate_dependencies))]
#![deny(clippy::self_named_module_files)]

mod connection;
mod error;
mod observations;
mod service;
mod operation;
mod registry;
mod scheduler;
mod session;
mod stream_headers;

use std::sync::Arc;

use af_core::clock::Clock;
use af_stream::Budget;
use afd_authoring::Drafts;
use afd_bus::{Handle, wake::Wake};
use afd_fleet::lease::LeaseService;
use afd_runner::Runners;
use afd_sse::StreamLimit;
use tokio::time::Instant;
use tokio_util::{sync::CancellationToken, task::TaskTracker};

pub use connection::ConnectionContext;
pub use error::{Error, Result};
pub use operation::Operation;

/// Boot-selected handles; no service opens another pool or broker connection here.
pub struct GatewayDependencies {
    /// Host policy, liveness and guarded connection ownership.
    pub runners: Runners,
    /// Existing fleet placement, billing, activity and report owner.
    pub leases: LeaseService,
    /// Existing private authoring owner.
    pub drafts: Drafts,
    /// The same bus used by every production publisher.
    pub bus: Handle,
    /// The same stream ceiling used by browser streams.
    pub stream_limit: StreamLimit,
    /// Validated per-replica notification capacity.
    pub wake: Wake,
    /// Per-replica frame budgets resolved and validated before accepting connections.
    pub frame_memory: af_stream::MemoryLimits,
    /// Wall time for identifiers and cached lease expiry; timeouts use monotonic instants.
    pub wall_clock: Arc<dyn Clock>,
    /// Product completion events reuse the daemon's configured sink.
    pub analytics: af_observability::Analytics,
}

impl std::fmt::Debug for GatewayDependencies {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GatewayDependencies").finish_non_exhaustive()
    }
}

pub(crate) struct Inner {
    dependencies: GatewayDependencies,
    input_budget: Budget,
    output_budget: Budget,
    registry: Arc<registry::Registry>,
    tasks: TaskTracker,
}

/// Cloneable session opening and shutdown owner for one daemon replica.
#[derive(Clone)]
pub struct Gateway(Arc<Inner>);

impl std::fmt::Debug for Gateway {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Gateway").finish_non_exhaustive()
    }
}

impl Gateway {
    /// Share immutable domain handles and one replica-wide frame budget.
    #[must_use]
    pub fn new(dependencies: GatewayDependencies) -> Self {
        let (input_budget, output_budget) = Budget::duplex(dependencies.frame_memory);
        Self(Arc::new(Inner {
            dependencies, input_budget, output_budget,
            registry: Arc::new(registry::Registry::default()),
            tasks: TaskTracker::new(),
        }))
    }

    /// Own wake subscriptions, scheduling and the independent database backstop.
    ///
    /// # Errors
    /// Preserves scheduler failures; broker outages do not disable database recovery.
    pub async fn run(&self, stop: CancellationToken) -> Result<()> {
        scheduler::run(self.clone(), stop).await
    }

    /// Stop offers, finish accepted requests and close every session by one deadline.
    ///
    /// # Errors
    /// Returns the drain timeout after cancelling any session still serving.
    pub async fn drain(&self, deadline: Instant) -> Result<()> {
        self.0.registry.begin_drain();
        self.0.tasks.close();
        if let Err(source) = tokio::time::timeout_at(deadline, self.0.tasks.wait()).await {
            self.0.registry.cancel_all();
            return Err(error::ErrorKind::Timeout { operation: Operation::Drain, source }.into());
        }
        Ok(())
    }
}

impl afd_http::services::RunnerStreams for Gateway {
    async fn open(&self, runner: afd_auth::principal::Runner, request: axum::extract::Request) -> axum::response::Response {
        match session::open(self, runner, request).await {
            Ok(response) => response,
            Err(error) => { self.observe(&error, Operation::Opening); error.response() }
        }
    }
}
