//! Runtime routes come from the selected model's persisted lease settings.

use std::fmt;

use afd_wire::lease::LeasePayload;
use afd_wire::policy::ExecutionPolicy;

use crate::error::{Result, raise};
use crate::provider::Provider;
use crate::transport::Transport;
use crate::turn::Turns;
use afd_inference::Configured;
use afd_inference::stream::{CONNECT_TIMEOUT, READ_TIMEOUT};

/// What reaches the model a lease names.
///
/// One lease's life against it, in order:
///
/// ```text
/// LEASE ::= admit [connect TURN*]
/// TURN  ::= Provider::stream (Chunk)* [failure]
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
    fn connect(&self, lease: &LeasePayload<'_>) -> Result<Box<dyn Provider>>;
}

/// The production [`Connect`]: one HTTP client every lease's provider shares.
///
/// A cheap handle: cloning it shares the client's connection pool. The client
/// follows no redirect, so a turn reaches the host its route names and no
/// other, with the key on it.
#[derive(Debug, Clone)]
pub struct Connector {
    client: reqwest::Client,
}

impl Connector {
    /// A connector using the runner's guarded, pooled HTTP client.
    ///
    /// # Errors
    /// The HTTP client could not be built.
    pub fn new() -> Result<Self> {
        #[expect(
            clippy::disallowed_methods,
            reason = "afr_egress::guarded wraps this builder to pin DNS to the sandbox allowlist; afd_identity::direct is a daemon crate the runner does not depend on, and swapping to it would drop the egress guard"
        )]
        let client = afr_egress::guarded(reqwest::Client::builder())
            .connect_timeout(CONNECT_TIMEOUT)
            .read_timeout(READ_TIMEOUT)
            .build()
            .map_err(raise::client)?;
        Ok(Self::with_client(client))
    }

    /// Uses an already configured HTTP client; production callers must preserve the egress guard.
    #[must_use]
    pub const fn with_client(client: reqwest::Client) -> Self {
        Self { client }
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

    fn connect(&self, lease: &LeasePayload<'_>) -> Result<Box<dyn Provider>> {
        let policy = &lease.policy;
        let route = configured(policy)?;
        let settings = policy
            .inference
            .as_ref()
            .ok_or_else(|| raise::unhosted(&policy.provider))?;
        let lease_id = lease.lease_id.as_ref();
        let transport = Transport::new(self.client.clone(), lease_id, &policy.provider);
        let counting_transport = transport.clone().once().http();
        let model = route.model(&policy.context.model, &policy.api_key, transport.http());
        Ok(Box::new(Turns::new(
            model,
            settings,
            policy,
            lease_id,
            counting_transport,
        )))
    }
}

#[cfg(test)]
mod tests;
