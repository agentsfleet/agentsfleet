//! One §6 run: the Rust runner's real loop, over the hosted catalog, leasing
//! one event of a bundle scenario until the daemon has settled it.
//!
//! The loop's egress client is the HTTPS fake's routed one, so every
//! `http_request` leaves through the production guard, admission and TLS; the
//! model is a [`FakeModel`]. Each run gets its own runner, stopped once its
//! event settles, so a scenario that delivers two events reads two transcripts.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use std::sync::Arc;
use std::time::Duration;

use afr_supervisor::config::{ENV_API_URL, ENV_RUNNER_TOKEN, ENV_STORAGE_HOME};
use tokio_util::sync::CancellationToken;

use crate::bundle_install::MINTED_TOKEN;
use crate::bundle_repair::GITHUB;
use crate::e2e::Scenario;
use crate::fake_model::{Asked, FakeModel};
use crate::https::{Seen, Upstream};
use crate::reads::event_column;

/// How long one run is given to lease, run and settle its event.
const SETTLE_DEADLINE: Duration = Duration::from_secs(90);
/// How often the event row is read while waiting.
const SETTLE_POLL: Duration = Duration::from_millis(250);
/// The statuses an event ends in (`schema/800_fleet_events.sql`).
const SETTLED: [&str; 3] = ["processed", "fleet_error", "gate_blocked"];

/// A settled event's answer, NULL when no run produced one.
const ANSWER: &str = "SELECT response_text FROM core.fleet_events \
                      WHERE fleet_id = $1::uuid AND event_id = $2";

/// How one run's event ended.
#[derive(Debug)]
pub(crate) struct Settled {
    /// The event's final status.
    pub(crate) status: String,
    /// What the run answered, as the daemon stored it.
    pub(crate) answer: String,
}

/// Runs `event_id` of `run` through the real loop, the model answering as
/// `model` and every upstream as `upstream`, and stops the runner once the
/// event settles.
pub(crate) async fn run_event(
    run: &Scenario,
    event_id: &str,
    upstream: &Upstream,
    model: FakeModel,
) -> Settled {
    let home = tempfile::tempdir().expect("a storage home");
    let sandboxes = tempfile::tempdir_in("/tmp").expect("a short sandbox base");
    let shutdown = CancellationToken::new();
    let mut runner = tokio::spawn(runner(
        run,
        home.path(),
        sandboxes.path(),
        upstream.network(),
        model,
        shutdown.clone(),
    ));
    // A runner that stops on its own before the event settles failed to
    // boot or to lease; that is the failure to report, not a 90 s silence.
    let status = tokio::select! {
        status = settled(run, event_id) => status,
        exited = &mut runner => {
            panic!("the runner stopped before event {event_id} settled: {exited:?}")
        }
    };
    shutdown.cancel();
    runner
        .await
        .expect("the runner task joins")
        .expect("the runner stops cleanly on shutdown");
    let status = status
        .unwrap_or_else(|| panic!("event {event_id} did not settle within {SETTLE_DEADLINE:?}"));
    let answer = answer_of(run, event_id).await.unwrap_or_default();
    Settled { status, answer }
}

/// What the run answered, or `None` for an event that ended without a run.
async fn answer_of(run: &Scenario, event_id: &str) -> Option<String> {
    let mut connection = run
        .booted
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query_scalar(ANSWER)
        .bind(&run.fleet)
        .bind(event_id)
        .fetch_one(&mut *connection)
        .await
        .expect("the settled event's row reads")
}

/// The event's status once it settles, or `None` past the deadline.
async fn settled(run: &Scenario, event_id: &str) -> Option<String> {
    let deadline = tokio::time::Instant::now() + SETTLE_DEADLINE;
    while tokio::time::Instant::now() < deadline {
        let status = event_column(run, event_id, "status").await;
        if status
            .as_deref()
            .is_some_and(|seen| SETTLED.contains(&seen))
        {
            return status;
        }
        tokio::time::sleep(SETTLE_POLL).await;
    }
    None
}

/// The Rust runner's supervisor, pointed at the scenario's daemon, hosting
/// the real loop.
fn runner(
    run: &Scenario,
    home: &std::path::Path,
    sandboxes: &std::path::Path,
    network: afr_egress::Network,
    model: FakeModel,
    shutdown: CancellationToken,
) -> impl Future<Output = afr_supervisor::Result<()>> + use<> {
    let home = home.to_string_lossy().into_owned();
    let env = afd_core::env::MapEnv::from_pairs([
        (ENV_API_URL, run.base.as_str()),
        (ENV_RUNNER_TOKEN, run.token.as_str()),
        (ENV_STORAGE_HOME, home.as_str()),
    ]);
    let (config, home) = afr_supervisor::boot(&env).expect("the scenario's env is valid");
    let engine = afr_sandbox::UnsandboxedEngine::new(sandboxes.to_owned())
        .expect("a debug build permits the unsandboxed engine");
    let agent = afr_agent::Loop::new(afr_tools::Catalog::hosted(Arc::new(network)), model);
    async move {
        afr_supervisor::run(
            &config,
            home,
            Box::new(engine),
            Box::new(agent),
            capable(),
            shutdown,
        )
        .await
    }
}

/// A host that can enforce everything an allow-all policy asks for.
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

/// Invariant 1, end to end: every GitHub request carried the token the daemon
/// minted, and no prompt or tool output the model read ever held it.
pub(crate) fn assert_token_stayed_on_the_wire(seen: &[Seen], asked: &[Asked]) {
    let bearer = format!("Bearer {MINTED_TOKEN}");
    for request in seen.iter().filter(|request| request.host == GITHUB) {
        let sent = request
            .headers
            .get("authorization")
            .and_then(|value| value.to_str().ok());
        assert_eq!(
            sent,
            Some(bearer.as_str()),
            "{} {}",
            request.method,
            request.path
        );
    }
    for turn in asked {
        assert!(
            !turn.instructions.contains(MINTED_TOKEN),
            "the prompt holds the token"
        );
        assert!(
            turn.results
                .iter()
                .all(|result| !result.contains(MINTED_TOKEN)),
            "a tool output holds the token"
        );
    }
}

/// The path of every `POST` the upstreams received, in arrival order.
pub(crate) fn posts(seen: &[Seen]) -> Vec<&str> {
    (seen.iter())
        .filter(|request| request.method == hyper::Method::POST)
        .map(|request| request.path.as_str())
        .collect()
}

/// The JSON body of the last request to `path`.
pub(crate) fn body_at(seen: &[Seen], path: &str) -> serde_json::Value {
    let request = (seen.iter().rev())
        .find(|request| request.path == path)
        .unwrap_or_else(|| panic!("a request reached {path}"));
    serde_json::from_str(&request.body).expect("the body is JSON")
}
