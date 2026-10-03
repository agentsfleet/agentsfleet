//! Which wire a lease's policy names, and the provider that speaks it.
//!
//! `ExecutionPolicy.provider` selects the wire: `anthropic` is Messages,
//! `openai` is Responses, and `custom:<url>` is `OpenAI`-compatible chat at that
//! URL. A named provider dials its built-in host, which is why the daemon puts
//! no `inference_host` on its lease; any other name has nothing this runner can
//! dial, so the lease is refused at admission rather than run against a guess.

use std::fmt;
use std::time::Duration;

use afd_wire::lease::LeasePayload;
use afd_wire::policy::{CUSTOM_PROVIDER_PREFIX, ExecutionPolicy};
use reqwest::Url;

use crate::anthropic::Messages;
use crate::error::{Result, raise};
use crate::http::{ApiKey, Http};
use crate::openai_chat::Chat;
use crate::openai_responses::Responses;
use crate::provider::Provider;

/// The provider name the Messages wire answers to.
const ANTHROPIC: &str = "anthropic";
/// The provider name the Responses wire answers to.
const OPENAI: &str = "openai";
/// Where Anthropic serves Messages.
const ANTHROPIC_BASE: &str = "https://api.anthropic.com";
/// Where `OpenAI` serves Responses.
const OPENAI_BASE: &str = "https://api.openai.com";
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

/// Where the named providers are served.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoints {
    /// The base URL Messages posts under.
    pub anthropic: String,
    /// The base URL Responses posts under.
    pub openai: String,
}

impl Default for Endpoints {
    /// The providers' public hosts.
    fn default() -> Self {
        Self {
            anthropic: ANTHROPIC_BASE.to_owned(),
            openai: OPENAI_BASE.to_owned(),
        }
    }
}

/// The production [`Connect`]: one HTTP client every lease's provider shares.
///
/// A cheap handle: cloning it shares the client's connection pool.
#[derive(Debug, Clone)]
pub struct Connector {
    client: reqwest::Client,
    endpoints: Endpoints,
}

impl Connector {
    /// A connector dialling the named providers at `endpoints`.
    ///
    /// # Errors
    /// The HTTP client could not be built.
    pub fn new(endpoints: Endpoints) -> Result<Self> {
        let client = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .read_timeout(READ_TIMEOUT)
            .build()
            .map_err(raise::client)?;
        Ok(Self { client, endpoints })
    }
}

impl Connect for Connector {
    fn admit(&self, policy: &ExecutionPolicy<'_>) -> Result<()> {
        Wire::of(&policy.provider).map(drop)
    }

    fn connect(&self, lease: &LeasePayload<'_>) -> Result<Box<dyn Provider>> {
        let policy = &lease.policy;
        let client = self.client.clone();
        let key = ApiKey::new(&policy.api_key);
        let lease_id = lease.lease_id.as_ref();
        Ok(match Wire::of(&policy.provider)? {
            Wire::Messages => {
                let base = &self.endpoints.anthropic;
                Box::new(Http::new(client, base, key, lease_id, Messages))
            }
            Wire::Responses => {
                let base = &self.endpoints.openai;
                Box::new(Http::new(client, base, key, lease_id, Responses))
            }
            Wire::Chat(base) => Box::new(Http::new(client, base.as_str(), key, lease_id, Chat)),
        })
    }
}

/// The wire a provider name selects.
#[derive(Debug, PartialEq, Eq)]
enum Wire {
    Messages,
    Responses,
    Chat(Url),
}

impl Wire {
    /// The wire `provider` names.
    fn of(provider: &str) -> Result<Self> {
        match provider {
            ANTHROPIC => Ok(Self::Messages),
            OPENAI => Ok(Self::Responses),
            _ => provider
                .strip_prefix(CUSTOM_PROVIDER_PREFIX)
                .and_then(|base| Url::parse(base).ok())
                .map(Self::Chat)
                .ok_or_else(|| raise::unhosted(provider)),
        }
    }
}

#[cfg(test)]
#[path = "connect/tests.rs"]
mod tests;
