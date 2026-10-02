//! The runner verbs: one transport seam, and the typed calls built on it.
//!
//! [`RunnerApi`] is one method — send this verb to this path with this body —
//! so a fake answers every verb from one closure and production implements it
//! once over HTTP ([`HttpRunnerApi`]). [`ControlPlane`] is the typed face the
//! duties call: it builds each path from `afd_wire::paths`, serializes the
//! body, and hands back the reply as a [`Body`] that decodes on demand into a
//! wire type borrowing from it, so a lease is parsed without copying a field.

use std::fmt;
use std::time::Duration;

use afd_core::error_code::{self, ErrorCode};
use afd_core::id::Uuid7;
use afd_wire::activity::ActivityRequest;
use afd_wire::credentials::MintCredentialRequest;
use afd_wire::memory::MemoryPushRequest;
use afd_wire::paths;
use afd_wire::report::RenewRequest;
use afd_wire::runner::HeartbeatRequest;
use backon::{ExponentialBuilder, Retryable as _};
use bytes::Bytes;
use serde::{Deserialize, Serialize};

use crate::error::{self, Result};

mod http;

pub use self::http::HttpRunnerApi;

/// Attempts a retryable call gets before its caller decides what a failure
/// means: keep the report spooled, fail the lease's start.
const RETRY_ATTEMPTS: usize = 4;
/// The first pause between attempts; each later one doubles.
const RETRY_FIRST_DELAY: Duration = Duration::from_millis(250);

/// One runner verb.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Verb {
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
    /// A fleet bundle by content hash.
    Bundle,
    /// A scoped credential for a held lease.
    Mint,
}

impl Verb {
    /// Whether this verb reads (`GET`) rather than reports (`POST`).
    #[must_use]
    pub const fn reads(self) -> bool {
        matches!(self, Self::Hydrate | Self::Bundle)
    }

    /// The registry code a failure of this verb is logged under.
    pub(crate) const fn code(self) -> ErrorCode {
        match self {
            Self::Bundle => error_code::FLEET_BUNDLE_FETCH_FAILED,
            Self::Hydrate | Self::Capture => error_code::MEM_UNAVAILABLE,
            Self::Heartbeat
            | Self::Lease
            | Self::Renew
            | Self::Activity
            | Self::Report
            | Self::Mint => error_code::INTERNAL_OPERATION_FAILED,
        }
    }
}

impl fmt::Display for Verb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

/// One request: the verb, the path it goes to, and its body if it has one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    /// What the request does.
    pub verb: Verb,
    /// The path under the daemon's base address, query included.
    pub path: String,
    /// The JSON body; a read has none.
    pub body: Option<Bytes>,
}

/// Sends one runner verb and returns the successful reply's bytes.
///
/// Every failure comes back classified: [`crate::Error::is_retryable`] for a
/// transport failure, a 5xx or a 429, a refusal for any other 4xx.
#[async_trait::async_trait]
pub trait RunnerApi: Send + Sync + fmt::Debug {
    /// Sends `call` and returns the reply body of a 2xx.
    async fn send(&self, call: Call) -> Result<Bytes>;
}

/// A successful reply, decoded on demand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Body {
    verb: Verb,
    bytes: Bytes,
}

impl Body {
    /// Decodes the reply into `T`, borrowing every text field from it.
    ///
    /// # Errors
    /// A reply that is not `T`'s shape.
    pub fn decode<'a, T: Deserialize<'a>>(&'a self) -> Result<T> {
        serde_json::from_slice(&self.bytes).map_err(error::malformed(self.verb))
    }
}

/// The runner verbs, typed: paths, bodies and replies.
#[derive(Debug)]
pub struct ControlPlane {
    api: Box<dyn RunnerApi>,
}

impl ControlPlane {
    /// Speaks every verb through `api`.
    #[must_use]
    pub fn new(api: Box<dyn RunnerApi>) -> Self {
        Self { api }
    }

    /// Beats, carrying the capability report and any self-test.
    ///
    /// # Errors
    /// Any classified failure of the call.
    pub async fn heartbeat(&self, request: &HeartbeatRequest<'_>) -> Result<Body> {
        self.post(
            Verb::Heartbeat,
            paths::RUNNER_HEARTBEATS.to_owned(),
            request,
        )
        .await
    }

    /// Polls for the next lease.
    ///
    /// # Errors
    /// Any classified failure of the call.
    pub async fn lease(&self) -> Result<Body> {
        self.send(Verb::Lease, paths::RUNNER_LEASES.to_owned(), None)
            .await
    }

    /// Renews a held lease. The token counts ride later work; this meters the
    /// run fee, which the daemon owes either way.
    ///
    /// # Errors
    /// Any classified failure of the call.
    pub async fn renew(&self, lease_id: &Uuid7) -> Result<()> {
        let path = lease_path(lease_id, paths::LEASE_RENEW_SUFFIX);
        self.post(Verb::Renew, path, &RenewRequest::default())
            .await
            .map(drop)
    }

    /// Posts one batch of live-tail frames.
    ///
    /// # Errors
    /// Any classified failure of the call.
    pub async fn activity(&self, lease_id: &Uuid7, request: &ActivityRequest<'_>) -> Result<()> {
        let path = lease_path(lease_id, paths::LEASE_ACTIVITY_SUFFIX);
        self.post(Verb::Activity, path, request).await.map(drop)
    }

    /// Posts a spooled report, byte for byte as it was spooled.
    ///
    /// # Errors
    /// Any classified failure of the call.
    pub async fn report(&self, report: Bytes) -> Result<()> {
        self.send(Verb::Report, paths::RUNNER_REPORTS.to_owned(), Some(report))
            .await
            .map(drop)
    }

    /// Reads a fleet's memory.
    ///
    /// # Errors
    /// Any classified failure of the call.
    pub async fn hydrate(&self, fleet_id: &Uuid7) -> Result<Body> {
        self.send(Verb::Hydrate, memory_path(fleet_id), None).await
    }

    /// Writes a fleet's memory back, fenced by the lease's token.
    ///
    /// # Errors
    /// Any classified failure of the call.
    pub async fn capture(&self, fleet_id: &Uuid7, request: &MemoryPushRequest<'_>) -> Result<()> {
        self.post(Verb::Capture, memory_path(fleet_id), request)
            .await
            .map(drop)
    }

    /// Downloads a bundle's canonical tar.
    ///
    /// # Errors
    /// Any classified failure of the call.
    pub async fn bundle(&self, content_hash: &str) -> Result<Bytes> {
        let path = format!("{}/{content_hash}", paths::RUNNER_BUNDLES);
        self.api
            .send(Call {
                verb: Verb::Bundle,
                path,
                body: None,
            })
            .await
    }

    /// Mints a scoped credential for a held lease.
    ///
    /// # Errors
    /// Any classified failure of the call.
    pub async fn mint(&self, request: &MintCredentialRequest<'_>) -> Result<Body> {
        self.post(
            Verb::Mint,
            paths::RUNNER_CREDENTIALS_MINT.to_owned(),
            request,
        )
        .await
    }

    async fn post<T: Serialize + Sync>(&self, verb: Verb, path: String, body: &T) -> Result<Body> {
        let bytes = serde_json::to_vec(body).map_err(error::encode)?;
        self.send(verb, path, Some(Bytes::from(bytes))).await
    }

    async fn send(&self, verb: Verb, path: String, body: Option<Bytes>) -> Result<Body> {
        let bytes = self.api.send(Call { verb, path, body }).await?;
        Ok(Body { verb, bytes })
    }
}

/// A path under one held lease.
fn lease_path(lease_id: &Uuid7, suffix: &str) -> String {
    format!("{}/{lease_id}/{suffix}", paths::RUNNER_LEASES)
}

/// One fleet's memory.
fn memory_path(fleet_id: &Uuid7) -> String {
    format!("{}/{fleet_id}", paths::RUNNER_MEMORY)
}

/// Runs `call` until it succeeds, fails terminally, or exhausts its attempts.
///
/// Only a retryable failure is retried: a 4xx is final on its first answer.
pub(crate) async fn retrying<T, F, Fut>(call: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let backoff = ExponentialBuilder::default()
        .with_min_delay(RETRY_FIRST_DELAY)
        .with_max_times(RETRY_ATTEMPTS);
    call.retry(backoff).when(crate::Error::is_retryable).await
}

#[cfg(test)]
#[path = "client/tests.rs"]
mod tests;
