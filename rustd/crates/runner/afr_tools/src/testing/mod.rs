//! What the handler suites share: one call, made the way the router makes it,
//! a run that sends through a fake, and an executor served in-process for
//! the file tools to cross.

#![expect(
    clippy::expect_used,
    reason = "test support: a harness that cannot start should fail the test loudly"
)]

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use af_core::clock::FixedClock;
use af_egress::Egress;
use af_egress::fixture::{RUN_ID, policy};
use af_egress::testing::{CountingMint, RecordingTransport, Sent};
use af_wire::policy::ExecutionPolicy;
use afr_executor::{Client, Executor};
use afr_memory::Loaded;
use guffbolt::ToolOutput;
use jiff::Timestamp;
use tokio::task::JoinHandle;

use crate::catalog::{Catalog, Entry};
use crate::egress::SharedTransport;
use crate::run::Run;
use crate::runtime::{CatalogTool, ToolContext};
use crate::selection::Selection;

/// When every egress suite's clock starts.
pub(crate) const START: Timestamp = Timestamp::constant(1_700_000_000, 0);
/// What the fixture's mint answers for `github`.
pub(crate) const MINTED: &str = "ghs_minted_token";
/// A minted token's lifetime: one hour.
pub(crate) const HOUR: Duration = Duration::from_hours(1);
/// How long a suite waits on the executor before failing.
const PATIENCE: Duration = Duration::from_secs(20);

/// The catalog the runner hosts, its egress tools sending into a recorder
/// that answers everything 200; the receiver keeps the recorder answering.
pub(crate) fn hosted() -> (Catalog, Receiver<Sent>) {
    let (transport, sent) = RecordingTransport::replying(200, "");
    (Catalog::hosted(Arc::new(transport)), sent)
}

/// The handler `selection` offers for `entry`.
pub(crate) fn offered<'c>(selection: &Selection<'c>, entry: &Entry) -> &'c CatalogTool {
    selection
        .tool(entry.name())
        .expect("the runner hosts every sandbox-side tool")
}

/// The real executor, served in-process on a scratch socket over a scratch
/// workspace, and a client on it: a file call crosses what a sandbox's does.
pub(crate) struct Live {
    pub(crate) client: Client,
    /// The workspace on the host, for a suite to plant and inspect files.
    pub(crate) root: PathBuf,
    server: JoinHandle<afr_executor::Result<()>>,
    scratch: tempfile::TempDir,
}

impl Live {
    /// Serves an executor and connects to it.
    pub(crate) async fn start() -> Self {
        let scratch = tempfile::tempdir().expect("a scratch directory");
        let socket = scratch.path().join("executor.sock");
        let root = scratch.path().join("workspace");
        std::fs::create_dir(&root).expect("the workspace directory");
        let server = tokio::spawn({
            let (socket, root) = (socket.clone(), root.clone());
            async move { afr_executor::serve(&socket, &root).await }
        });
        let client = Client::connect_within(&socket, PATIENCE)
            .await
            .expect("the executor answers on its socket");
        Self {
            client,
            root,
            server,
            scratch,
        }
    }

    /// A directory beside the workspace, outside it, for a link to point at.
    pub(crate) fn outside(&self) -> PathBuf {
        let outside = self.scratch.path().join("outside");
        std::fs::create_dir_all(&outside).expect("the outside directory");
        outside
    }

    /// Hangs up and waits for the executor to stop.
    pub(crate) async fn stop(self) {
        drop(self.client);
        tokio::time::timeout(PATIENCE, self.server)
            .await
            .expect("the executor stops once its client hangs up")
            .expect("the server task finishes")
            .expect("the executor served without failing");
    }
}

/// Calls `tool` with `arguments` from the supervisor, with `run`'s state.
pub(crate) async fn call(
    tool: &CatalogTool,
    run: &Run<'_>,
    arguments: serde_json::Value,
) -> ToolOutput {
    tool.call(
        &arguments,
        ToolContext {
            executor: None,
            run,
        },
    )
    .await
}

/// Calls `tool` with `arguments` as the router calls a sandbox-side tool:
/// with `executor`, and `run`'s state.
pub(crate) async fn call_in(
    tool: &CatalogTool,
    executor: &dyn Executor,
    run: &Run<'_>,
    arguments: serde_json::Value,
) -> ToolOutput {
    tool.call(
        &arguments,
        ToolContext {
            executor: Some(executor),
            run,
        },
    )
    .await
}

/// What an egress suite holds for one run: the policy the guard admits
/// under, its clock, and a mint answering [`MINTED`].
pub(crate) struct RunFixture {
    pub(crate) policy: ExecutionPolicy<'static>,
    pub(crate) clock: FixedClock,
    pub(crate) mint: CountingMint,
}

impl RunFixture {
    /// The fixture policy, with `read_only` as given.
    pub(crate) fn new(read_only: bool) -> Self {
        let clock = FixedClock::at(START);
        Self {
            policy: policy(read_only),
            mint: CountingMint::answering(MINTED, HOUR, clock.clone()),
            clock,
        }
    }

    /// A run sending through this run's guard.
    pub(crate) fn run(&self) -> Run<'_> {
        self.run_reaching(&crate::verbs::CLOSED)
    }

    /// A run sending through this run's guard and reaching `verbs`.
    pub(crate) fn run_reaching<'r>(&'r self, verbs: &'r dyn crate::verbs::RunVerbs) -> Run<'r> {
        Run::new(
            Box::new(Loaded::default()),
            Egress::new(RUN_ID, &self.policy, &self.mint, &self.clock),
            verbs,
        )
    }
}

/// A recording transport answering every request `status` with `body`, as
/// the handlers hold it, and where what it was handed arrives.
pub(crate) fn replying(status: u16, body: &str) -> (SharedTransport, Receiver<Sent>) {
    let (transport, sent) = RecordingTransport::replying(status, body);
    (Arc::new(transport), sent)
}

mod recorded;

pub(crate) use recorded::{
    Asked, OwnedCall, OwnedFleetsCall, RecordingVerbs, STEERED_EVENT, run_with,
};
