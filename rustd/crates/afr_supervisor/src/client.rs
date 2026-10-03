//! The runner verbs: one transport seam, and the typed calls built on it.
//!
//! [`RunnerApi`] is one method — send this verb to this path with this body —
//! so a fake answers every verb from one closure and production implements it
//! once over HTTP ([`HttpRunnerApi`]). [`ControlPlane`] is the typed face the
//! duties call: it builds each path from `afd_wire::paths`, serializes the
//! body, and hands back the reply as a [`Body`] that decodes on demand into a
//! wire type borrowing from it, so a lease is parsed without copying a field.

use std::borrow::Cow;
use std::fmt;
use std::time::Duration;

use afd_core::error_code::{self, ErrorCode};
use afd_core::id::Uuid7;
use afd_wire::activity::ActivityRequest;
use afd_wire::credentials::MintCredentialRequest;
use afd_wire::memory::{MemoryPushRequest, MemoryRecallRequest};
use afd_wire::paths;
use afd_wire::report::{RenewRequest, RenewResponse};
use afd_wire::runner::HeartbeatRequest;
use backon::{ExponentialBuilder, Retryable as _};
use bytes::Bytes;
use serde::{Deserialize, Serialize};

use crate::error::{self, Result};

mod http;

pub(crate) use self::http::HttpRunnerApi;

/// Attempts a retryable call gets before its caller decides what a failure
/// means: keep the report spooled, fail the lease's start.
const RETRY_ATTEMPTS: usize = 4;
/// The first pause between attempts; each later one doubles, with jitter.
const RETRY_FIRST_DELAY: Duration = Duration::from_millis(250);
/// The longest pause between attempts, however many have failed.
const RETRY_MAX_DELAY: Duration = Duration::from_secs(30);
const EVENT_STARTED: &str = "daemon_call_started";
const EVENT_COMPLETED: &str = "daemon_call_completed";
const EVENT_FAILED: &str = "daemon_call_failed";

/// One runner verb.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Verb {
    /// Liveness, capability, assignment.
    Heartbeat,
    /// The next event to run.
    Lease,
    /// More time on a held lease.
    Renew,
    /// Live-tail frames for a held lease.
    Activity,
    /// A lease's terminal result.
    Report,
    /// A fleet's memory at lease start.
    Hydrate,
    /// A fleet's memory written back.
    Capture,
    /// A search of a fleet's memory past the window.
    Recall,
    /// A fleet bundle by content hash.
    Bundle,
    /// A scoped credential for a held lease.
    Mint,
    /// Finished calls' full records for a held lease.
    Records,
    /// The runner's own row: which runner this is, as the daemon names it.
    Me,
}

impl Verb {
    /// Whether this verb reads (`GET`) rather than reports (`POST`).
    pub(crate) const fn reads(self) -> bool {
        matches!(self, Self::Hydrate | Self::Bundle | Self::Me)
    }

    /// The verb as a log line and an error name it.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Heartbeat => "heartbeat",
            Self::Lease => "lease",
            Self::Renew => "renew",
            Self::Activity => "activity",
            Self::Report => "report",
            Self::Hydrate => "hydrate",
            Self::Capture => "capture",
            Self::Recall => "recall",
            Self::Bundle => "bundle",
            Self::Mint => "mint",
            Self::Records => "records",
            Self::Me => "me",
        }
    }

    /// The registry code a failure of this verb is logged under.
    pub(crate) const fn code(self) -> ErrorCode {
        match self {
            Self::Bundle => error_code::FLEET_BUNDLE_FETCH_FAILED,
            Self::Hydrate | Self::Capture | Self::Recall => error_code::MEM_UNAVAILABLE,
            Self::Heartbeat
            | Self::Lease
            | Self::Renew
            | Self::Activity
            | Self::Report
            | Self::Mint
            | Self::Records
            | Self::Me => error_code::INTERNAL_OPERATION_FAILED,
        }
    }
}

impl fmt::Display for Verb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One request: the verb, the path it goes to, and its body if it has one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Call {
    /// What the request does.
    pub(crate) verb: Verb,
    /// The path under the daemon's base address. A fixed route borrows its
    /// constant; only a path naming a lease or a fleet is built.
    pub(crate) path: Cow<'static, str>,
    /// The JSON body; a read has none.
    pub(crate) body: Option<Bytes>,
}

/// Sends one runner verb and returns the successful reply's bytes.
///
/// Every failure comes back classified: [`crate::Error::is_retryable`] for a
/// transport failure, a 5xx or a 429, a refusal for any other 4xx.
#[async_trait::async_trait]
pub(crate) trait RunnerApi: Send + Sync + fmt::Debug {
    /// Sends `call` and returns the reply body of a 2xx.
    async fn send(&self, call: Call) -> Result<Bytes>;
}

/// A successful reply, decoded on demand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Body {
    verb: Verb,
    bytes: Bytes,
}

impl Body {
    /// Decodes the reply into `T`, borrowing every text field from it, through
    /// the gate that refuses a JSON array read into a struct's fields in order.
    pub(crate) fn decode<'a, T: Deserialize<'a>>(&'a self) -> Result<T> {
        afd_core::json::object_from_slice(&self.bytes).map_err(error::malformed(self.verb))
    }
}

/// The runner verbs, typed: paths, bodies and replies.
///
/// Public so the calls built on it — [`crate::credentials::mint`] — can name
/// it; only this crate constructs one.
#[derive(Debug)]
pub struct ControlPlane {
    api: Box<dyn RunnerApi>,
}

impl ControlPlane {
    /// Speaks every verb through `api`.
    pub(crate) fn new(api: Box<dyn RunnerApi>) -> Self {
        Self { api }
    }

    /// Beats, carrying the capability report and any self-test.
    pub(crate) async fn heartbeat(&self, request: &HeartbeatRequest<'_>) -> Result<Body> {
        let path = Cow::Borrowed(paths::RUNNER_HEARTBEATS);
        self.post(Verb::Heartbeat, path, request).await
    }

    /// Polls for the next lease.
    pub(crate) async fn lease(&self) -> Result<Body> {
        let path = Cow::Borrowed(paths::RUNNER_LEASES);
        self.send(Verb::Lease, path, None).await
    }

    /// Renews a held lease and returns its new expiry, in Unix milliseconds.
    /// The token counts ride later work; this meters the run fee, which the
    /// daemon owes either way.
    pub(crate) async fn renew(&self, lease_id: &Uuid7) -> Result<i64> {
        let path = lease_path(lease_id, paths::LEASE_RENEW_SUFFIX);
        let body = self
            .post(Verb::Renew, path, &RenewRequest::default())
            .await?;
        body.decode::<RenewResponse>()
            .map(|renewed| renewed.lease_expires_at)
    }

    /// Posts one batch of live-tail frames.
    pub(crate) async fn activity(
        &self,
        lease_id: &Uuid7,
        request: &ActivityRequest<'_>,
    ) -> Result<()> {
        let path = lease_path(lease_id, paths::LEASE_ACTIVITY_SUFFIX);
        self.post(Verb::Activity, path, request).await.map(drop)
    }

    /// Posts a report, byte for byte as it was spooled.
    pub(crate) async fn report(&self, report: Bytes) -> Result<()> {
        let path = Cow::Borrowed(paths::RUNNER_REPORTS);
        self.send(Verb::Report, path, Some(report)).await.map(drop)
    }

    /// Posts one body of finished calls' full records, already encoded.
    pub(crate) async fn tool_calls(&self, lease_id: &Uuid7, body: Bytes) -> Result<()> {
        let path = lease_path(lease_id, paths::LEASE_TOOL_CALLS_SUFFIX);
        self.send(Verb::Records, path, Some(body)).await.map(drop)
    }

    /// Reads a fleet's memory.
    pub(crate) async fn hydrate(&self, fleet_id: &Uuid7) -> Result<Body> {
        self.send(Verb::Hydrate, memory_path(fleet_id), None).await
    }

    /// Writes a fleet's memory back, fenced by the lease's token.
    pub(crate) async fn capture(
        &self,
        fleet_id: &Uuid7,
        request: &MemoryPushRequest<'_>,
    ) -> Result<()> {
        self.post(Verb::Capture, memory_path(fleet_id), request)
            .await
            .map(drop)
    }

    /// Searches a fleet's memory past the window, fenced by the lease's token.
    pub(crate) async fn recall(
        &self,
        fleet_id: &Uuid7,
        request: &MemoryRecallRequest<'_>,
    ) -> Result<Body> {
        let path = Cow::Owned(format!(
            "{}/{}",
            memory_path(fleet_id),
            paths::RUNNER_MEMORY_RECALL_SUFFIX
        ));
        self.post(Verb::Recall, path, request).await
    }

    /// Mints a scoped credential for a held lease.
    pub(crate) async fn mint(&self, request: &MintCredentialRequest<'_>) -> Result<Body> {
        let path = Cow::Borrowed(paths::RUNNER_CREDENTIALS_MINT);
        self.post(Verb::Mint, path, request).await
    }

    /// Reads this runner's own row.
    pub(crate) async fn me(&self) -> Result<Body> {
        self.send(Verb::Me, Cow::Borrowed(paths::RUNNER_SELF), None)
            .await
    }

    /// Downloads a bundle's canonical tar.
    pub(crate) async fn bundle(&self, content_hash: &str) -> Result<Bytes> {
        let path = Cow::Owned(format!("{}/{content_hash}", paths::RUNNER_BUNDLES));
        self.send(Verb::Bundle, path, None)
            .await
            .map(|body| body.bytes)
    }

    async fn post<T: Serialize + Sync>(
        &self,
        verb: Verb,
        path: Cow<'static, str>,
        body: &T,
    ) -> Result<Body> {
        let bytes = serde_json::to_vec(body).map_err(error::encode)?;
        self.send(verb, path, Some(Bytes::from(bytes))).await
    }

    /// The one place a verb leaves the runner, so every call is logged once.
    async fn send(&self, verb: Verb, path: Cow<'static, str>, body: Option<Bytes>) -> Result<Body> {
        let event = EVENT_STARTED;
        tracing::debug!(verb = verb.as_str(), event);
        match self.api.send(Call { verb, path, body }).await {
            Ok(bytes) => {
                let event = EVENT_COMPLETED;
                tracing::debug!(verb = verb.as_str(), event);
                Ok(Body { verb, bytes })
            }
            Err(failure) => {
                let code = failure.code().as_str();
                let event = EVENT_FAILED;
                tracing::debug!(error_code = code, verb = verb.as_str(), event);
                Err(failure)
            }
        }
    }
}

/// A path under one held lease.
fn lease_path(lease_id: &Uuid7, suffix: &str) -> Cow<'static, str> {
    Cow::Owned(format!("{}/{lease_id}/{suffix}", paths::RUNNER_LEASES))
}

/// One fleet's memory.
fn memory_path(fleet_id: &Uuid7) -> Cow<'static, str> {
    Cow::Owned(format!("{}/{fleet_id}", paths::RUNNER_MEMORY))
}

/// The one backoff every retried path shares: exponential from
/// [`RETRY_FIRST_DELAY`], capped at [`RETRY_MAX_DELAY`], jittered so runners
/// that lost the daemon together do not return together.
pub(crate) fn backoff() -> ExponentialBuilder {
    ExponentialBuilder::default()
        .with_min_delay(RETRY_FIRST_DELAY)
        .with_max_delay(RETRY_MAX_DELAY)
        .with_jitter()
}

/// The pauses for a path that never gives up: polls, the spool drain, boot.
pub(crate) fn endless() -> backon::ExponentialBackoff {
    backon::BackoffBuilder::build(backoff().without_max_times())
}

/// Runs `call` until it succeeds, fails terminally, or exhausts its attempts.
///
/// Only a retryable failure is retried: a 4xx is final on its first answer.
pub(crate) async fn retrying<T, F, Fut>(call: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    call.retry(backoff().with_max_times(RETRY_ATTEMPTS))
        .when(crate::Error::is_retryable)
        .await
}

#[cfg(test)]
#[path = "client/tests.rs"]
mod tests;
