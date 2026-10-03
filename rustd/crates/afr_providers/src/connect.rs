//! Which wire a lease's policy names, and the provider that speaks it.
//!
//! The [`Registry`] answers which wire and where: a named provider dials the
//! base URL its entry names, and a `custom:<url>` provider dials that URL. Any
//! other name has nothing this runner can dial, so the lease is refused at
//! admission rather than run against a guess.

use std::fmt;
use std::time::Duration;

use afd_wire::lease::LeasePayload;
use afd_wire::policy::ExecutionPolicy;
use reqwest::redirect;

use crate::anthropic::Messages;
use crate::error::{Result, raise};
use crate::http::{ApiKey, Http};
use crate::openai_chat::Chat;
use crate::openai_responses::Responses;
use crate::provider::Provider;
use crate::registry::{Registry, Wire};

/// How long a connection may take to open.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a turn's stream may sit silent: a reasoning model can think for
/// minutes before its first event, so this is long, and the lease's own stop
/// still ends a turn sooner.
const READ_TIMEOUT: Duration = Duration::from_secs(300);

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
    /// Refuses a policy naming a provider this runner does not speak.
    ///
    /// # Errors
    /// The provider is one this runner has no wire for.
    fn admit(&self, policy: &ExecutionPolicy<'_>) -> Result<()>;

    /// The provider `lease` drives.
    ///
    /// # Errors
    /// The provider is one this runner has no wire for.
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
    registry: Registry,
}

impl Connector {
    /// A connector dialling the providers `registry` names.
    ///
    /// # Errors
    /// The HTTP client could not be built.
    pub fn new(registry: Registry) -> Result<Self> {
        let client = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .read_timeout(READ_TIMEOUT)
            .redirect(redirect::Policy::none())
            .build()
            .map_err(raise::client)?;
        Ok(Self { client, registry })
    }
}

impl Connect for Connector {
    fn admit(&self, policy: &ExecutionPolicy<'_>) -> Result<()> {
        self.registry.route(&policy.provider).map(drop)
    }

    fn connect(&self, lease: &LeasePayload<'_>) -> Result<Box<dyn Provider>> {
        let policy = &lease.policy;
        let route = self.registry.route(&policy.provider)?;
        let client = self.client.clone();
        let base = route.base.as_str();
        let key = ApiKey::new(&policy.api_key);
        let lease_id = lease.lease_id.as_ref();
        Ok(match route.wire {
            Wire::Messages => Box::new(Http::new(client, base, key, lease_id, Messages)),
            Wire::Responses => Box::new(Http::new(client, base, key, lease_id, Responses)),
            Wire::Chat => Box::new(Http::new(client, base, key, lease_id, Chat)),
        })
    }
}

#[cfg(test)]
#[path = "connect/tests.rs"]
mod tests;
