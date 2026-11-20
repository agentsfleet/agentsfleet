//! The runner verbs: one transport seam, and the typed calls built on it.
//!
//! [`RunnerApi`] is one method — send this verb to this path with this body —
//! so a fake answers every verb from one closure and production implements it
//! once over HTTP ([`HttpRunnerApi`]). [`ControlPlane`] is the typed face the
//! duties call: it builds each path from `af_wire::paths`, serializes the
//! body, and hands back the reply as a [`Body`] that decodes on demand into a
//! wire type borrowing from it, so a run is parsed without copying a field.

use std::borrow::Cow;
use std::fmt;

use af_core::id::Uuid7;
use af_wire::credentials::MintCredentialRequest;
use af_wire::memory::{MemoryPushRequest, MemoryRecallRequest};
use af_wire::paths;
#[cfg(test)]
use backon::ExponentialBuilder;
use backon::Retryable as _;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::error::{self, Result};

mod context;
mod http;
mod run_verbs;
pub(crate) mod stream;
mod task;
mod turn_checkpoint;
mod verb;

pub(crate) use self::http::HttpRunnerApi;
#[cfg(test)]
pub(crate) use self::verb::Method;
pub(crate) use self::verb::Verb;

const EVENT_STARTED: &str = "daemon_call_started";
const EVENT_COMPLETED: &str = "daemon_call_completed";
const EVENT_FAILED: &str = "daemon_call_failed";

/// One request: the verb, the path it goes to, and its body if it has one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Call {
    /// What the request does.
    pub(crate) verb: Verb,
    /// The path under the daemon's base address. A fixed route borrows its
    /// constant; only a path naming a run or a fleet is built.
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

/// A successful reply, decoded on demand, and zeroed when dropped.
///
/// A run reply holds the run's policy, the fleet's memory window and its
/// earlier turns in clear, and the run borrows all three from these bytes for
/// as long as the run lasts; zeroing them on drop means the copy the runner
/// owns does not outlive the run in freed memory. The transport's own
/// buffer, handed over here, is the client's and is not covered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Body {
    verb: Verb,
    bytes: Zeroizing<Vec<u8>>,
}

impl Body {
    /// The reply as text, for a caller that hands it on whole — a tool whose
    /// model reads `agentsfleetd`'s answer as it came. A byte that is not
    /// UTF-8 reads as U+FFFD rather than failing the call.
    pub(crate) fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }

    /// Decodes the reply into `T`, borrowing every text field from it, through
    /// the gate that refuses a JSON array read into a struct's fields in order.
    pub(crate) fn decode<'a, T: Deserialize<'a>>(&'a self) -> Result<T> {
        af_core::json::object_from_slice(&self.bytes).map_err(error::malformed(self.verb))
    }
}

/// The runner verbs, typed: paths, bodies and replies.
///
/// Public so the calls built on it — [`crate::credentials::mint`] — can name
/// it; only this crate constructs one.
#[derive(Debug)]
pub struct ControlPlane {
    api: Box<dyn RunnerApi>,
    pub(crate) stream: crate::uplink::StreamHandle,
    pub(crate) runtime: crate::config::RuntimePolicy,
}

impl ControlPlane {
    /// Speaks every verb through `api`.
    pub(crate) fn new(api: Box<dyn RunnerApi>, stream: crate::uplink::StreamHandle) -> Self {
        Self {
            api,
            stream,
            runtime: crate::config::RuntimePolicy::DEFAULT,
        }
    }

    pub(crate) const fn with_runtime(mut self, runtime: crate::config::RuntimePolicy) -> Self {
        self.runtime = runtime;
        self
    }

    /// Runs a bounded operation using this runner's checked retry settings.
    pub(crate) async fn retrying<T, F, Fut>(&self, call: F) -> Result<T>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T>>,
    {
        call.retry(
            self.runtime
                .backoff()
                .with_max_times(self.runtime.attempts() - 1),
        )
        .when(crate::Error::is_retryable)
        .await
    }

    /// Posts one body of finished calls' full records, already encoded.
    pub(crate) async fn tool_calls(&self, run_id: &Uuid7, body: Bytes) -> Result<()> {
        let path = run_path(run_id, paths::RUN_ITEMS_SUFFIX);
        self.send(Verb::Items, path, Some(body)).await.map(drop)
    }

    /// Reads a fleet's memory.
    pub(crate) async fn load(&self, fleet_id: &Uuid7) -> Result<Body> {
        self.send(Verb::Load, memory_path(fleet_id), None).await
    }

    /// Writes a fleet's memory back, fenced by the run's token.
    pub(crate) async fn capture(
        &self,
        fleet_id: &Uuid7,
        request: &MemoryPushRequest<'_>,
    ) -> Result<()> {
        self.send_json(Verb::Capture, memory_path(fleet_id), request)
            .await
            .map(drop)
    }

    /// Searches a fleet's memory past the window, fenced by the run's token.
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
        self.send_json(Verb::Recall, path, request).await
    }

    /// Mints a scoped credential for a held run.
    pub(crate) async fn mint(&self, request: &MintCredentialRequest<'_>) -> Result<Body> {
        let path = Cow::Borrowed(paths::RUNNER_CREDENTIALS_MINT);
        self.send_json(Verb::Mint, path, request).await
    }

    /// Reads this runner's own row.
    pub(crate) async fn me(&self) -> Result<Body> {
        self.send(Verb::Me, Cow::Borrowed(paths::RUNNER_SELF), None)
            .await
    }

    /// Sends `body` as JSON, with the method `verb` is sent with.
    async fn send_json<T: Serialize + Sync>(
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
        let verb_name = verb.as_str();
        let event = EVENT_STARTED;
        tracing::debug!(verb = verb_name, event);
        match self.api.send(Call { verb, path, body }).await {
            Ok(bytes) => {
                let event = EVENT_COMPLETED;
                tracing::debug!(verb = verb_name, event);
                // `Vec::from` takes the buffer without a copy when the reply
                // owns it alone, so no second clear copy is left to free.
                Ok(Body {
                    verb,
                    bytes: Zeroizing::new(Vec::from(bytes)),
                })
            }
            Err(failure) => {
                let code = failure.code().as_str();
                let event = EVENT_FAILED;
                tracing::debug!(error_code = code, verb = verb_name, event);
                Err(failure)
            }
        }
    }
}

/// A path under one held run.
fn run_path(run_id: &Uuid7, suffix: &str) -> Cow<'static, str> {
    Cow::Owned(format!("{}/{run_id}/{suffix}", paths::RUNNER_RUNS))
}

/// One fleet's memory.
fn memory_path(fleet_id: &Uuid7) -> Cow<'static, str> {
    Cow::Owned(format!("{}/{fleet_id}", paths::RUNNER_MEMORY))
}

/// Default retry settings for isolated transport tests.
#[cfg(test)]
pub(crate) fn backoff() -> ExponentialBuilder {
    crate::config::RuntimePolicy::DEFAULT.backoff()
}

#[cfg(test)]
mod tests;
