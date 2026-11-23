//! Runtime routes come from the selected model's persisted run settings.

use std::fmt;
use std::sync::Arc;

use af_wire::policy::ExecutionPolicy;
use af_wire::run::RunPayload;
use guffbolt_ai::wire::Connected;

use crate::error::{Result, raise};
use crate::provider::Provider;
use crate::settings::{Configured, RetryPolicy, Timeouts};
use crate::transport::{REPLY_MAX_BYTES, guarded};
use crate::turn::Turns;
use crate::turn::observer::RunObserver;

/// What reaches the model a run names.
///
/// One run's life against it, in order:
///
/// ```text
/// RUN ::= admit [connect TURN*]
/// TURN  ::= Model::stream (Chunk)* [failure]
/// ```
///
/// `admit` runs before anything is prepared for the run, so a refusal costs
/// no sandbox and no bundle. `connect` runs once per run.
pub trait Connect: Send + Sync + fmt::Debug {
    /// Refuses missing or invalid saved inference settings.
    ///
    /// # Errors
    /// The selected model's saved inference settings are missing or invalid.
    fn admit(&self, policy: &ExecutionPolicy<'_>) -> Result<()>;

    /// The provider `run` drives.
    ///
    /// # Errors
    /// The selected model's saved inference settings are missing or invalid.
    fn connect(&self, run: &RunPayload<'_>) -> Result<Box<Provider>>;
}

/// The production [`Connect`]: one HTTP client every run's provider shares.
///
/// A cheap handle: cloning it shares the client's connection pool. The client
/// follows no redirect, so a turn reaches the host its route names and no
/// other, with the key on it. A reply is bounded by the runner frame
/// (`af_wire::protocol::MESSAGE_BYTES_MAX`).
#[derive(Debug, Clone)]
pub struct Connector {
    client: reqwest::Client,
    timeouts: Timeouts,
    retries: RetryPolicy,
}

impl Connector {
    /// A connector using the runner's guarded, pooled HTTP client.
    ///
    /// # Errors
    /// The HTTP client could not be built.
    pub fn new() -> Result<Self> {
        Self::configured(Timeouts::DEFAULT, RetryPolicy::DEFAULT)
    }

    /// Builds one guarded pool with checked timeout and retry settings.
    ///
    /// # Errors
    /// The HTTP client could not be built.
    pub fn configured(timeouts: Timeouts, retries: RetryPolicy) -> Result<Self> {
        #[expect(
            clippy::disallowed_methods,
            reason = "af_egress::guarded wraps this builder to pin DNS to the sandbox allowlist; afd_identity::direct is a daemon crate the runner does not depend on, and swapping to it would drop the egress guard"
        )]
        let client = af_egress::guarded(reqwest::Client::builder())
            .connect_timeout(timeouts.connect())
            .read_timeout(timeouts.stream_idle())
            .build()
            .map_err(raise::client)?;
        Ok(Self {
            client,
            timeouts,
            retries,
        })
    }

    /// Uses an already configured HTTP client; production callers must preserve the egress guard.
    #[must_use]
    pub const fn with_client(client: reqwest::Client) -> Self {
        Self {
            client,
            timeouts: Timeouts::DEFAULT,
            retries: RetryPolicy::DEFAULT,
        }
    }
}

fn configured(policy: &ExecutionPolicy<'_>) -> Result<Configured> {
    let settings = policy
        .inference
        .as_ref()
        .ok_or_else(|| raise::unhosted(&policy.provider))?;
    Configured::new(settings, policy.model_context_tokens).map_err(raise::configuration)
}

impl Connect for Connector {
    fn admit(&self, policy: &ExecutionPolicy<'_>) -> Result<()> {
        configured(policy).map(drop)
    }

    fn connect(&self, run: &RunPayload<'_>) -> Result<Box<Provider>> {
        let policy = &run.policy;
        let route = configured(policy)?.into_route();
        let run_id = run.run_id.as_ref();
        let observer = RunObserver::new(run_id, &policy.provider);
        let connected = Connected::new(
            route,
            &policy.context.model,
            &policy.api_key,
            guarded(self.client.clone()),
        )
        .with_retry(self.retries.wire())
        .with_timeouts(self.timeouts.wire())
        .with_reply_bound(REPLY_MAX_BYTES)
        .with_observer(Arc::new(observer));
        Ok(Box::new(Turns::new(connected, run_id, &policy.provider)))
    }
}

#[cfg(test)]
mod tests;
