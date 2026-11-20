//! Which concrete store each of the router's seams resolves to, in production.
//!
//! Split from [`super`] along the line the file already had in it: everything
//! there CONSTRUCTS the plane — the handles, the configuration, the wiring done
//! once at boot — and everything here ANSWERS for it, one block per plane and
//! one accessor per seam. The two grow on different schedules. Construction
//! changes when a dependency changes; this list changes every time the API
//! grows a surface.
//!
//! Nothing here decides anything. Every method returns a field, and the reason
//! that is worth its own file is that the compiler checks it: a seam added to a
//! plane trait with no store behind it fails HERE, in the one file that is only
//! ever a mapping, rather than somewhere a reader has to disentangle it from
//! the boot sequence. An accessor two planes share returns the same field in
//! each block.

use afd_api_tenant::services::TenantPlane;
use afd_approval::{Inbox, IntegrationGrants};
use afd_billing::tenant::Billing;
use afd_credential::provider::Providers;
use afd_fleet::run::RunService;
use afd_fleet_lifecycle::Fleets;
use afd_http::services::{SchedulePlane, Shared};
use afd_memory::Memories;
use afd_tenant::apikey::ApiKeys;
use afd_tenant::cli_credential::CliCredentials;
use afd_tenant::models::Models;
use afd_tenant::preference::Preferences;
use afd_tenant::session::Sessions as Logins;
use afd_tenant::workspace::Workspaces;
use afd_turns::view::History;
use afd_vault::Vault as SecretVault;

use crate::identity::SignupWriteback;

use super::{Authenticator, ServingPlane};

impl Shared for ServingPlane {
    type Auth = Authenticator;
    type Workspaces = Workspaces;

    fn authenticator(&self) -> &Self::Auth {
        &self.authenticator
    }

    fn workspaces(&self) -> &Self::Workspaces {
        &self.tenant.workspaces
    }

    /// The wall clock, read once per verb by whichever handler asked.
    ///
    /// Not a `Clock` behind an `Arc`: `af_core::clock` reserves injection for
    /// an owner that reads repeatedly and asks everything else to take the
    /// instant as a parameter, which is exactly what a handler does with this.
    /// A test drives its own instant by implementing [`Shared`] itself.
    fn now(&self) -> jiff::Timestamp {
        af_core::clock::now()
    }
}

impl TenantPlane for ServingPlane {
    type WorkspaceDirectory = Workspaces;
    type ApiKeys = ApiKeys;
    type Team = afd_tenant::team::Team;
    type CliCredentials = CliCredentials;
    type Billing = Billing;
    type Catalogue = Models;
    type TenantProviders = Providers;
    type InviteMail = afd_mail::InviteMailer;
    type Sessions = Logins;
    type Fleets = Fleets;
    type Secrets = SecretVault;
    type Approvals = Inbox;
    type Grants = IntegrationGrants;
    type Memories = Memories;
    type Schedules = SchedulePlane;
    type Ingress = afd_ingress::Ingress;
    type Connectors = afd_connector::Connectors;
    type Steering = afd_turns::view::Steer;
    type Events = History;
    type Preferences = Preferences;

    /// The same value as [`Shared::workspaces`], deliberately: production
    /// holds one directory that answers both seams, and the split exists for
    /// the suites.
    fn workspace_directory(&self) -> &Self::WorkspaceDirectory {
        &self.tenant.workspaces
    }

    fn api_keys(&self) -> &Self::ApiKeys {
        &self.tenant.api_keys
    }

    fn team(&self) -> &Self::Team {
        &self.tenant.team
    }

    fn cli_credentials(&self) -> &Self::CliCredentials {
        &self.tenant.cli_credentials
    }

    fn billing(&self) -> &Self::Billing {
        &self.tenant.billing
    }

    fn catalogue(&self) -> &Self::Catalogue {
        &self.tenant.models
    }

    fn tenant_providers(&self) -> &Self::TenantProviders {
        &self.tenant.providers
    }

    fn invite_mail(&self) -> &Self::InviteMail {
        &self.tenant.invite_mail
    }

    fn sessions(&self) -> &Self::Sessions {
        &self.login.logins
    }

    fn fleets(&self) -> &Self::Fleets {
        &self.tenant.fleets
    }

    fn secrets(&self) -> &Self::Secrets {
        &self.tenant.secrets
    }

    fn approvals(&self) -> &Self::Approvals {
        &self.activity.approvals
    }

    fn grants(&self) -> &Self::Grants {
        &self.activity.grants
    }

    /// The store the run plane already holds. ONE `Memories` in this process:
    /// reading what a fleet learned and writing it are two verbs over one table.
    fn memories(&self) -> &Self::Memories {
        &self.runs.memories
    }

    fn schedules(&self) -> &Self::Schedules {
        &self.outbound.schedules
    }

    fn ingress(&self) -> &Self::Ingress {
        &self.activity.ingress
    }

    fn connectors(&self) -> &Self::Connectors {
        &self.outbound.connectors
    }

    fn steering(&self) -> &Self::Steering {
        &self.activity.steering
    }

    fn events(&self) -> &Self::Events {
        &self.activity.events
    }

    fn preferences(&self) -> &Self::Preferences {
        &self.tenant.preferences
    }

    fn runner_commands(&self) -> &afd_bus::Handle {
        &self.runner_commands
    }

    fn tasks(&self) -> &dyn afd_fleet::task::TaskLifecycle {
        &self.tasks
    }

    fn authoring(&self) -> &afd_setup::Drafts {
        &self.authoring.drafts
    }

    fn authoring_setup(&self) -> &afd_setup::Setup {
        &self.authoring.setup
    }

    fn creations(&self) -> &afd_setup::Creations {
        &self.authoring.creations
    }

    fn platform_admin_workspace(&self) -> Option<&af_core::id::Uuid7> {
        self.platform_admin_workspace.as_ref()
    }

    /// Where a PERSON goes, which is a different deployment fact from
    /// [`TenantPlane::deployment`] and never the same string.
    fn dashboard(&self) -> &afd_connector::Dashboard {
        &self.login.app_url
    }

    fn analytics(&self) -> &af_observability::Analytics {
        &self.analytics
    }

    fn live(&self) -> &afd_sse::Live {
        &self.live
    }

    fn deployment(&self) -> &str {
        &self.login.api_url
    }
}

impl afd_api_runner::services::RunnerPlane for ServingPlane {
    type Runs = RunService;
    type RunnerStreams = afd_gateway::Gateway;
    type Fleets = Fleets;
    type Schedules = SchedulePlane;
    type Steering = afd_turns::view::Steer;
    type Events = History;

    fn runs(&self) -> &Self::Runs {
        &self.runs
    }

    fn runner_streams(&self) -> &Self::RunnerStreams {
        &self.runner_streams
    }

    fn fleets(&self) -> &Self::Fleets {
        &self.tenant.fleets
    }

    fn schedules(&self) -> &Self::Schedules {
        &self.outbound.schedules
    }

    fn steering(&self) -> &Self::Steering {
        &self.activity.steering
    }

    fn events(&self) -> &Self::Events {
        &self.activity.events
    }

    fn runners(&self) -> &afd_runner::Runners {
        &self.runners
    }

    fn tasks(&self) -> &dyn afd_fleet::task::TaskLifecycle {
        &self.tasks
    }

    fn interjector(&self) -> &afd_outbound::Interjector {
        &self.outbound.interjector
    }
}

impl afd_api_operator::OperatorPlane for ServingPlane {
    fn runners(&self) -> &afd_runner::Runners {
        &self.runners
    }

    fn runner_commands(&self) -> &afd_bus::Handle {
        &self.runner_commands
    }

    fn runner_run_history(&self) -> &afd_fleet_ops::RunnerRunHistory {
        &self.admin.runner_run_history
    }

    /// The admin plane's WRITE of the priced catalogue: a different store
    /// from [`TenantPlane::catalogue`], the tenant's read of the same rows.
    fn models(&self) -> &afd_admin::Models {
        &self.admin.models
    }

    fn platform_keys(&self) -> &afd_admin::PlatformKeys {
        &self.admin.platform_keys
    }
}

impl afd_api_ingress::services::IngressPlane for ServingPlane {
    type Ingress = afd_ingress::Ingress;
    type Approvals = Inbox;
    type Schedules = SchedulePlane;
    type Connectors = afd_connector::Connectors;
    type Signups = afd_tenant::signup::Signups;
    type SignupMetadata = SignupWriteback;

    fn ingress(&self) -> &Self::Ingress {
        &self.activity.ingress
    }

    fn approvals(&self) -> &Self::Approvals {
        &self.activity.approvals
    }

    fn schedules(&self) -> &Self::Schedules {
        &self.outbound.schedules
    }

    fn connectors(&self) -> &Self::Connectors {
        &self.outbound.connectors
    }

    fn signups(&self) -> &Self::Signups {
        &self.tenant.signups
    }

    fn signup_metadata(&self) -> &Self::SignupMetadata {
        &self.signup_writeback
    }

    fn scheduler(&self) -> &dyn afd_cron::Scheduler {
        self.outbound.scheduler.as_ref()
    }

    fn tasks(&self) -> &dyn afd_fleet::task::TaskLifecycle {
        &self.tasks
    }

    fn platform_admin_workspace(&self) -> Option<&af_core::id::Uuid7> {
        self.platform_admin_workspace.as_ref()
    }

    fn identity_webhook_secret(&self) -> Option<&af_crypto::secret::SecretBytes> {
        self.identity_webhook_secret.as_ref()
    }
}
