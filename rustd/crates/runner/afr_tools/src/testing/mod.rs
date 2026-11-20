//! What the handler suites share: one call, made the way the router makes it,
//! to a tool built the way a run builds it; a run that sends through a fake;
//! and an executor served in-process for the file tools to cross.

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
use afr_executor::{Client, Executor, ExecutorSlot, Unavailable};
use afr_memory::Loaded;
use guffbolt::{CallRef, ToolOutput};
use jiff::Timestamp;
use tokio::task::JoinHandle;

use crate::backed::Backends;
use crate::catalog::{Catalog, Entry};
use crate::egress::SharedTransport;
use crate::run::Run;
use crate::runtime::{RunTool, ToolContext};
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

/// The tool `selection` offers for `entry`, which a call builds the way a
/// run does.
pub(crate) fn offered(selection: &Selection<'_>, entry: &'static Entry) -> &'static Entry {
    assert!(
        selection.offers(entry.name()),
        "the selection offers {}",
        entry.name()
    );
    entry
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

/// Where the one call a suite makes sits: the first of step 1.
fn first_call() -> CallRef {
    CallRef {
        step: 1,
        index: 0,
        call_id: "call-1-0".to_owned(),
    }
}

/// A tool a suite calls: one already built, or one a run builds by name.
pub(crate) trait Callable: Sync {
    /// Calls it with `arguments` as the router does, with `run`'s state and
    /// `executor`, the run's sandbox when it has one.
    fn called(
        &self,
        run: &Run<'_>,
        executor: Option<&dyn Executor>,
        arguments: serde_json::Value,
    ) -> impl Future<Output = ToolOutput> + Send;
}

/// A tool already built: a hosted handler, or one a run built.
impl Callable for RunTool<'_> {
    async fn called(
        &self,
        run: &Run<'_>,
        executor: Option<&dyn Executor>,
        arguments: serde_json::Value,
    ) -> ToolOutput {
        let call_ref = first_call();
        let context = ToolContext {
            executor,
            run,
            call_ref: &call_ref,
        };
        self.call(&arguments, context).await
    }
}

/// The published tool `entry` names, built the way a run builds it, over
/// `executor` when there is one and over no sandbox otherwise; it sends no
/// request.
impl Callable for Entry {
    async fn called(
        &self,
        run: &Run<'_>,
        executor: Option<&dyn Executor>,
        arguments: serde_json::Value,
    ) -> ToolOutput {
        built(self.name(), None, run, executor, arguments).await
    }
}

/// A published tool that sends requests, built the way a run builds it, its
/// requests sent through its transport.
pub(crate) struct Sending {
    entry: &'static Entry,
    transport: SharedTransport,
}

/// `entry`'s tool, sending through `transport`.
pub(crate) fn sending(entry: &'static Entry, transport: SharedTransport) -> Sending {
    Sending { entry, transport }
}

impl Callable for Sending {
    async fn called(
        &self,
        run: &Run<'_>,
        executor: Option<&dyn Executor>,
        arguments: serde_json::Value,
    ) -> ToolOutput {
        let transport = Some(self.transport.as_ref());
        built(self.entry.name(), transport, run, executor, arguments).await
    }
}

/// Builds `name` as `run` builds it, sending through `transport`, in
/// `executor`'s sandbox or none, and calls it with `arguments`.
async fn built(
    name: &str,
    transport: Option<&dyn af_egress::Transport>,
    run: &Run<'_>,
    executor: Option<&dyn Executor>,
    arguments: serde_json::Value,
) -> ToolOutput {
    let slot = executor.map_or_else(
        || ExecutorSlot::unavailable(Unavailable::NoSandbox),
        ExecutorSlot::ready,
    );
    let backends = Backends::new(run, &slot, transport);
    let tools = backends
        .tools(&[name])
        .expect("a run builds every guffbolt tool it names");
    let tool = tools.tool(name).expect("the name is a built tool's");
    tool.called(run, executor, arguments).await
}

/// The executor source of a run with no sandbox.
pub(crate) fn no_sandbox() -> ExecutorSlot<'static> {
    ExecutorSlot::unavailable(Unavailable::NoSandbox)
}

/// Calls `tool` with `arguments` from the supervisor, with `run`'s state.
pub(crate) async fn call<T: Callable + ?Sized>(
    tool: &T,
    run: &Run<'_>,
    arguments: serde_json::Value,
) -> ToolOutput {
    tool.called(run, None, arguments).await
}

/// Calls `tool` with `arguments` as the router calls a sandbox-side tool:
/// with `executor`, and `run`'s state.
pub(crate) async fn call_in<T: Callable + ?Sized>(
    tool: &T,
    executor: &dyn Executor,
    run: &Run<'_>,
    arguments: serde_json::Value,
) -> ToolOutput {
    tool.called(run, Some(executor), arguments).await
}

/// Calls the tool named `name` that `run` builds over its own backends, with
/// `arguments`, the way the router calls it; its notifier sends nowhere, and
/// the run has no sandbox.
pub(crate) async fn call_built(
    run: &Run<'_>,
    name: &str,
    arguments: serde_json::Value,
) -> ToolOutput {
    built(name, None, run, None, arguments).await
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
            Loaded::default(),
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
