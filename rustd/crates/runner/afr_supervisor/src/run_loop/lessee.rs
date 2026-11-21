//! What every run shares: the runner's parts, built once and handed to
//! each worker.

use std::sync::Arc;

use af_core::clock::Clock;
use af_egress::Resolve;
use afr_agent::AgentEngine;
use afr_sandbox::{Engine, Limits};

use crate::client::ControlPlane;
use crate::halt::Halt;
use crate::holds::Holds;
use crate::identity::Whoami;
use crate::workspace_clone::Mirrors;

/// Everything a run needs, shared by every worker.
#[derive(Debug)]
pub(crate) struct Lessee {
    /// The daemon.
    pub(crate) plane: ControlPlane,
    /// Builds each run's sandbox.
    pub(crate) engine: Box<dyn Engine>,
    /// Resolves each run's egress allowlist before its sandbox is built.
    pub(crate) resolver: Box<dyn Resolve>,
    /// Runs each run's turn.
    pub(crate) agent: Box<dyn AgentEngine>,
    /// Bound repositories' mirrors, fetched outside every sandbox.
    pub(crate) mirrors: Mirrors,
    /// What every sandbox enforces.
    pub(crate) limits: Limits,
    /// The wall clock the daemon's run deadlines are written in.
    pub(crate) clock: Arc<dyn Clock>,
    /// Sandboxes held for their fleets' next runs.
    pub(crate) holds: Holds,
    /// How the runner stops.
    pub(crate) halt: Halt,
    /// Which runner this is, for every run's span.
    pub(crate) whoami: Whoami,
    /// File admission and signed-object transport shared by every worker.
    pub(crate) recovery: crate::recovery::RecoveryResources,
}
