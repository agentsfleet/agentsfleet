//! A lease's telemetry: what one lease records, and one lease exported end to
//! end through the runner's own pipeline.
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::sync::{Arc, PoisonError};

use afd_core::env::MapEnv;
use afd_observability::semconv::{ATTR_EVENT_ID, ATTR_LEASE_ID, SPAN_RUNNER_LEASE};
use afr_agent::{AgentEngine, Loop};
use afr_sandbox::Limits;
use afr_telemetry::labels::SandboxStart;
use afr_telemetry::testing::{Recorded, Tally, scoped};
use afr_telemetry::{Endpoint, Telemetry};
use afr_tools::Catalog;
use afr_tools::catalog::FILE_READ;
use afr_tools::stub::Stub;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
use tracing::subscriber::DefaultGuard;
use tracing_subscriber::layer::SubscriberExt as _;

use crate::egress::Egress;
use crate::halt::Halt;
use crate::identity::Whoami;
use crate::lease_loop::Lessee;
use crate::report_spool::ReportSpool;
use crate::storage_home::StorageHome;
use crate::test_support::{
    Behaviour, FLEET_ID, FakeAgent, FakeEngine, FakeResolver, LEASE_ID, Rig, clock, daemon, lease,
    plane,
};
use crate::turns::FleetTurns;
use crate::workspace_clone::{GITHUB_ORIGIN, Mirrors};

use self::collector::{Posted, collector, spans};
use self::scripted::Scripted;

mod collector;
mod scripted;

/// The knobs a runner reads, as the test sets them.
const ENDPOINT_KNOB: &str = "OTEL_EXPORTER_OTLP_ENDPOINT";
const PROTOCOL_KNOB: &str = "OTEL_EXPORTER_OTLP_PROTOCOL";
/// The signal paths a collector serves.
const TRACES: &str = "/v1/traces";
const METRICS: &str = "/v1/metrics";
/// The four span kinds one lease is traced in.
const SPAN_KINDS: [&str; 4] = [SPAN_RUNNER_LEASE, "invoke_agent", "chat", "execute_tool"];
/// The families one sandboxed lease with one tool call feeds.
const FED: [&str; 3] = [
    "agentsfleet_runner_provider_turn_duration_seconds",
    "agentsfleet_runner_tool_call_duration_seconds",
    "agentsfleet_runner_sandbox_start_duration_seconds",
];

/// A built sandbox is counted ready, and one the host refuses is counted
/// failed, each with how long the attempt took.
#[tokio::test(start_paused = true)]
async fn a_sandbox_start_is_counted_by_how_it_ended() {
    for (refuse, outcome) in [(false, SandboxStart::Ready), (true, SandboxStart::Failed)] {
        let engine = FakeEngine {
            refuse,
            ..FakeEngine::default()
        };
        let rig = Rig::new(
            daemon(|_call| None),
            engine,
            FakeAgent::new(Behaviour::Answer),
        );
        let (tally, recorded) = Tally::new();

        scoped(tally, rig.run(&lease(LEASE_ID, FLEET_ID)))
            .await
            .unwrap();

        let started: Vec<SandboxStart> = recorded
            .try_iter()
            .filter_map(|recorded| match recorded {
                Recorded::SandboxStart(outcome, _elapsed) => Some(outcome),
                _other => None,
            })
            .collect();
        assert_eq!(started, vec![outcome], "refuse={refuse}");
    }
}

/// One lease run with the endpoint set delivers its four span kinds under one
/// trace and the runner's families to the collector, and no request carries a
/// credential.
#[tokio::test]
async fn test_runner_exports_spans_and_metrics_when_configured() {
    let (endpoint, received) = collector().await;
    let (telemetry, guard) = install(&endpoint);

    let root = tempfile::tempdir().unwrap();
    let home = StorageHome::open(root.path()).unwrap();
    let lessee = scripted_lessee(&home);
    let (turns, coordinator) = FleetTurns::start();
    tokio::spawn(coordinator);
    let leased = lease(LEASE_ID, FLEET_ID);
    lessee
        .run(&turns, &leased, &Egress::closed())
        .await
        .unwrap();
    drop(guard);
    tokio::task::spawn_blocking(move || telemetry.flush())
        .await
        .unwrap();

    let posted = received
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    assert!(
        posted.iter().all(|post| !post.authorized),
        "the runner sent no credential"
    );
    assert!(
        posted.iter().all(|post| post.path != "/v1/logs"),
        "logs stay on stderr"
    );
    assert_one_lease_trace(&posted, leased.event.event_id.as_ref());
    assert_families_fed(&posted);
}

/// Installs the runner's pipeline against the collector at `endpoint`, and
/// makes its span layer this thread's default until the guard drops.
fn install(endpoint: &str) -> (Telemetry, DefaultGuard) {
    let env = MapEnv::from_pairs([(ENDPOINT_KNOB, endpoint), (PROTOCOL_KNOB, "http/json")]);
    let endpoint = Endpoint::from_env(&env)
        .unwrap()
        .expect("an endpoint is configured");
    let telemetry = Telemetry::install(&endpoint).expect("the runner's pipeline builds");
    assert!(
        telemetry.recording(),
        "the only install in this test binary is the process recorder"
    );
    let _global = tracing::subscriber::set_global_default(tracing_subscriber::registry());
    let subscriber = tracing_subscriber::registry().with(telemetry.layer());
    let guard = tracing::subscriber::set_default(subscriber);
    (telemetry, guard)
}

/// A lessee over `home` whose model makes one `file_read` call, then answers.
fn scripted_lessee(home: &StorageHome) -> Lessee {
    let (plane, _calls) = plane(daemon(|_call| None));
    let agent: Box<dyn AgentEngine> = Box::new(Loop::new(
        Catalog::new(vec![Stub::boxed(&FILE_READ)]),
        Scripted(Arc::default()),
    ));
    Lessee {
        plane,
        engine: Box::new(FakeEngine::default()),
        resolver: Box::new(FakeResolver::default()),
        agent,
        spool: ReportSpool::new(home),
        authoring_spool: crate::report_spool::authoring::Spool::open(home, clock()).unwrap(),
        mirrors: Mirrors::new(home.mirrors(), GITHUB_ORIGIN),
        limits: Limits::default(),
        holds: crate::holds::Holds::start(clock()),
        clock: clock(),
        halt: Halt::new(CancellationToken::new()),
        held: Notify::new(),
        whoami: Whoami::default(),
    }
}

/// Every span kind arrived, all under one trace, and the lease span names
/// the lease and the event it ran.
fn assert_one_lease_trace(posted: &[Posted], event_id: &str) {
    let spans = spans(posted);
    for kind in SPAN_KINDS {
        assert!(
            spans.iter().any(|span| span.name == kind),
            "no `{kind}` span arrived: {spans:?}"
        );
    }
    let traces: std::collections::BTreeSet<&str> =
        spans.iter().map(|span| span.trace.as_str()).collect();
    assert_eq!(traces.len(), 1, "one lease is one trace: {spans:?}");
    let root = spans
        .iter()
        .find(|span| span.name == SPAN_RUNNER_LEASE)
        .expect("the lease span arrived");
    assert_eq!(root.attribute(ATTR_LEASE_ID), Some(LEASE_ID));
    assert_eq!(root.attribute(ATTR_EVENT_ID), Some(event_id));
}

/// Every family one sandboxed lease with one tool call feeds reached the
/// collector's metrics path.
fn assert_families_fed(posted: &[Posted]) {
    let metrics: String = posted
        .iter()
        .filter(|post| post.path == METRICS)
        .map(|post| post.body.to_string())
        .collect();
    for family in FED {
        assert!(
            metrics.contains(family),
            "`{family}` never reached the collector"
        );
    }
}
