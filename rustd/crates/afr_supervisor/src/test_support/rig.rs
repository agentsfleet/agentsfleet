//! A lessee over the fakes, for every suite that runs a lease.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

use afd_wire::lease::LeasePayload;
use afd_wire::runner::{AssignedPolicy, NetworkPolicy, SandboxTier};
use afr_sandbox::Limits;
use tokio::sync::{Notify, mpsc};
use tokio_util::sync::CancellationToken;

use super::{
    Answer, FakeAgent, FakeEngine, FakeResolver, GRANTED_UNTIL, INTERVAL_MS, clock, drain, json,
    plane,
};
use crate::bundles::BundleCache;
use crate::client::{Call, Verb};
use crate::egress::Egress;
use crate::halt::Halt;
use crate::holds::Holds;
use crate::identity::Whoami;
use crate::lease_loop::Lessee;
use crate::report_spool::ReportSpool;
use crate::storage_home::StorageHome;
use crate::test_util::Healthy;
use crate::turns::FleetTurns;
use crate::workspace_clone::Mirrors;

/// Where a rig's repositories are served from, under its root, so no lease
/// test ever reaches the network.
const ORIGINS: &str = "origins";

/// A lessee, its storage, the daemon's call log, and the fakes' counters.
pub(crate) struct Rig {
    root: tempfile::TempDir,
    pub(crate) home: StorageHome,
    pub(crate) lessee: Arc<Lessee>,
    pub(crate) calls: mpsc::UnboundedReceiver<Call>,
    pub(crate) shutdown: CancellationToken,
    pub(crate) runs: Arc<AtomicUsize>,
    pub(crate) peak: Arc<AtomicUsize>,
    pub(crate) prepared: Arc<AtomicUsize>,
    pub(crate) destroyed: Arc<AtomicUsize>,
}

impl Rig {
    /// A lessee whose daemon answers with `answer`.
    pub(crate) fn new(
        answer: impl Fn(&Call) -> Answer + Send + Sync + 'static,
        engine: FakeEngine,
        agent: FakeAgent,
    ) -> Self {
        Self::with_holds(answer, engine, agent, Holds::start(clock()))
    }

    /// [`Rig::new`], keeping its sandboxes in `holds`.
    pub(crate) fn with_holds(
        answer: impl Fn(&Call) -> Answer + Send + Sync + 'static,
        engine: FakeEngine,
        agent: FakeAgent,
        holds: Holds,
    ) -> Self {
        Self::build(answer, engine, agent, holds, FakeResolver::default())
    }

    /// [`Rig::new`], resolving each lease's egress through `resolver`.
    pub(crate) fn resolving(
        answer: impl Fn(&Call) -> Answer + Send + Sync + 'static,
        engine: FakeEngine,
        agent: FakeAgent,
        resolver: FakeResolver,
    ) -> Self {
        Self::build(answer, engine, agent, Holds::start(clock()), resolver)
    }

    fn build(
        answer: impl Fn(&Call) -> Answer + Send + Sync + 'static,
        engine: FakeEngine,
        agent: FakeAgent,
        holds: Holds,
        resolver: FakeResolver,
    ) -> Self {
        let root = tempfile::tempdir().unwrap();
        let home = StorageHome::open(root.path()).unwrap();
        let (plane, calls) = plane(answer);
        let shutdown = CancellationToken::new();
        let counters = (
            Arc::clone(&agent.runs),
            Arc::clone(&agent.peak),
            Arc::clone(&engine.prepared),
            Arc::clone(&engine.destroyed),
        );
        let lessee = Arc::new(Lessee {
            plane,
            engine: Box::new(engine),
            resolver: Box::new(resolver),
            agent: Box::new(agent),
            spool: ReportSpool::new(&home),
            bundles: BundleCache::new(&home),
            mirrors: Mirrors::new(
                home.mirrors(),
                format!("file://{}/", root.path().join(ORIGINS).display()),
            ),
            limits: Limits::default(),
            holds,
            clock: clock(),
            halt: Halt::new(shutdown.clone()),
            held: Notify::new(),
            whoami: Whoami::default(),
        });
        let (runs, peak, prepared, destroyed) = counters;
        Self {
            root,
            home,
            lessee,
            calls,
            shutdown,
            runs,
            peak,
            prepared,
            destroyed,
        }
    }

    /// Runs one lease under a fresh turn coordinator, reaching nothing.
    pub(crate) async fn run(&self, lease: &LeasePayload<'_>) -> crate::Result<()> {
        self.run_under(lease, &Egress::closed()).await
    }

    /// Runs one lease under a fresh turn coordinator, reaching what `egress`
    /// admits.
    pub(crate) async fn run_under(
        &self,
        lease: &LeasePayload<'_>,
        egress: &Egress,
    ) -> crate::Result<()> {
        let (turns, coordinator) = FleetTurns::start();
        tokio::spawn(coordinator);
        self.lessee.run(&turns, lease, egress).await
    }

    /// Where the repository `owner/name` is served from: `<origins>/owner/name.git`.
    pub(crate) fn origins(&self) -> PathBuf {
        self.root.path().join(ORIGINS)
    }

    /// Every call the daemon received since the last look.
    pub(crate) fn calls(&mut self) -> Vec<Call> {
        drain(&mut self.calls)
    }
}

/// A daemon that answers every verb the way a healthy one would; `special`
/// answers first when it has an answer.
pub(crate) fn daemon(
    special: impl Fn(&Call) -> Option<Answer> + Send + Sync + 'static,
) -> impl Fn(&Call) -> Answer + Send + Sync + 'static {
    let healthy = Healthy {
        assigned: AssignedPolicy {
            sandbox_tier: SandboxTier::LandlockFull,
            network_policy: NetworkPolicy::AllowAll,
            registry_allowlist: Vec::new(),
            worker_count: 1,
            extra_binds: Vec::new(),
        },
        interval_ms: INTERVAL_MS,
        granted_until: GRANTED_UNTIL,
    };
    move |call| special(call).unwrap_or_else(|| json(&healthy.to(Some(call.verb))))
}

/// The body of the last report the daemon received.
pub(crate) fn reported(calls: &[Call]) -> serde_json::Value {
    calls
        .iter()
        .rfind(|call| call.verb == Verb::Report)
        .and_then(|report| report.body.as_ref())
        .and_then(|body| serde_json::from_slice(body).ok())
        .unwrap_or_default()
}

/// Where the first call of `verb` falls among `calls`.
pub(crate) fn position(calls: &[Call], verb: Verb) -> Option<usize> {
    calls.iter().position(|call| call.verb == verb)
}
