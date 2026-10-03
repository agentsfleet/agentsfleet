//! The Rust runner's wire, against the daemon that ships.
//!
//! The runner reads the daemon's replies leniently, so a daemon that grows a
//! field never strands a runner built before it. The other direction stays
//! closed: what a runner WRITES is refused when it names a key the daemon does
//! not carry, because a runner believing something about the protocol that is
//! not true must find out at the first request, not in production.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_state::sql::{LEASE_STATUS_ACTIVE, LEASE_STATUS_REPORTED};
use afd_wire::paths::RUNNER_REPORTS;
use agentsfleetd::supervisor::Supervisor;

use crate::e2e::scenario;
use crate::tail::lease;
use crate::wire::{post, report_body};

/// A key no build of the daemon carries.
const FUTURE: &str = "future";

/// A report naming an unknown key is refused, and the lease survives to take
/// the correct one.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_runner_body_with_unknown_field_refused() {
    let mut supervisor = Supervisor::new();
    let run = scenario(&mut supervisor).await;
    let http = reqwest::Client::new();
    let (lease_id, fence) = lease(&http, &run).await;
    let mut body = report_body(&lease_id, &run.event_id, fence);
    let report = body.as_object_mut().expect("a report is a JSON object");
    report.insert(FUTURE.to_owned(), serde_json::Value::from(1));

    let refused = post(&http, &run, RUNNER_REPORTS, &body).await;
    assert_eq!(refused.status().as_u16(), 400, "an unknown key is refused");

    body.as_object_mut()
        .expect("a report is a JSON object")
        .remove(FUTURE);
    let accepted = post(&http, &run, RUNNER_REPORTS, &body).await;
    assert_eq!(
        accepted.status().as_u16(),
        200,
        "the refusal settled nothing, so the corrected report lands"
    );

    supervisor.shutdown().await;
    run.cleanup().await;
}

/// How long the runner is given to take, run and settle the seeded event.
const SETTLE_DEADLINE: std::time::Duration = std::time::Duration::from_secs(60);

/// How often the lease row is read while waiting for it to settle.
const SETTLE_POLL: std::time::Duration = std::time::Duration::from_millis(250);

/// The newest lease the fleet was issued, by status.
const LATEST_LEASE_STATUS: &str = "SELECT status FROM fleet.runner_leases WHERE fleet_id = $1::uuid ORDER BY created_at DESC LIMIT 1";

/// The content one fleet memory key holds.
const REMEMBERED_CONTENT: &str =
    "SELECT content FROM memory.memory_entries WHERE fleet_id = $1::uuid AND key = $2";

/// The memory item the scripted run remembers and the push must carry.
const REMEMBERED_KEY: &str = "rust-runner-roundtrip";

/// The tool the scripted run calls.
const TOOL: &str = "shell";

/// What the scripted run says, streamed as its answer.
const ANSWER: &str = "the scripted run is done";

/// A lease, end to end, through the Rust runner: it heartbeats, takes the
/// seeded event, runs a scripted turn whose tool call executes through the
/// executor, streams its frames to the fleet's live tail, pushes memory,
/// settles the report, and leaves no sandbox behind.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_rust_runner_lease_roundtrip() {
    let mut supervisor = Supervisor::new();
    let run = scenario(&mut supervisor).await;
    allow_all_egress(&run).await;
    let hub = afd_dragonfly::SubscriptionHub::start(crate::e2e::dragonfly_config())
        .await
        .expect("the lane's Dragonfly accepts a subscriber");
    let mut tail = hub.subscribe(&format!("fleet:{}:activity", run.fleet));
    crate::tail::settle().await;
    let home = tempfile::tempdir().expect("a storage home");
    let sandboxes = tempfile::tempdir_in("/tmp").expect("a short sandbox base");
    let shutdown = tokio_util::sync::CancellationToken::new();
    let runner = tokio::spawn(rust_runner(
        &run,
        home.path(),
        sandboxes.path(),
        shutdown.clone(),
    ));

    assert_settled(&run).await;
    assert_frames(&mut tail).await;
    assert_memory_pushed(&run).await;

    shutdown.cancel();
    runner
        .await
        .expect("the runner task joins")
        .expect("the runner stops cleanly on shutdown");
    let left = std::fs::read_dir(sandboxes.path())
        .expect("the engine's sandbox base still exists")
        .count();
    assert_eq!(left, 0, "no sandbox outlives its lease");
    drop(tail);
    supervisor.shutdown().await;
    // Retired like every other scenario's fleet: left `active` with its lease,
    // the next daemon's reclaim sweeper re-marks it and the scenarios after
    // this one spend their poll budget on it (`e2e_retire`).
    run.cleanup().await;
}

/// The lease is reported, and the event carries the scripted turn's outcome
/// and answer — not merely a lease that left `leased`.
async fn assert_settled(run: &crate::e2e::Scenario) {
    assert_eq!(
        settled_status(run).await.as_deref(),
        Some(LEASE_STATUS_REPORTED)
    );
    let status = crate::reads::event_column(run, &run.event_id, "status").await;
    assert_eq!(status.as_deref(), Some("processed"));
    let answer = crate::reads::event_column(run, &run.event_id, "response_text").await;
    assert_eq!(answer.as_deref(), Some(ANSWER));
}

/// Both tool-call frames and the answer chunk reached the fleet's live tail.
async fn assert_frames(tail: &mut afd_dragonfly::Subscription) {
    let kinds = frame_kinds(tail).await;
    for kind in ["tool_call_started", "tool_call_completed", "chunk"] {
        assert!(
            kinds.iter().any(|seen| seen == kind),
            "{kind} reached the tail: {kinds:?}"
        );
    }
}

/// The remembered item is in the fleet's durable memory, read from the store:
/// the runner's own memory read is scoped to a live lease, and this one is
/// settled.
async fn assert_memory_pushed(run: &crate::e2e::Scenario) {
    let mut connection = run
        .booted
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    let content: Option<String> = sqlx::query_scalar(REMEMBERED_CONTENT)
        .bind(&run.fleet)
        .bind(REMEMBERED_KEY)
        .fetch_optional(&mut *connection)
        .await
        .expect("the memory table reads");
    assert_eq!(
        content.as_deref(),
        Some(ANSWER),
        "the remembered item was pushed"
    );
}

/// Reassigns the seeded runner a policy with no egress control: this runner
/// reports none yet, and the daemon rightly withholds leases from a host that
/// cannot enforce what its assignment demands.
async fn allow_all_egress(run: &crate::e2e::Scenario) {
    use afd_wire::runner::{AssignedPolicy, NetworkPolicy, SandboxTier};
    let policy = AssignedPolicy {
        sandbox_tier: SandboxTier::LandlockFull,
        network_policy: NetworkPolicy::AllowAll,
        registry_allowlist: Vec::new(),
        worker_count: 1,
        extra_binds: Vec::new(),
    };
    afd_runner::Runners::new(
        run.booted.database.clone(),
        afd_crypto::entropy::Entropy::new(),
    )
    .assign_policy(&run.runner_id, &policy, afd_core::clock::now())
    .await
    .expect("an operator may relax a runner's egress");
}

/// The Rust runner's supervisor, pointed at the scenario's daemon.
fn rust_runner(
    run: &crate::e2e::Scenario,
    home: &std::path::Path,
    sandboxes: &std::path::Path,
    shutdown: tokio_util::sync::CancellationToken,
) -> impl std::future::Future<Output = afr_supervisor::Result<()>> + use<> {
    use afr_supervisor::config::{ENV_API_URL, ENV_RUNNER_TOKEN, ENV_STORAGE_HOME};
    let home = home.to_string_lossy().into_owned();
    let env = afd_core::env::MapEnv::from_pairs([
        (ENV_API_URL, run.base.as_str()),
        (ENV_RUNNER_TOKEN, run.token.as_str()),
        (ENV_STORAGE_HOME, home.as_str()),
    ]);
    let (config, home) = afr_supervisor::boot(&env).expect("the scenario's env is valid");
    let engine = afr_sandbox::UnsandboxedEngine::new(sandboxes.to_owned())
        .expect("a debug build permits the unsandboxed engine");
    async move {
        afr_supervisor::run(
            &config,
            home,
            Box::new(engine),
            Box::new(scripted()),
            capable(),
            shutdown,
        )
        .await
    }
}

/// A turn that runs one process, says one thing and remembers one item.
fn scripted() -> afr_agent::scripted::ScriptedEngine {
    use afr_agent::scripted::{ScriptedEngine, Step};
    use std::borrow::Cow;
    ScriptedEngine::new([
        Step::Run {
            tool: TOOL,
            spawn: afr_executor::Spawn::program("echo").arg("hi"),
        },
        Step::Say(ANSWER.to_owned()),
        Step::Remember(afd_wire::memory::MemoryDelta {
            key: Cow::Borrowed(REMEMBERED_KEY),
            content: Cow::Borrowed(ANSWER),
            category: Cow::Borrowed("core"),
        }),
    ])
}

/// A host that can enforce everything the reassigned policy asks for.
fn capable() -> afr_sandbox::HostProbe {
    afr_sandbox::HostProbe {
        landlock: true,
        seccomp: true,
        cgroup_controllers: afr_sandbox::REQUIRED_CONTROLLERS
            .map(str::to_owned)
            .to_vec(),
        bubblewrap: true,
        kvm: afr_sandbox::Kvm::Absent,
        toolbox_filesystem: true,
    }
}

/// The fleet's lease status once it leaves `active`, or `None` past the deadline.
///
/// A lease is born `active`, so a poll that stopped at the first row it saw
/// passed only when the whole run fit between two reads.
async fn settled_status(run: &crate::e2e::Scenario) -> Option<String> {
    let deadline = tokio::time::Instant::now() + SETTLE_DEADLINE;
    while tokio::time::Instant::now() < deadline {
        let mut connection = run
            .booted
            .database
            .acquire()
            .await
            .expect("a pooled connection");
        let status: Option<String> = sqlx::query_scalar(LATEST_LEASE_STATUS)
            .bind(&run.fleet)
            .fetch_optional(&mut *connection)
            .await
            .expect("the lease table reads");
        if status
            .as_deref()
            .is_some_and(|status| status != LEASE_STATUS_ACTIVE)
        {
            return status;
        }
        tokio::time::sleep(SETTLE_POLL).await;
    }
    None
}

/// Every frame kind the tail carried, read until it falls quiet.
async fn frame_kinds(tail: &mut afd_dragonfly::Subscription) -> Vec<String> {
    let mut kinds = Vec::new();
    while let Some(frame) = crate::tail::next_frame(tail).await {
        if let Some(kind) = frame.get("kind").and_then(serde_json::Value::as_str) {
            kinds.push(kind.to_owned());
        }
    }
    kinds
}
