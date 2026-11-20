//! A healthy daemon, as this crate's own suites fake it.
//!
//! A suite outside the crate fakes the same one from here (`M-TEST-UTIL`):
//! the fixture run, the identifiers it carries, and the reply a healthy
//! daemon gives each route.

#[cfg(feature = "test-util")]
mod native;
#[cfg(feature = "test-util")]
mod session;
#[cfg(feature = "test-util")]
pub use native::NativeDaemon;
#[cfg(feature = "test-util")]
pub use session::Session;

use std::borrow::Cow;

use af_wire::context::ContextLoadResponse;
use af_wire::fold::FoldResponse;
use af_wire::memory::{MemoryCaptureResponse, MemoryLoadResponse, MemoryRecallResponse};
use af_wire::paths::{
    RUN_ITEMS_SUFFIX, RUNNER_MEMORY, RUNNER_MEMORY_RECALL_SUFFIX, RUNNER_RUNS, RUNNER_SELF,
};
use af_wire::report::{RenewResponse, ReportResponse};
use af_wire::runner::{AssignedPolicy, HeartbeatResponse, HeartbeatStatus, SelfResponse};
use af_wire::tool_detail::ToolCallRecordsStored;
use serde::Serialize;
use serde_json::Value;

use crate::client::Verb;

/// The run every suite is granted, with its identifiers left for the suite
/// to fill.
pub const RUN_JSON: &str = include_str!("../test_support/run.json");
/// A canonical run identifier.
pub const RUN_ID: &str = "01890a5d-ac96-774b-bcce-b302099a8057";
/// A canonical fleet identifier.
pub const FLEET_ID: &str = "01890a5d-ac96-774b-bcce-b302099a8058";
/// The fencing token the fixture run carries.
pub const FENCING: u64 = 504;
/// The scope a healthy daemon assigns a setup run's draft context.
pub const DRAFT_SCOPE: &str = "draft:01890a5d-ac96-774b-bcce-b302099a8058";
/// The id a healthy daemon names this runner by.
pub const RUNNER_ID: &str = "01890a5d-ac96-774b-bcce-b302099a8059";
/// The host a healthy daemon has this runner on.
pub const RUNNER_HOST: &str = "host-7";
/// The GET method, as a request line spells it.
const GET: &str = "GET";
/// The status a healthy daemon reports this runner in.
const ACTIVE: &str = "active";

/// What sets one healthy daemon apart from another: the posture it assigns,
/// the cadence it asks for, and when a renewal is granted until.
#[derive(Debug, Clone)]
pub struct Healthy {
    /// The assignment every beat carries.
    pub assigned: AssignedPolicy<'static>,
    /// How often to beat and to ask for a run again, in milliseconds.
    pub interval_ms: u32,
    /// When a renewal is granted until.
    pub granted_until: jiff::Timestamp,
}

impl Healthy {
    /// The reply to a `method` request for `path`: a route with no reply of
    /// its own is acknowledged.
    #[must_use]
    pub fn reply(&self, method: &str, path: &str) -> Value {
        self.to(routed(method, path))
    }

    /// The reply to `verb`, or the acknowledgement when it has none of its
    /// own.
    pub(crate) fn to(&self, verb: Option<Verb>) -> Value {
        match verb {
            // Authoring fixtures must provide their attempt-bound receipts.
            Some(
                Verb::AuthoringModel
                | Verb::AuthoringActivity
                | Verb::AuthoringRenew
                | Verb::AuthoringReport
                // Durable-task fixtures also supply their native receipts explicitly.
                | Verb::TaskInputs
                | Verb::TaskBindings
                | Verb::TaskInputReceipt
                | Verb::TaskDecisionReserve
                | Verb::TaskDecisionRecord
                | Verb::TaskEffectPublish
                | Verb::TaskEffects
                | Verb::TaskEffect
                | Verb::TaskCheckpointPlan
                | Verb::TaskCheckpointCommit
                | Verb::TaskCheckpointPageWrite
                | Verb::TaskCheckpointPage
                | Verb::TaskCheckpointObjectsRegister
                | Verb::TaskCheckpointUploads
                | Verb::TaskCheckpointAuthorizations
                | Verb::TaskCheckpointPin
                | Verb::TurnCheckpoint
                // A setup run's save names a version only its fixture knows.
                | Verb::ContextSave
                // The native opening is a stream, never an HTTP fixture route.
                | Verb::Open,
            ) => Value::Null,
            // A healthy daemon whose draft has no saved context yet.
            Some(Verb::ContextLoad) => value(&ContextLoadResponse {
                scope: DRAFT_SCOPE.to_owned(),
                resource_version: 0,
                checkpoint: None,
            }),
            Some(Verb::Heartbeat) => self.heartbeat(),
            Some(Verb::Renew) => self.renewed(),
            Some(Verb::Load) => value(&MemoryLoadResponse {
                memory: Vec::new(),
                shared: Vec::new(),
                publish: false,
            }),
            Some(Verb::Capture) => value(&MemoryCaptureResponse {
                stored: 1,
                skipped: 0,
            }),
            Some(Verb::Recall) => value(&MemoryRecallResponse {
                memory: Vec::new(),
                shared: Vec::new(),
            }),
            Some(Verb::Items) => value(&ToolCallRecordsStored {
                stored_count: 1,
                skipped_count: 0,
            }),
            Some(Verb::Me) => self.me(),
            // A healthy daemon with no message waiting for the run.
            Some(Verb::Folds) => value(&FoldResponse::default()),
            Some(
                Verb::Progress
                | Verb::Report
                | Verb::Mint
                | Verb::ScheduleCreate
                | Verb::ScheduleList
                | Verb::ScheduleUpdate
                | Verb::ScheduleDelete
                | Verb::ScheduleRun
                | Verb::ScheduleRuns
                | Verb::Message
                | Verb::FleetList
                | Verb::FleetTurns
                | Verb::FleetSteer,
            )
            | None => value(&ReportResponse { ok: true, hold: None }),
        }
    }

    /// A renewal: the instant this daemon granted, with nothing recorded.
    fn renewed(&self) -> Value {
        value(&RenewResponse {
            run_expires_at: self.granted_until,
            recorded_through: None,
        })
    }

    /// A healthy beat: the assignment and the cadence this daemon was given.
    fn heartbeat(&self) -> Value {
        value(&HeartbeatResponse {
            status: HeartbeatStatus::Ok,
            assigned_policy: self.assigned.clone(),
            degraded: false,
            degraded_reason: None,
            selftest_requested: false,
            release_holds: Vec::new(),
            heartbeat_interval_ms: self.interval_ms,
        })
    }

    /// This runner as a healthy daemon names it, on the assigned tier.
    fn me(&self) -> Value {
        value(&SelfResponse {
            id: RUNNER_ID.into(),
            status: ACTIVE.into(),
            host_id: RUNNER_HOST.into(),
            sandbox_tier: Cow::Owned(
                value(&self.assigned.sandbox_tier)
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
            ),
            last_seen_at: jiff::Timestamp::UNIX_EPOCH,
            assigned_policy: self.assigned.clone(),
            achievable: None,
            degraded: false,
            degraded_reason: None,
        })
    }
}

/// The verb a `method` request for `path` is, among those a healthy daemon
/// answers with a reply of its own; none for a route it only acknowledges.
fn routed(method: &str, path: &str) -> Option<Verb> {
    let path = path.split_once('?').map_or(path, |(path, _query)| path);
    let held = path
        .strip_prefix(RUNNER_RUNS)
        .and_then(|rest| rest.strip_prefix('/'))
        .and_then(|rest| rest.split_once('/'))
        .map(|(_run, suffix)| suffix);
    let memory = path.starts_with(RUNNER_MEMORY);
    match (path, held) {
        (RUNNER_SELF, _) => Some(Verb::Me),
        (_, Some(RUN_ITEMS_SUFFIX)) => Some(Verb::Items),
        _ if memory && path.ends_with(RUNNER_MEMORY_RECALL_SUFFIX) => Some(Verb::Recall),
        _ if memory && method == GET => Some(Verb::Load),
        _ if memory => Some(Verb::Capture),
        _ => None,
    }
}

/// `reply` as the JSON a daemon sends.
fn value(reply: &impl Serialize) -> Value {
    serde_json::to_value(reply)
        .unwrap_or_else(|never| unreachable!("a wire reply always encodes: {never}"))
}

/// A loopback listener a fake daemon or collector serves on, aborted when
/// its owner drops it.
#[cfg(feature = "test-util")]
#[derive(Debug)]
pub struct Listener {
    /// Where the fake answers, as `http://127.0.0.1:<port>`.
    pub url: String,
    stop: tokio_util::sync::CancellationToken,
    task: tokio::task::JoinHandle<std::io::Result<()>>,
}

#[cfg(feature = "test-util")]
impl Listener {
    /// Serves `app` on a fresh loopback port.
    ///
    /// # Errors
    /// No loopback port binds, or the bound one has no address.
    pub async fn serve(app: axum::Router) -> std::io::Result<Self> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let url = format!("http://{}", listener.local_addr()?);
        let stop = tokio_util::sync::CancellationToken::new();
        let stopped = stop.clone();
        let task = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(stopped.cancelled_owned())
                .await
        });
        Ok(Self { url, stop, task })
    }

    /// Stops serving and waits for the listener to finish.
    ///
    /// # Errors
    /// The listener did not finish within ten seconds, panicked, or failed.
    pub async fn stop(&mut self) -> std::io::Result<()> {
        self.stop.cancel();
        tokio::time::timeout(std::time::Duration::from_secs(10), &mut self.task)
            .await
            .map_err(std::io::Error::other)?
            .map_err(std::io::Error::other)?
    }
}

#[cfg(feature = "test-util")]
impl Drop for Listener {
    fn drop(&mut self) {
        self.stop.cancel();
        self.task.abort();
    }
}

#[cfg(test)]
mod tests;
