//! A connection's life outside the round-trip that made it: what a workspace
//! has, what it COULD have, and letting one go.
//!
//! [`crate::connect`] and [`crate::complete`] own the four steps that produce a
//! grant. These three answer about grants that already exist, and they are on
//! the same value because they act through the same two handles a round-trip
//! does — the deployment's app bags and the workspace's own vault. A second
//! type over the same pair would be a second thing for a composition root to
//! build and a second seam for a suite to stub.
//!
//! # The catalogue is a join of two listings, not five reads
//!
//! `configured` is a fact about the DEPLOYMENT and `connected` a fact about the
//! workspace, so the two come from two different vaults and neither depends on
//! the other. Two listings answer both columns for every provider at once.
//! `catalog.zig` reaches the same two round trips by hand-building two
//! index-aligned key arrays with a `made` counter and a `defer` free loop; the
//! arrays exist there because the answer has to be carried alongside a registry
//! it cannot own, which is not a problem an owned collection has.

use std::collections::BTreeSet;

use afd_core::id::Uuid7;

use crate::connect::Connectors;
use crate::error::Result;
use crate::grant::{Connection, Forgotten};
use crate::provider::Provider;
use crate::registry::Archetype;

/// One row of what this workspace can connect to.
///
/// Carries the archetype because the dashboard renders a different affordance
/// for each — an OAuth consent hop and an App installation are not the same
/// button — and it is the registry's fact rather than the card's, so a sixth
/// connector cannot arrive with the wrong one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Catalogued {
    /// What this row is about.
    pub provider: Provider,
    /// Whether an operator has configured this deployment's app for it.
    ///
    /// Platform-wide: false here means nobody in this deployment can connect
    /// the provider, whatever their workspace holds.
    pub configured: bool,
    /// Whether THIS workspace holds a landed grant for it.
    pub connected: bool,
}

impl Catalogued {
    /// Whether this row's connect flow is an App installation.
    ///
    /// Answered from the registry rather than stored, so the two cannot drift:
    /// a provider that changed archetype changes this with it.
    #[must_use]
    pub const fn is_app_install(self) -> bool {
        matches!(self.provider.archetype(), Archetype::AppInstall(_))
    }
}

impl Connectors {
    /// This workspace's connection to `provider`, or nothing.
    ///
    /// # Errors
    /// Reports a datastore that would not answer and an envelope that would not
    /// open — see [`crate::grant::Grants::connection`].
    pub async fn connection(
        &self,
        workspace: &Uuid7,
        provider: Provider,
    ) -> Result<Option<Connection>> {
        self.grants.connection(workspace, provider).await
    }

    /// Every provider, with what this deployment and this workspace hold.
    ///
    /// In [`Provider::ALL`] order, which is the order a dashboard lists them
    /// in: the catalogue is the product's own list of connectors, so its
    /// ordering is a decision the registry already made rather than one this
    /// read gets to re-make.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn catalogue(
        &self,
        admin: Option<&Uuid7>,
        workspace: &Uuid7,
    ) -> Result<Vec<Catalogued>> {
        let configured = self.app.provisioned(admin).await?;
        let connected = self.grants.held(workspace).await?;
        Ok(rows(&configured, &connected))
    }

    /// Forgets this workspace's connection to `provider`.
    ///
    /// # Errors
    /// Reports a datastore that would not answer and a vault that refused the
    /// delete — see [`crate::grant::Grants::forget`].
    pub async fn forget(&self, workspace: &Uuid7, provider: Provider) -> Result<Forgotten> {
        self.grants.forget(workspace, provider).await
    }
}

/// The catalogue, joined from the two membership sets.
///
/// Lifted out of the read so the join is provable without a datastore: the
/// interesting part is that a provider in neither set still LISTS, and pinning
/// that through a vault would need two workspaces and a seeded row.
fn rows(configured: &BTreeSet<Provider>, connected: &BTreeSet<Provider>) -> Vec<Catalogued> {
    Provider::ALL
        .iter()
        .copied()
        .map(|provider| Catalogued {
            provider,
            configured: configured.contains(&provider),
            connected: connected.contains(&provider),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]

    use std::collections::BTreeSet;

    use super::rows;
    use crate::provider::Provider;

    /// Every shipped provider lists, however little is held.
    ///
    /// The load-bearing case: a dashboard renders its cards from this and from
    /// no hard-coded list, so a catalogue that omitted the unconfigured
    /// providers would leave an operator with no card to discover them from —
    /// and no way to learn that connecting one needs a deployment change.
    #[test]
    fn a_deployment_holding_nothing_still_lists_every_provider() {
        let none = BTreeSet::new();

        let catalogue = rows(&none, &none);

        assert_eq!(catalogue.len(), Provider::ALL.len());
        assert!(
            catalogue
                .iter()
                .all(|row| !row.configured && !row.connected),
        );
    }

    /// The two columns are read from their own sets and do not bleed.
    ///
    /// `configured` is the deployment's fact and `connected` the workspace's,
    /// so a workspace that connected Slack must not make Jira's app look
    /// configured — and a configured app must not read as connected, which is
    /// the pairing a person acts on when they press Connect.
    #[test]
    fn the_configured_and_connected_columns_come_from_their_own_sets() {
        let configured = BTreeSet::from([Provider::Slack, Provider::Jira]);
        let connected = BTreeSet::from([Provider::Slack]);

        let catalogue = rows(&configured, &connected);
        let row = |provider| {
            *catalogue
                .iter()
                .find(|row| row.provider == provider)
                .expect("every provider lists")
        };

        assert!(row(Provider::Slack).configured && row(Provider::Slack).connected);
        assert!(row(Provider::Jira).configured && !row(Provider::Jira).connected);
        assert!(!row(Provider::Linear).configured && !row(Provider::Linear).connected);
    }

    /// A connection whose app was later unconfigured still reads as connected.
    ///
    /// A real state rather than a contradiction: the grant is in the vault and
    /// the fleet can still spend it, and an operator who removed the app bag
    /// has broken RECONNECTING, not the existing connection. Collapsing the
    /// two columns into one would hide exactly that.
    #[test]
    fn a_grant_outlives_the_app_bag_it_was_minted_through() {
        let row = rows(&BTreeSet::new(), &BTreeSet::from([Provider::Zoho]))
            .into_iter()
            .find(|row| row.provider == Provider::Zoho)
            .expect("every provider lists");

        assert!(row.connected);
        assert!(!row.configured);
    }
}
