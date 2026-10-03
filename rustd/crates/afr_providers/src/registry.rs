//! The named providers this runner dials: each name, a wire and a base URL.
//!
//! The table is data, `assets/providers.json`, embedded at build time the way
//! `IronClaw` embeds its `providers.json` (`ironclaw_llm/src/registry.rs`), so a
//! provider is a reviewed line of JSON rather than code (RULE CFG). It carries
//! every name the Zig runner's provider table maps to a wire this runner
//! speaks, at the base URL the Zig runner dials. A name it leaves out, such as
//! one needing request signing, a token exchange, a non-streaming wire or a
//! loopback host, is refused at admission rather than dialled wrong.
//!
//! A `custom:<url>` provider is no entry: its URL comes with the lease. It is
//! taken only as `https` with a host, and the transport follows no redirect,
//! so a turn reaches that host and no other.

use std::collections::HashMap;

use afd_wire::policy::CUSTOM_PROVIDER_PREFIX;
use reqwest::Url;
use serde::Deserialize;

use crate::error::{Result, raise};

/// The table this runner ships.
const BUILTIN: &str = include_str!("../assets/providers.json");
/// The only scheme a provider is dialled over.
const HTTPS: &str = "https";

/// The wire a provider speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Wire {
    /// Anthropic Messages.
    Messages,
    /// `OpenAI` Responses.
    Responses,
    /// `OpenAI`-compatible chat completions.
    Chat,
}

/// One named provider, as the table spells it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderSpec {
    /// The name a policy selects it by.
    pub name: String,
    /// Other names that select it.
    #[serde(default)]
    pub aliases: Vec<String>,
    /// The wire it speaks.
    pub wire: Wire,
    /// Where its turns post under.
    pub base_url: String,
}

/// Where one lease's provider is dialled, and over which wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Route {
    pub(crate) wire: Wire,
    pub(crate) base: Url,
}

/// Every named provider, by each name and alias.
#[derive(Debug, Clone)]
pub struct Registry {
    routes: HashMap<String, Route>,
}

impl Registry {
    /// The table this runner ships.
    ///
    /// # Errors
    /// The embedded table does not parse, or an entry's URL does not.
    pub fn builtin() -> Result<Self> {
        Self::new(serde_json::from_str::<Vec<ProviderSpec>>(BUILTIN)?)
    }

    /// A registry of `specs`.
    ///
    /// # Errors
    /// An entry's base URL does not parse.
    pub fn new(specs: impl IntoIterator<Item = ProviderSpec>) -> Result<Self> {
        let mut routes = HashMap::new();
        for spec in specs {
            let base =
                Url::parse(&spec.base_url).map_err(|source| raise::registry(&spec.name, source))?;
            let route = Route {
                wire: spec.wire,
                base,
            };
            for name in spec.aliases.into_iter().chain([spec.name]) {
                routes.insert(name, route.clone());
            }
        }
        Ok(Self { routes })
    }

    /// The route `provider` names: a registered name or alias, or an `https`
    /// `custom:<url>` with a host.
    ///
    /// # Errors
    /// The provider is neither.
    pub(crate) fn route(&self, provider: &str) -> Result<Route> {
        if let Some(route) = self.routes.get(provider) {
            return Ok(route.clone());
        }
        provider
            .strip_prefix(CUSTOM_PROVIDER_PREFIX)
            .and_then(|base| Url::parse(base).ok())
            .filter(|base| base.scheme() == HTTPS && base.host_str().is_some())
            .map(|base| Route {
                wire: Wire::Chat,
                base,
            })
            .ok_or_else(|| raise::unhosted(provider))
    }
}

#[cfg(test)]
#[path = "registry/tests.rs"]
mod tests;
