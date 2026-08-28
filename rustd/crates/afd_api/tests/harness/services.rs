//! Which store each of the router's seams resolves to, under the suite.
//!
//! The mirror of `agentsfleetd::plane::services`, and split from [`super`] for
//! the same reason: that file BUILDS the fixture — the unreachable pool, the
//! mock directory, the frozen clock — and this one ANSWERS for it, one
//! accessor per seam. The list grows every time the API grows a surface; the
//! construction beside it grows when a dependency changes.
//!
//! Every store here is the PRODUCTION one, pointed at nothing. That is what
//! makes the refusal matrix reachable: a verb refuses with the error its own
//! crate raises rather than one this file invented.

use afd_api::{Planes, Services};
use afd_approval::{Inbox, IntegrationGrants};
use afd_auth::mock::{MockCapabilities, MockDirectory};
use afd_auth::verifier::NoVerifier;
use afd_billing::tenant::Billing;
use afd_core::clock::UnixMillis;
use afd_events::History;
use afd_fleet::bundle::Bundles;
use afd_fleet::memory::Memories;
use afd_fleet_lifecycle::Fleets;
use afd_runner::Runners;
use afd_tenant::apikey::ApiKeys;
use afd_tenant::cli_credential::CliCredentials;
use afd_tenant::models::Models;
use afd_tenant::preference::Preferences;
use afd_tenant::session::Sessions as Logins;
use afd_tenant::workspace::Workspaces;
use afd_vault::Vault as SecretVault;

use super::stubs_runner::NoWork;
use super::stubs_tenant::OneWorkspace;
use super::{DEPLOYMENT, Fleet};

impl Services for Fleet {
    type Auth = Planes<MockDirectory, MockCapabilities, NoVerifier>;
    type Leases = NoWork;
    type Sessions = Logins;
    type Workspaces = OneWorkspace;
    type WorkspaceDirectory = Workspaces;
    type ApiKeys = ApiKeys;
    type CliCredentials = CliCredentials;
    type Fleets = Fleets;
    type Secrets = SecretVault;
    type Preferences = Preferences;
    type Approvals = Inbox;
    type Grants = IntegrationGrants;
    type Events = History;
    type Memories = Memories;
    type Billing = Billing;
    type Catalogue = Models;

    fn authenticator(&self) -> &Self::Auth {
        &self.authenticator
    }

    fn runners(&self) -> &Runners {
        &self.runners
    }

    fn leases(&self) -> &NoWork {
        &self.leases
    }

    fn bundles(&self) -> &Bundles {
        &self.bundles
    }

    fn sessions(&self) -> &Logins {
        &self.logins
    }

    fn workspaces(&self) -> &OneWorkspace {
        &self.workspaces
    }

    /// A different value from [`Services::workspaces`], unlike production.
    ///
    /// The split is the suites': the ownership seam has to answer HONESTLY for
    /// both halves of the refusal matrix to be reachable, so it stays
    /// [`OneWorkspace`], while the directory refuses like every other store.
    fn workspace_directory(&self) -> &Workspaces {
        &self.workspace_directory
    }

    fn api_keys(&self) -> &ApiKeys {
        &self.api_keys
    }

    fn cli_credentials(&self) -> &CliCredentials {
        &self.cli_credentials
    }

    fn preferences(&self) -> &Preferences {
        &self.preferences
    }

    fn approvals(&self) -> &Inbox {
        &self.approvals
    }

    fn grants(&self) -> &IntegrationGrants {
        &self.grants
    }

    fn events(&self) -> &History {
        &self.events
    }

    fn memories(&self) -> &Memories {
        &self.memories
    }

    fn secrets(&self) -> &SecretVault {
        &self.secrets
    }

    fn fleets(&self) -> &Fleets {
        &self.fleets
    }

    fn billing(&self) -> &Billing {
        &self.billing
    }

    fn catalogue(&self) -> &Models {
        &self.catalogue
    }

    /// A fixed deployment, which is what a real one is too.
    ///
    /// Read from configuration in the binary rather than from the request, so a
    /// constant here is the same KIND of value the daemon serves with — not a
    /// simplification a suite would have to remember is one.
    fn deployment(&self) -> &str {
        DEPLOYMENT
    }

    fn now(&self) -> UnixMillis {
        self.now
    }
}
