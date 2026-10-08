//! What the handler suites share: one call, made the way the router makes it,
//! a lease that sends through a fake, and an executor served in-process for
//! the file tools to cross.

#![expect(
    clippy::expect_used,
    reason = "test support: a harness that cannot start should fail the test loudly"
)]

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use afd_core::clock::{FixedClock, UnixMillis};
use afd_wire::policy::ExecutionPolicy;
use afr_egress::Egress;
use afr_egress::fixture::{LEASE_ID, policy};
use afr_egress::testing::{CountingMint, RecordingTransport, Sent};
use afr_executor::{Client, Executor};
use afr_memory::Hydrated;
use tokio::task::JoinHandle;

use crate::catalog::{Catalog, Entry};
use crate::egress::SharedTransport;
use crate::lease::Lease;
use crate::runtime::{Tool, ToolContext, ToolOutput};
use crate::selection::Selection;

/// When every egress suite's clock starts.
pub(crate) const START: UnixMillis = UnixMillis::from_millis(1_700_000_000_000);
/// What the fixture's mint answers for `github`.
pub(crate) const MINTED: &str = "ghs_minted_token";
/// A minted token's lifetime: one hour.
pub(crate) const HOUR: i64 = 3_600_000;
/// How long a suite waits on the executor before failing.
const PATIENCE: Duration = Duration::from_secs(20);

/// The catalog the runner hosts, its egress tools sending into a recorder
/// that answers everything 200; the receiver keeps the recorder answering.
pub(crate) fn hosted() -> (Catalog, Receiver<Sent>) {
    let (transport, sent) = RecordingTransport::replying(200, "");
    (Catalog::hosted(Arc::new(transport)), sent)
}

/// The handler `selection` offers for `entry`.
pub(crate) fn offered<'c>(selection: &Selection<'c>, entry: &Entry) -> &'c dyn Tool {
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

/// Calls `tool` with `arguments` from the supervisor, with `lease`'s state.
pub(crate) async fn call(
    tool: &dyn Tool,
    lease: &Lease<'_>,
    arguments: serde_json::Value,
) -> ToolOutput {
    tool.call(
        &arguments,
        ToolContext {
            executor: None,
            lease,
        },
    )
    .await
}

/// Calls `tool` with `arguments` as the router calls a sandbox-side tool:
/// with `executor`, and `lease`'s state.
pub(crate) async fn call_in(
    tool: &dyn Tool,
    executor: &dyn Executor,
    lease: &Lease<'_>,
    arguments: serde_json::Value,
) -> ToolOutput {
    tool.call(
        &arguments,
        ToolContext {
            executor: Some(executor),
            lease,
        },
    )
    .await
}

/// What an egress suite holds for one lease: the policy the guard admits
/// under, its clock, and a mint answering [`MINTED`].
pub(crate) struct Run {
    pub(crate) policy: ExecutionPolicy<'static>,
    pub(crate) clock: FixedClock,
    pub(crate) mint: CountingMint,
}

impl Run {
    /// The fixture policy, with `read_only` as given.
    pub(crate) fn new(read_only: bool) -> Self {
        let clock = FixedClock::at(START);
        Self {
            policy: policy(read_only),
            mint: CountingMint::answering(MINTED, HOUR, clock.clone()),
            clock,
        }
    }

    /// A lease sending through this run's guard.
    pub(crate) fn lease(&self) -> Lease<'_> {
        self.lease_reaching(&crate::verbs::CLOSED)
    }

    /// A lease sending through this run's guard and reaching `verbs`.
    pub(crate) fn lease_reaching<'r>(
        &'r self,
        verbs: &'r dyn crate::verbs::LeaseVerbs,
    ) -> Lease<'r> {
        Lease::new(
            Box::new(Hydrated::default()),
            Egress::new(LEASE_ID, &self.policy, &self.mint, &self.clock),
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

/// One call a tool made through the lease's verbs, owned so a suite can read
/// it after the call returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Asked {
    /// A schedules call, field by field.
    Schedules(OwnedCall),
    /// A message, as the text that left the runner.
    Message(String),
}

/// A [`crate::verbs::ScheduleCall`] with every field owned, compared whole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OwnedCall {
    Create {
        cron: String,
        timezone: Option<String>,
        message: String,
        once: bool,
    },
    List,
    Update {
        schedule: String,
        cron: Option<String>,
        timezone: Option<String>,
        message: Option<String>,
        paused: Option<bool>,
    },
    Delete(String),
    Run(String),
    Runs {
        schedule: String,
        limit: Option<u32>,
        starting_after: Option<String>,
    },
}

impl From<crate::verbs::ScheduleCall<'_>> for OwnedCall {
    fn from(call: crate::verbs::ScheduleCall<'_>) -> Self {
        use crate::verbs::ScheduleCall;
        let owned = |text: Option<&str>| text.map(str::to_owned);
        match call {
            ScheduleCall::Create {
                cron,
                timezone,
                message,
                once,
            } => Self::Create {
                cron: cron.to_owned(),
                timezone: owned(timezone),
                message: message.to_owned(),
                once,
            },
            ScheduleCall::List => Self::List,
            ScheduleCall::Update {
                schedule,
                cron,
                timezone,
                message,
                paused,
            } => Self::Update {
                schedule: schedule.as_str().to_owned(),
                cron: owned(cron),
                timezone: owned(timezone),
                message: owned(message),
                paused,
            },
            ScheduleCall::Delete { schedule } => Self::Delete(schedule.as_str().to_owned()),
            ScheduleCall::Run { schedule } => Self::Run(schedule.as_str().to_owned()),
            ScheduleCall::Runs {
                schedule,
                limit,
                starting_after,
            } => Self::Runs {
                schedule: schedule.as_str().to_owned(),
                limit,
                starting_after: owned(starting_after),
            },
        }
    }
}

/// Lease verbs answering every call with `answer`, recording what was asked.
#[derive(Debug)]
pub(crate) struct RecordingVerbs {
    answer: Result<String, crate::verbs::Unanswered>,
    delivered: Result<bool, crate::verbs::Unanswered>,
    asked: std::sync::Mutex<Vec<Asked>>,
}

impl RecordingVerbs {
    /// Verbs answering schedules calls with `answer` and messages with
    /// `delivered`.
    pub(crate) fn answering(
        answer: Result<String, crate::verbs::Unanswered>,
        delivered: Result<bool, crate::verbs::Unanswered>,
    ) -> Self {
        Self {
            answer,
            delivered,
            asked: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Everything asked so far, in order.
    pub(crate) fn asked(&self) -> Vec<Asked> {
        self.asked
            .lock()
            .map(|asked| asked.clone())
            .unwrap_or_default()
    }

    fn record(&self, asked: Asked) {
        if let Ok(mut held) = self.asked.lock() {
            held.push(asked);
        }
    }
}

#[async_trait::async_trait]
impl crate::verbs::LeaseVerbs for RecordingVerbs {
    async fn schedules(
        &self,
        call: crate::verbs::ScheduleCall<'_>,
    ) -> Result<String, crate::verbs::Unanswered> {
        self.record(Asked::Schedules(OwnedCall::from(call)));
        self.answer.clone()
    }

    async fn message(&self, text: &str) -> Result<bool, crate::verbs::Unanswered> {
        self.record(Asked::Message(text.to_owned()));
        self.delivered
    }
}

/// A lease whose verbs are `verbs`, with empty memory and closed egress.
pub(crate) fn lease_with(verbs: &dyn crate::verbs::LeaseVerbs) -> Lease<'_> {
    Lease::new(
        Box::new(Hydrated::default()),
        afr_egress::testing::closed(),
        verbs,
    )
}
