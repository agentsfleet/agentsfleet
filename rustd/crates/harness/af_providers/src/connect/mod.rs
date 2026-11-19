//! Runtime routes come from the selected model's persisted lease settings.

use std::fmt;

use af_wire::lease::LeasePayload;
use af_wire::policy::ExecutionPolicy;

use crate::error::{Result, raise};
use crate::provider::Provider;
use crate::transport::Transport;
use crate::turn::Turns;
use af_inference::Configured;

/// What reaches the model a lease names.
///
/// One lease's life against it, in order:
///
/// ```text
/// LEASE ::= admit [connect TURN*]
/// TURN  ::= Model::stream (Chunk)* [failure]
/// ```
///
/// `admit` runs before anything is prepared for the lease, so a refusal costs
/// no sandbox and no bundle. `connect` runs once per lease.
pub trait Connect: Send + Sync + fmt::Debug {
    /// Refuses missing or invalid saved inference settings.
    ///
    /// # Errors
    /// The selected model's saved inference settings are missing or invalid.
    fn admit(&self, policy: &ExecutionPolicy<'_>) -> Result<()>;

    /// The provider `lease` drives.
    ///
    /// # Errors
    /// The selected model's saved inference settings are missing or invalid.
    fn connect(&self, lease: &LeasePayload<'_>) -> Result<Box<Provider>>;
}

/// The production [`Connect`]: one HTTP client every lease's provider shares.
///
/// A cheap handle: cloning it shares the client's connection pool. The client
/// follows no redirect, so a turn reaches the host its route names and no
/// other, with the key on it.
#[derive(Debug, Clone)]
pub struct Connector {
    client: reqwest::Client,
    timeouts: af_inference::Timeouts,
    retries: crate::RetryPolicy,
}

impl Connector {
    /// A connector using the runner's guarded, pooled HTTP client.
    ///
    /// # Errors
    /// The HTTP client could not be built.
    pub fn new() -> Result<Self> {
        Self::configured(af_inference::Timeouts::DEFAULT, crate::RetryPolicy::DEFAULT)
    }

    /// Builds one guarded pool with checked timeout and retry settings.
    ///
    /// # Errors
    /// The HTTP client could not be built.
    pub fn configured(
        timeouts: af_inference::Timeouts,
        retries: crate::RetryPolicy,
    ) -> Result<Self> {
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
            timeouts: af_inference::Timeouts::DEFAULT,
            retries: crate::RetryPolicy::DEFAULT,
        }
    }
}

fn configured<'a>(policy: &'a ExecutionPolicy<'_>) -> Result<Configured<'a>> {
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

    fn connect(&self, lease: &LeasePayload<'_>) -> Result<Box<Provider>> {
        let policy = &lease.policy;
        let route = configured(policy)?;
        let settings = policy
            .inference
            .as_ref()
            .ok_or_else(|| raise::unhosted(&policy.provider))?;
        let lease_id = lease.lease_id.as_ref();
        let transport = Transport::new(self.client.clone(), lease_id, &policy.provider)
            .with_retry(self.retries);
        let counting_transport = transport.clone().once().http();
        let model = route.model(&policy.context.model, &policy.api_key, transport.http());
        Ok(Box::new(
            Turns::new(model, settings, policy, lease_id, counting_transport)
                .with_settings(self.timeouts, self.retries),
        ))
    }
}

#[cfg(test)]
mod tests;
