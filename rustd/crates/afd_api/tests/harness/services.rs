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

use afd_admin::{Models as AdminModels, PlatformKeys};
use afd_api::{Planes, SchedulePlane, Services};
use afd_approval::{Inbox, IntegrationGrants};
use afd_auth::mock::{MockCapabilities, MockVerifier};
use afd_billing::tenant::Billing;
use afd_connector::Connectors;
use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_credential::provider::Providers;
use afd_events::{History, Steer};
use afd_fleet::bundle::Bundles;
use afd_fleet::memory::Memories;
use afd_fleet_lifecycle::Fleets;
use afd_fleet_ops::RunnerLeaseHistory;
use afd_library::{Libraries, LibraryImports};
use afd_observability::Analytics;
use afd_runner::Runners;
use afd_sse::Live;
use afd_tenant::apikey::ApiKeys;
use afd_tenant::cli_credential::CliCredentials;
use afd_tenant::models::Models;
use afd_tenant::preference::Preferences;
use afd_tenant::session::Sessions as Logins;
use afd_tenant::workspace::Workspaces;
use afd_vault::Vault as SecretVault;

use super::stubs_runner::NoWork;
use super::stubs_tenant::OneWorkspace;
use super::{DEPLOYMENT, Directory, FIXTURE_APP_URL, Fleet, HarnessIngress, SCHEDULE_DESTINATION};

impl Services for Fleet {
    type Auth = Planes<Directory, MockCapabilities, MockVerifier>;
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
    type Ingress = HarnessIngress;
    type Schedules = SchedulePlane;
    type Connectors = Connectors;
    type Steering = Steer;
    type Memories = Memories;
    type Billing = Billing;
    type Catalogue = Models;
    type TenantProviders = Providers;

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

    fn live(&self) -> &Live {
        &self.live
    }

    fn analytics(&self) -> &Analytics {
        &self.analytics
    }

    fn ingress(&self) -> &HarnessIngress {
        &self.ingress
    }

    fn schedules(&self) -> &SchedulePlane {
        &self.schedules
    }

    fn connectors(&self) -> &Connectors {
        &self.connectors
    }

    /// The same fixed base the device-flow surface signs its login links with.
    ///
    /// A real URL rather than a placeholder, because it is half of what a
    /// connect proves: the `redirect_uri` a code is minted against is built
    /// from this, and a base that is not a URL would make every connect refuse
    /// for a reason no test was about.
    fn dashboard(&self) -> &str {
        &self.dashboard_base
    }

    /// No signing keys, which is the fail-closed default.
    ///
    /// A suite proving that a fire is refused when this deployment cannot
    /// verify one needs exactly this state, and it is the state most
    /// deployments are in — the schedules surface is opt-in.
    fn schedule_signing_keys(&self) -> Option<&afd_cron::SigningKeys> {
        self.schedule_keys.as_ref()
    }

    fn schedule_destination(&self) -> &str {
        SCHEDULE_DESTINATION
    }

    /// The deployment's own admin workspace, when a suite configured one.
    ///
    /// Answered from a field rather than as a constant `None`, unlike
    /// [`Services::deployment`] beside it, and the difference is that both
    /// answers are reachable states of a real daemon: a deployment with no
    /// admin workspace refuses every App delivery as unconfigured, and one with
    /// it serves them. A suite that could not say which it was could prove only
    /// half of that.
    type Signups = afd_tenant::signup::Signups;

    fn signups(&self) -> &Self::Signups {
        &self.signups
    }

    fn identity_webhook_secret(&self) -> Option<&afd_crypto::secret::SecretBytes> {
        self.identity_webhook_secret.as_ref()
    }

    fn platform_admin_workspace(&self) -> Option<&Uuid7> {
        self.platform_admin.as_ref()
    }

    fn steering(&self) -> &Steer {
        &self.steering
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

    fn tenant_providers(&self) -> &Providers {
        &self.providers
    }

    fn catalogue(&self) -> &Models {
        &self.catalogue
    }

    fn runner_lease_history(&self) -> &RunnerLeaseHistory {
        &self.runner_lease_history
    }

    fn models(&self) -> &AdminModels {
        &self.admin_models
    }

    fn platform_keys(&self) -> &PlatformKeys {
        &self.platform_keys
    }

    fn libraries(&self) -> &Libraries {
        &self.libraries
    }

    fn library_imports(&self) -> &LibraryImports {
        &self.library_imports
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
