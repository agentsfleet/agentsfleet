//! Which concrete store each of the router's seams resolves to, in production.
//!
//! Split from [`super`] along the line the file already had in it: everything
//! there CONSTRUCTS the plane — the handles, the configuration, the wiring done
//! once at boot — and everything here ANSWERS for it, one accessor per seam.
//! The two grow on different schedules. Construction changes when a dependency
//! changes; this list changes every time the API grows a surface, which on this
//! milestone alone was four times.
//!
//! Nothing here decides anything. Every method returns a field, and the reason
//! that is worth its own file rather than worth collapsing is that the compiler
//! checks it: a seam added to [`Services`] with no store behind it fails HERE,
//! in the one file that is only ever a mapping, rather than somewhere a reader
//! has to disentangle it from the boot sequence.

use afd_admin::{Models as AdminModels, PlatformKeys};
use afd_api::{Services, TenantSurface};

use crate::identity::SignupWriteback;
use afd_approval::{Inbox, IntegrationGrants};
use afd_billing::tenant::Billing;
use afd_core::clock::UnixMillis;
use afd_credential::provider::Providers;
use afd_events::History;
use afd_fleet::bundle::Bundles;
use afd_fleet::lease::Plane;
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

use super::{Authenticator, ServingPlane};

impl Services for ServingPlane {
    type Auth = Authenticator;
    type SignupMetadata = SignupWriteback;
    type Leases = Plane;
    type Sessions = Logins;
    type Workspaces = Workspaces;
    type Fleets = Fleets;
    type Secrets = SecretVault;
    type Preferences = Preferences;
    type Approvals = Inbox;
    type Grants = IntegrationGrants;
    type Events = History;
    type Ingress = afd_ingress::Ingress;
    type Schedules = afd_api::SchedulePlane;
    type Connectors = afd_connector::Connectors;
    type Steering = afd_events::Steer;
    type Memories = Memories;

    fn authenticator(&self) -> &Self::Auth {
        &self.authenticator
    }

    fn signup_metadata(&self) -> &Self::SignupMetadata {
        &self.signup_writeback
    }

    fn runners(&self) -> &Runners {
        &self.runners
    }

    fn leases(&self) -> &Plane {
        &self.leases
    }

    fn bundles(&self) -> &Bundles {
        &self.bundles
    }

    fn sessions(&self) -> &Logins {
        &self.logins
    }

    fn ingress(&self) -> &afd_ingress::Ingress {
        &self.ingress
    }

    fn schedules(&self) -> &afd_api::SchedulePlane {
        &self.schedules
    }

    fn connectors(&self) -> &afd_connector::Connectors {
        &self.connectors
    }

    /// Where a PERSON goes, which is a different deployment fact from
    /// [`Services::deployment`] below and never the same string.
    fn dashboard(&self) -> &str {
        &self.app_url
    }

    fn schedule_signing_keys(&self) -> Option<&afd_cron::SigningKeys> {
        self.schedule_keys.as_ref()
    }

    fn schedule_destination(&self) -> &str {
        &self.schedule_destination
    }

    fn platform_admin_workspace(&self) -> Option<&afd_core::id::Uuid7> {
        self.platform_admin_workspace.as_ref()
    }

    fn identity_webhook_secret(&self) -> Option<&afd_crypto::secret::SecretBytes> {
        self.identity_webhook_secret.as_ref()
    }

    fn workspaces(&self) -> &Workspaces {
        &self.workspaces
    }

    /// The same value as [`Services::workspaces`], deliberately: production
    /// holds one directory that answers both seams, and the split exists for
    /// the suites — see the trait's own note.
    fn fleets(&self) -> &Fleets {
        &self.fleets
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

    fn steering(&self) -> &afd_events::Steer {
        &self.steering
    }

    /// The store the lease plane already holds. ONE `Memories` in this process:
    /// reading what a fleet learned and writing it are two verbs over one table.
    fn memories(&self) -> &Memories {
        &self.leases.memories
    }

    fn secrets(&self) -> &SecretVault {
        &self.secrets
    }

    fn runner_lease_history(&self) -> &RunnerLeaseHistory {
        &self.runner_lease_history
    }

    /// The admin plane's WRITE of the priced catalogue.
    ///
    /// A different store from [`Services::catalogue`], which is the tenant's
    /// read of the same rows — the split is what keeps a tenant route from
    /// reaching a mutation by holding the wrong accessor.
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

    fn deployment(&self) -> &str {
        &self.api_url
    }

    /// The wall clock, read once per verb by whichever handler asked.
    ///
    /// Not a `Clock` behind an `Arc`: `afd_core::clock` reserves injection for
    /// an owner that reads repeatedly and asks everything else to take the
    /// instant as a parameter, which is exactly what a handler does with this.
    /// A test drives its own instant by implementing `Services` itself, so the
    /// seam a fixed clock would provide already exists one level up.
    fn now(&self) -> UnixMillis {
        afd_core::clock::now()
    }
}

// The tenant-scoped half, which the seam declares as a supertrait. Every field
// read here is the same field the block above would have read; the split is the
// seam's, not this plane's.
impl TenantSurface for ServingPlane {
    type WorkspaceDirectory = Workspaces;
    type ApiKeys = ApiKeys;
    type CliCredentials = CliCredentials;
    type Billing = Billing;
    type Catalogue = Models;
    type TenantProviders = Providers;
    type Signups = afd_tenant::signup::Signups;

    fn signups(&self) -> &Self::Signups {
        &self.signups
    }

    fn workspace_directory(&self) -> &Workspaces {
        &self.workspaces
    }

    fn api_keys(&self) -> &ApiKeys {
        &self.api_keys
    }

    fn cli_credentials(&self) -> &CliCredentials {
        &self.cli_credentials
    }

    fn billing(&self) -> &Billing {
        &self.billing
    }

    fn tenant_providers(&self) -> &Providers {
        &self.providers
    }

    fn catalogue(&self) -> &Models {
        &self.models
    }
}
