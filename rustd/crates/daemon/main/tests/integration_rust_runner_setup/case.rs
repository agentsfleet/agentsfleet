//! One case's harness: a scenario whose own fleet is retired, a person and
//! their draft, the Rust runner answering through a scripted model, and the
//! rows a case reads back.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use af_core::id::Uuid7;
use afd_setup::types::MessageState;
use afr_sandbox::{Engine, Sandbox, SandboxRequest, UnsandboxedEngine};
use afr_supervisor::config::{
    ENV_API_URL, ENV_RUNNER_TOKEN, ENV_STORAGE_HOME, ENV_STREAM_CA, ENV_STREAM_URL,
};
use agentsfleetd::supervisor::Supervisor;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use super::seat::Seat;
use crate::e2e::{Scenario, scenario};
use crate::fake_model::{Asked, FakeModel, Transcript, say};

/// How long one turn is given to be assigned, run and settled.
const SETTLE_DEADLINE: Duration = Duration::from_secs(60);
/// How often the turn's row is read while waiting.
const SETTLE_POLL: Duration = Duration::from_millis(100);

/// One case: the scenario, its person and draft, the runner answering them,
/// and how many sandboxes that runner prepared.
pub(super) struct Case {
    pub(super) scene: Scenario,
    pub(super) seat: Seat,
    pub(super) marker: String,
    runner: tokio::task::JoinHandle<afr_supervisor::Result<()>>,
    shutdown: CancellationToken,
    prepared: Arc<AtomicUsize>,
    _home: tempfile::TempDir,
    _sandboxes: tempfile::TempDir,
}

impl Case {
    /// A scenario whose own fleet is retired, a person whose model has a
    /// window of `context_tokens` writing at most `output_tokens`, and a
    /// runner answering through `decide`, which reads a request only when it
    /// carries this case's marker.
    pub(super) async fn start(
        supervisor: &mut Supervisor,
        (context_tokens, output_tokens): (i32, u32),
        decide: impl Fn(&Asked) -> Vec<guffbolt::Chunk> + Send + Sync + 'static,
    ) -> (Self, Transcript) {
        let scene = scenario(supervisor).await;
        crate::e2e_retire::retire_fleet(&scene.booted, &scene.fleet).await;
        crate::integration_rust_runner::allow_all_egress(&scene).await;
        let seat = Seat::create(&scene, context_tokens, output_tokens).await;
        let marker = format!("[draft case {}]", scene.fleet);
        let mine = marker.clone();
        let (model, transcript) = FakeModel::deciding(move |asked| {
            if asked.user.iter().any(|text| text.contains(&mine)) {
                decide(asked)
            } else {
                vec![say("This request is another case's.")]
            }
        });
        let home = tempfile::tempdir().expect("a storage home");
        let sandboxes = tempfile::tempdir_in("/tmp").expect("a short sandbox base");
        let prepared = Arc::new(AtomicUsize::new(0));
        let engine = Counting {
            inner: UnsandboxedEngine::new(sandboxes.path().to_owned())
                .expect("a debug build permits the unsandboxed engine"),
            prepared: Arc::clone(&prepared),
        };
        let shutdown = CancellationToken::new();
        let runner = tokio::spawn(runner(&scene, home.path(), engine, model, shutdown.clone()));
        let case = Self {
            scene,
            seat,
            marker,
            runner,
            shutdown,
            prepared,
            _home: home,
            _sandboxes: sandboxes,
        };
        (case, transcript)
    }

    /// `text`, marked as this case's.
    pub(super) fn said(&self, text: &str) -> String {
        format!("{text} {}", self.marker)
    }

    /// Turn `sequence` of `draft` once it settles: its status, answer and
    /// candidate.
    pub(super) async fn settled(&self, draft: &Uuid7, sequence: i64) -> TurnRow {
        let deadline = tokio::time::Instant::now() + SETTLE_DEADLINE;
        let finished = [
            MessageState::Completed.as_str(),
            MessageState::Failed.as_str(),
            MessageState::Cancelled.as_str(),
            MessageState::TimedOut.as_str(),
        ];
        while tokio::time::Instant::now() < deadline {
            if let Some(row) = self.turn(draft, sequence).await
                && finished.contains(&row.status.as_str())
            {
                return row;
            }
            tokio::time::sleep(SETTLE_POLL).await;
        }
        panic!("turn {sequence} of draft {draft} did not settle within {SETTLE_DEADLINE:?}");
    }

    async fn turn(&self, draft: &Uuid7, sequence: i64) -> Option<TurnRow> {
        let row: Option<TurnColumns> = sqlx::query_as(
            "SELECT status, answer_text, candidate_id, failure_reason FROM thread.turns \
             WHERE thread_id = $1 AND sequence = $2",
        )
        .bind(draft)
        .bind(sequence)
        .fetch_optional(&mut *self.connection().await)
        .await
        .expect("the draft's turn reads");
        row.map(|(status, answer, candidate, failure_reason)| TurnRow {
            status,
            answer,
            candidate,
            failure_reason,
        })
    }

    /// The draft's candidates: how many, and the head's revision and
    /// Definition.
    pub(super) async fn candidates(&self, draft: &Uuid7) -> (i64, Option<(i64, Value)>) {
        let mut connection = self.connection().await;
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM setup.candidates WHERE draft_id = $1")
                .bind(draft)
                .fetch_one(&mut *connection)
                .await
                .expect("the draft's candidates count");
        let head: Option<(i64, Value)> = sqlx::query_as(
            "SELECT c.revision, c.definition FROM setup.drafts d \
             JOIN setup.candidates c ON c.id = d.head_candidate_id WHERE d.id = $1",
        )
        .bind(draft)
        .fetch_optional(&mut *connection)
        .await
        .expect("the draft's head reads");
        (count, head)
    }

    /// The draft's runs that reached the runner, by fence.
    pub(super) async fn runs(&self, draft: &Uuid7) -> Vec<Uuid7> {
        sqlx::query_scalar("SELECT id FROM thread.runs WHERE draft_id = $1 ORDER BY fencing_token")
            .bind(draft)
            .fetch_all(&mut *self.connection().await)
            .await
            .expect("the draft's runs")
    }

    /// How many sandboxes the runner prepared.
    pub(super) fn prepared(&self) -> usize {
        self.prepared.load(Ordering::SeqCst)
    }

    pub(super) async fn connection(&self) -> sqlx::pool::PoolConnection<sqlx::Postgres> {
        self.scene
            .booted
            .database
            .acquire()
            .await
            .expect("a pooled connection")
    }

    /// Stops the runner, removes the draft, and retires the scenario.
    pub(super) async fn finish(self, supervisor: Supervisor, draft: &afd_setup::types::Draft) {
        self.shutdown.cancel();
        self.runner
            .await
            .expect("the runner task joins")
            .expect("the runner stops cleanly on shutdown");
        self.seat.remove(&self.scene, draft).await;
        supervisor.shutdown().await;
        self.scene.cleanup().await;
    }
}

/// A setup turn's status, answer, candidate and failure reason, as read.
type TurnColumns = (String, Option<String>, Option<Uuid7>, Option<String>);

/// A setup turn as its run left it.
#[derive(Debug)]
pub(super) struct TurnRow {
    pub(super) status: String,
    pub(super) answer: Option<String>,
    pub(super) candidate: Option<Uuid7>,
    pub(super) failure_reason: Option<String>,
}

/// The unsandboxed engine, counting every prepare: a setup run must make
/// none.
#[derive(Debug)]
struct Counting {
    inner: UnsandboxedEngine,
    prepared: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl Engine for Counting {
    async fn prepare(&self, request: SandboxRequest<'_>) -> afr_sandbox::Result<Box<dyn Sandbox>> {
        self.prepared.fetch_add(1, Ordering::SeqCst);
        self.inner.prepare(request).await
    }
}

/// The Rust runner's supervisor, pointed at `scene`'s daemon, hosting the
/// real loop over `model` and building sandboxes, if ever, with `engine`.
fn runner(
    scene: &Scenario,
    home: &Path,
    engine: Counting,
    model: FakeModel,
    shutdown: CancellationToken,
) -> impl Future<Output = afr_supervisor::Result<()>> + use<> {
    let home = home.to_string_lossy().into_owned();
    let origin = format!(
        "https://localhost:{}",
        scene.booted.runner_stream_address.port()
    );
    let root = crate::support::runner_stream::root_file()
        .to_string_lossy()
        .into_owned();
    let env = af_core::env::MapEnv::from_pairs([
        (ENV_API_URL, scene.base.as_str()),
        (ENV_STREAM_URL, origin.as_str()),
        (ENV_STREAM_CA, root.as_str()),
        (ENV_RUNNER_TOKEN, scene.token.as_str()),
        (ENV_STORAGE_HOME, home.as_str()),
    ]);
    let (config, home) = afr_supervisor::boot(&env).expect("the scenario's env is valid");
    let network = af_egress::Network::new().expect("the runner's network builds");
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
        workspace_direct_io: None,
        egress: false,
    }
}
