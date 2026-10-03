//! A lessee over the fakes, for every suite that runs a lease.

use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

use afd_wire::lease::{LeasePayload, LeaseResponse};
use afd_wire::memory::MemoryHydrateResponse;
use afd_wire::report::ReportResponse;
use afd_wire::runner::SelfResponse;
use afr_sandbox::Limits;
use tokio::sync::{Notify, mpsc};
use tokio_util::sync::CancellationToken;

use super::{
    Answer, FakeAgent, FakeEngine, GRANTED_UNTIL, INTERVAL_MS, RUNNER_HOST, RUNNER_ID, clock,
    drain, json, plane,
};
use crate::bundles::BundleCache;
use crate::client::{Call, Verb};
use crate::halt::Halt;
use crate::identity::Whoami;
use crate::lease_loop::Lessee;
use crate::report_spool::ReportSpool;
use crate::storage_home::StorageHome;
use crate::turns::FleetTurns;

/// A lessee, its storage, the daemon's call log, and the fakes' counters.
pub(crate) struct Rig {
    _root: tempfile::TempDir,
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
            agent: Box::new(agent),
            spool: ReportSpool::new(&home),
            bundles: BundleCache::new(&home),
            limits: Limits::default(),
            clock: clock(),
            halt: Halt::new(shutdown.clone()),
            held: Notify::new(),
            whoami: Whoami::default(),
        });
        let (runs, peak, prepared, destroyed) = counters;
        Self {
            _root: root,
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

    /// Runs one lease under a fresh turn coordinator.
    pub(crate) async fn run(&self, lease: &LeasePayload<'_>) -> crate::Result<()> {
        let (turns, coordinator) = FleetTurns::start();
        tokio::spawn(coordinator);
        self.lessee.run(&turns, lease).await
    }

    /// Every call the daemon received since the last look.
    pub(crate) fn calls(&mut self) -> Vec<Call> {
        drain(&mut self.calls)
    }
}

/// The sandbox tier the fake daemon assigns and reports.
const SANDBOX_TIER: &str = "landlock_full";

/// A daemon that answers every verb the way a healthy one would; `special`
/// answers first when it has an answer.
pub(crate) fn daemon(
    special: impl Fn(&Call) -> Option<Answer> + Send + Sync + 'static,
) -> impl Fn(&Call) -> Answer + Send + Sync + 'static {
    move |call| {
        special(call).unwrap_or_else(|| match call.verb {
            Verb::Hydrate => json(&MemoryHydrateResponse { memory: Vec::new() }),
            Verb::Capture => json(&serde_json::json!({"stored": 1, "skipped": 0})),
            Verb::Renew => json(&serde_json::json!({"lease_expires_at": GRANTED_UNTIL})),
            Verb::Lease => json(&LeaseResponse {
                lease: None,
                retry_after_ms: Some(INTERVAL_MS),
            }),
            Verb::Heartbeat => json(&serde_json::json!({"status": "ok",
                "assigned_policy": {"sandbox_tier": SANDBOX_TIER, "network_policy": "allow_all",
                    "registry_allowlist": [], "worker_count": 1, "extra_binds": []},
                "degraded": false, "degraded_reason": null, "selftest_requested": false,
                "heartbeat_interval_ms": INTERVAL_MS})),
            Verb::Records => json(&serde_json::json!({"stored_count": 1, "skipped_count": 0})),
            Verb::Me => json(&SelfResponse {
                id: RUNNER_ID.into(),
                status: "active".into(),
                host_id: RUNNER_HOST.into(),
                sandbox_tier: SANDBOX_TIER.into(),
                last_seen_at: 0,
                assigned_policy: None,
                achievable: None,
                degraded: false,
                degraded_reason: None,
            }),
            Verb::Activity | Verb::Report | Verb::Bundle | Verb::Mint => {
                json(&ReportResponse { ok: true })
            }
        })
    }
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
