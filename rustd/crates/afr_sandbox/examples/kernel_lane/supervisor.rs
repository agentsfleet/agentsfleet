//! Compose the shipping supervisor, agent loop and bubblewrap engine.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use afd_core::env::MapEnv;
use afr_providers::Call;
use afr_sandbox::{BubblewrapEngine, Engine, Sandbox, SandboxRequest, probe};
use afr_supervisor::config::{ENV_API_URL, ENV_RUNNER_TOKEN, ENV_STORAGE_HOME};
use afr_supervisor::test_util::LEASE_ID;
use libtest_mimic::Failed;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use crate::lane::Lane;
use crate::run::{expect, runtime};
use crate::supervisor_model::{Asked, model};
use crate::supervisor_plane::{GRAFANA, RUNNER_TOKEN, Received, Server};
use crate::upstream::{Reply, Route, Seen, Upstream};

const DEADLINE: Duration = Duration::from_secs(60);
const MARKER: &str = "workspace/workspace/running";

/// Counts actual preparations without replacing any engine operation.
#[derive(Debug)]
struct Counted {
    engine: BubblewrapEngine,
    prepared: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl Engine for Counted {
    async fn prepare(&self, request: SandboxRequest<'_>) -> afr_sandbox::Result<Box<dyn Sandbox>> {
        self.prepared.fetch_add(1, Ordering::Relaxed);
        self.engine.prepare(request).await
    }
}

#[derive(Default)]
pub(crate) struct Case {
    pub(crate) calls: Vec<Call>,
    pub(crate) repository: bool,
    pub(crate) fail_mutations: bool,
    pub(crate) cancel_running: bool,
}

pub(crate) struct Observed {
    pub(crate) asked: Vec<Asked>,
    pub(crate) received: Vec<Received>,
    pub(crate) outbound: Vec<Seen>,
    pub(crate) prepared: usize,
    pub(crate) origin_unchanged: bool,
    pub(crate) reaped: Vec<u32>,
    calls: Vec<Call>,
}

impl Observed {
    pub(crate) fn output(&self, id: &str) -> Result<&str, Failed> {
        let ordinal = self
            .calls
            .iter()
            .position(|call| call.id == id)
            .ok_or_else(|| Failed::from(format!("no scripted call {id}")))?;
        self.asked
            .get(ordinal + 1)
            .and_then(|asked| asked.results.last())
            .map(String::as_str)
            .ok_or_else(|| format!("no result for {id}").into())
    }
}

pub(crate) fn run(lane: &Lane, case: Case) -> Result<Observed, Failed> {
    runtime().block_on(exercise(lane, case))
}

async fn exercise(lane: &Lane, case: Case) -> Result<Observed, Failed> {
    let root = tempfile::tempdir()?;
    crate::git::serve(root.path())?;
    let origin_ref = root
        .path()
        .join("origins/acme/widget.git/.git/refs/heads/main");
    let initial = fs::read(&origin_ref)?;
    let tools: Vec<&str> = case
        .calls
        .iter()
        .map(|call| call.name.as_str())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut server = Server::start(&tools, case.repository, case.fail_mutations).await?;
    let mut upstream = upstream().await;
    let outcome = drive(lane, &case, root.path(), &server, &upstream).await;
    let outbound = upstream.seen();
    upstream.stop().await;
    let stopped = server.stop().await;
    let (asked, prepared, reaped) = outcome?;
    stopped?;
    expect(
        !lane.lease_dir(LEASE_ID).exists(),
        "lease directory removed after supervisor shutdown",
    )?;
    expect(
        !lane.config.cgroup_root.join(LEASE_ID).exists(),
        "lease cgroup removed after shutdown",
    )?;
    let origin_unchanged = initial == fs::read(origin_ref)?;
    Ok(Observed {
        asked,
        outbound,
        prepared,
        reaped,
        origin_unchanged,
        received: server.calls.try_iter().collect(),
        calls: case.calls,
    })
}

async fn upstream() -> Upstream {
    Upstream::serve(vec![Route::new(
        GRAFANA,
        hyper::Method::GET,
        "/api/search",
        vec![Reply::json(
            200,
            &serde_json::json!({"dashboard": "grafana fixture dashboard"}),
        )],
    )])
    .await
}

type Driven = (Vec<Asked>, usize, Vec<u32>);

async fn drive(
    lane: &Lane,
    case: &Case,
    root: &Path,
    server: &Server,
    upstream: &Upstream,
) -> Result<Driven, Failed> {
    let prepared = Arc::new(AtomicUsize::new(0));
    let engine = Counted {
        engine: lane.engine(),
        prepared: Arc::clone(&prepared),
    };
    let (model, asked) = model(&case.calls);
    let catalog = afr_tools::Catalog::hosted(Arc::new(upstream.network()));
    let agent = afr_agent::Loop::new(catalog, model);
    let shutdown = CancellationToken::new();
    let mut runner = start(lane, root, server, engine, agent, shutdown.clone())?;
    let waited = tokio::time::timeout(DEADLINE, until_done(lane, case, &server.reported)).await;
    shutdown.cancel();
    let joined = tokio::time::timeout(DEADLINE, &mut runner).await;
    if joined.is_err() {
        runner.abort();
        let _cancelled = runner.await;
        return Err("supervisor did not stop before the trial deadline".into());
    }
    joined???;
    let reaped = waited??;
    for pid in &reaped {
        expect(
            !Path::new(&format!("/proc/{pid}")).exists(),
            format!("sandbox child {pid} reaped"),
        )?;
    }
    Ok((asked.asked(), prepared.load(Ordering::Relaxed), reaped))
}

fn start(
    lane: &Lane,
    root: &Path,
    server: &Server,
    engine: Counted,
    agent: afr_agent::Loop,
    stop: CancellationToken,
) -> Result<tokio::task::JoinHandle<afr_supervisor::Result<()>>, Failed> {
    let storage = root.join("runner").display().to_string();
    let env = MapEnv::from_pairs([
        (ENV_API_URL, server.url.as_str()),
        (ENV_RUNNER_TOKEN, RUNNER_TOKEN),
        (ENV_STORAGE_HOME, storage.as_str()),
    ]);
    let (config, home) = afr_supervisor::boot(&env)?;
    let probe = probe(&lane.config.probe_paths());
    let origin = format!("file://{}/", root.join("origins").display());
    Ok(tokio::spawn(async move {
        afr_supervisor::run_fetching_from(
            &config,
            home,
            Box::new(engine),
            Box::new(agent),
            probe,
            &origin,
            stop,
        )
        .await
    }))
}

async fn until_done(lane: &Lane, case: &Case, reported: &Notify) -> Result<Vec<u32>, Failed> {
    if !case.cancel_running {
        reported.notified().await;
        return Ok(Vec::new());
    }
    let marker = lane.lease_dir(LEASE_ID).join(MARKER);
    let mut poll = tokio::time::interval(Duration::from_millis(20));
    while !marker.exists() {
        poll.tick().await;
    }
    let procs = lane
        .config
        .cgroup_root
        .join(LEASE_ID)
        .join("tenant/cgroup.procs");
    let pids = fs::read_to_string(procs)?
        .lines()
        .map(str::parse)
        .collect::<Result<Vec<u32>, _>>()?;
    expect(
        !pids.is_empty(),
        "the marker belongs to a running sandbox process tree",
    )?;
    Ok(pids)
}
