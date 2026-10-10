//! What a stored handle claims, read the way the broker will read it.
//!
//! The broker resolves its connector from the handle's own `integration` field
//! and, for a GitHub App, mints for whichever installation the handle names.
//! Both are bytes the workspace wrote, and the generic secrets route lets it
//! write any of them under any name. So before the exchange, the mint holds the
//! handle to two facts the workspace did not author:
//!
//! - **The name it was asked for.** The grant gate ran against the requested
//!   integration. An on-demand handle filed under some other name would be
//!   minted as a connector no grant was checked for.
//! - **The installation's owner.** A GitHub installation id is a short integer,
//!   not a secret. The App's key mints for any installation it is installed on,
//!   so without the routing row the connect flow wrote, one workspace's handle
//!   could name another's installation and be handed its token.

use afd_credential::credential::github;
use afd_credential::secrets::connector::{
    Connectors as _, Exchange, FIELD_INTEGRATION, Registry, Supply,
};
use serde_json::Value;

use crate::error::{Result, github_reconnect_required, integration_not_connected};

/// What the mint must still prove before it may exchange this handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Claim<'a> {
    /// Nothing a datastore can hold to an owner: an inline credential the
    /// workspace supplied itself, a refresh handle whose exchange spends the
    /// workspace's own refresh token, or an integration no connector carries.
    Unowned,
    /// An App installation, which the workspace must have connected itself.
    Installation {
        /// The connector's id, as `core.connector_installs.provider` holds it.
        provider: &'a str,
        /// The installation, as the exchange would mint for it.
        id: u64,
    },
}

/// Reads what `handle`, opened under `requested`, would have the broker do.
///
/// # Errors
/// Refuses an on-demand handle filed under a name other than its connector's,
/// as not connected: the requested integration holds no connection of its own.
/// Refuses a GitHub handle that names no readable installation as a reconnect,
/// the same answer the exchange would give it.
pub(super) fn claim<'a>(
    connectors: &'a Registry,
    requested: &str,
    handle: &Value,
) -> Result<Claim<'a>> {
    let Some(connector) = handle
        .get(FIELD_INTEGRATION)
        .and_then(Value::as_str)
        .and_then(|named| connectors.resolve(named))
        .filter(|connector| connector.supply() == Supply::OnDemand)
    else {
        return Ok(Claim::Unowned);
    };
    if connector.name() != requested {
        return Err(integration_not_connected());
    }
    match connector.exchange() {
        Exchange::GithubApp => github::installation(handle)
            .map(|id| Claim::Installation {
                provider: connector.name(),
                id,
            })
            .ok_or_else(github_reconnect_required),
        Exchange::Stored | Exchange::OAuthRefresh { .. } => Ok(Claim::Unowned),
    }
}

#[cfg(test)]
#[path = "claim_tests.rs"]
mod tests;
