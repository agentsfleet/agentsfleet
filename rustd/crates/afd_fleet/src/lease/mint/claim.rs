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
//! - **Where a refresh posts.** A refresh grant carries this deployment's OAuth
//!   client secret, and the handle can name the accounts server it is posted
//!   to. The exchange holds that to Zoho's accounts servers; this holds it to
//!   Zoho, so no other provider's secret is posted there either.
//!
//! The provider vocabulary comes from [`Provider`], which owns the routing
//! table's `provider` column; the connector registry's names are held to it by
//! a test rather than trusted to match.

use afd_connector::Provider;
use afd_credential::credential::{github, oauth};
use afd_credential::secrets::connector::{Connectors as _, Exchange, FIELD_INTEGRATION, Registry};
use serde_json::Value;

use crate::error::{
    Result, connector_reconnect_required, github_reconnect_required, integration_not_connected,
};

/// What the mint must still prove before it may exchange this handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Claim {
    /// Nothing a datastore can hold to an owner: an inline credential the
    /// workspace supplied itself, a refresh handle posting where its connector
    /// does, or an integration no connector carries.
    Unowned,
    /// An App installation, which the workspace must have connected itself.
    Installation {
        /// The provider, as `core.connector_installs.provider` holds it.
        provider: Provider,
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
/// the same answer the exchange would give it, and a refresh handle steering
/// its post anywhere the connect flow does not write as a reconnect too.
pub(super) fn claim(connectors: Registry, requested: &str, handle: &Value) -> Result<Claim> {
    let Some(connector) = handle
        .get(FIELD_INTEGRATION)
        .and_then(Value::as_str)
        .and_then(|named| connectors.resolve(named))
    else {
        return Ok(Claim::Unowned);
    };
    match connector.exchange() {
        Exchange::Stored => Ok(Claim::Unowned),
        Exchange::OAuthRefresh { .. } => {
            filed_as(connector.name(), requested).and_then(|provider| refresh(provider, handle))
        }
        Exchange::GithubApp => filed_as(connector.name(), requested)?
            .zip(github::installation(handle))
            .map(|(provider, id)| Claim::Installation { provider, id })
            .ok_or_else(github_reconnect_required),
    }
}

/// The provider an on-demand handle was filed as, refusing one stored under a
/// name other than its connector's.
///
/// The grant gate ran against `requested`, so a handle answering to any other
/// connector would mint as one no grant was checked for.
fn filed_as(connector: &str, requested: &str) -> Result<Option<Provider>> {
    (connector == requested)
        .then(|| Provider::parse(connector))
        .ok_or_else(integration_not_connected)
}

/// Whether a refresh handle posts only where its connector's connect flow
/// could have pointed it.
///
/// The exchange itself refuses a base outside Zoho's accounts servers, so this
/// adds the provider's half: a named base is Zoho's shape and nobody else's,
/// and is refused here before the vault's handle reaches the broker.
fn refresh(provider: Option<Provider>, handle: &Value) -> Result<Claim> {
    match oauth::steer(handle) {
        oauth::Steer::Declared => Ok(Claim::Unowned),
        oauth::Steer::To(base)
            if provider == Some(Provider::Zoho) && oauth::is_zoho_accounts_server(base) =>
        {
            Ok(Claim::Unowned)
        }
        oauth::Steer::To(_) => Err(connector_reconnect_required()),
    }
}

#[cfg(test)]
#[path = "claim_tests.rs"]
mod tests;
