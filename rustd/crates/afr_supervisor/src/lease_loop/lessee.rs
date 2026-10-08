//! What every lease shares: the runner's parts, built once and handed to
//! each worker.

use std::sync::Arc;

use afd_core::clock::Clock;
use afr_agent::AgentEngine;
use afr_sandbox::{Engine, Limits};
use tokio::sync::Notify;

use crate::bundles::BundleCache;
use crate::client::ControlPlane;
use crate::egress::Resolve;
use crate::halt::Halt;
use crate::holds::Holds;
use crate::identity::Whoami;
use crate::report_spool::ReportSpool;
use crate::workspace_clone::Mirrors;

/// Everything a lease needs, shared by every worker.
#[derive(Debug)]
pub(crate) struct Lessee {
    /// The daemon.
    pub(crate) plane: ControlPlane,
    /// Builds each lease's sandbox.
    pub(crate) engine: Box<dyn Engine>,
    /// Resolves each lease's egress allowlist before its sandbox is built.
    pub(crate) resolver: Box<dyn Resolve>,
    /// Runs each lease's turn.
    pub(crate) agent: Box<dyn AgentEngine>,
    /// Where reports wait to be posted.
    pub(crate) spool: ReportSpool,
    /// Verified fleet bundles.
    pub(crate) bundles: BundleCache,
    /// Bound repositories' mirrors, fetched outside every sandbox.
    pub(crate) mirrors: Mirrors,
    /// What every sandbox enforces.
    pub(crate) limits: Limits,
    /// The wall clock the daemon's lease deadlines are written in.
    pub(crate) clock: Arc<dyn Clock>,
    /// Sandboxes held for their fleets' next leases.
    pub(crate) holds: Holds,
    /// How the runner stops.
    pub(crate) halt: Halt,
    /// Rung when a report stays spooled, so the drain takes it over.
    pub(crate) held: Notify,
    /// Which runner this is, for every lease's span.
    pub(crate) whoami: Whoami,
}
