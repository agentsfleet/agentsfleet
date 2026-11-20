//! Which store each of the router's seams resolves to, under the suite.
//!
//! The mirror of `agentsfleetd::plane::services`, and split from [`super`] for
//! the same reason: that file BUILDS the fixture — the unreachable pool, the
//! mock directory, the frozen clock — and this one ANSWERS for it, one block
//! per plane and one accessor per seam.
//!
//! Every store here is the PRODUCTION one, pointed at nothing. That is what
//! makes the refusal matrix reachable: a verb refuses with the error its own
//! crate raises rather than one this file invented.

use afd_api_ingress::services::IngressPlane;
use afd_api_operator::OperatorPlane;
use afd_api_runner::services::RunnerPlane;
use afd_api_tenant::services::TenantPlane;
use afd_approval::{Inbox, IntegrationGrants};
use afd_auth::mock::{MockCapabilities, MockVerifier};
use afd_billing::tenant::Billing;
use afd_connector::Connectors;
use afd_fleet_lifecycle::Fleets;
use afd_http::auth::Planes;
use afd_http::services::{SchedulePlane, Shared};
use afd_memory::Memories;
use afd_tenant::apikey::ApiKeys;
use afd_tenant::cli_credential::CliCredentials;
use afd_tenant::models::Models;
use afd_tenant::preference::Preferences;
use afd_tenant::session::Sessions as Logins;
use afd_tenant::workspace::Workspaces;
use afd_turns::view::{History, Steer};
use afd_vault::Vault as SecretVault;

use super::HarnessProviders;
use super::stubs_runner::NoWork;
use super::stubs_tenant::Ownership;
use super::{DEPLOYMENT, Directory, Fleet, HarnessIngress, HarnessTeam, RecordingWriteback};

impl Shared for Fleet {
    type Auth = Planes<Directory, MockCapabilities, MockVerifier>;
    type Workspaces = Ownership;

    fn authenticator(&self) -> &Self::Auth {
        &self.authenticator
    }

    fn workspaces(&self) -> &Self::Workspaces {
        &self.workspaces
    }

    fn now(&self) -> jiff::Timestamp {
        self.now
    }
}

impl TenantPlane for Fleet {
    type WorkspaceDirectory = Workspaces;
    type ApiKeys = ApiKeys;
    type Team = HarnessTeam;
    type CliCredentials = CliCredentials;
    type Billing = Billing;
    type Catalogue = Models;
    type TenantProviders = HarnessProviders;
    type InviteMail = afd_mail::InviteMailer;
    type Sessions = Logins;
    type Fleets = Fleets;
    type Secrets = SecretVault;
    type Approvals = Inbox;
    type Grants = IntegrationGrants;
    type Memories = Memories;
    type Schedules = SchedulePlane;
    type Ingress = HarnessIngress;
    type Connectors = Connectors;
    type Steering = Steer;
    type Events = History;
    type Preferences = Preferences;

    /// A different value from [`Shared::workspaces`], unlike production.
    ///
    /// The split is the suites': the ownership seam has to answer HONESTLY for
    /// both halves of the refusal matrix to be reachable, so it stays
    /// `Ownership`, while the directory refuses like every other store.
    fn workspace_directory(&self) -> &Self::WorkspaceDirectory {
        &self.workspace_directory
    }

    fn api_keys(&self) -> &Self::ApiKeys {
        &self.api_keys
    }

    fn team(&self) -> &Self::Team {
        &self.team
    }

    fn cli_credentials(&self) -> &Self::CliCredentials {
        &self.cli_credentials
    }

    fn billing(&self) -> &Self::Billing {
        &self.billing
    }

    fn catalogue(&self) -> &Self::Catalogue {
        &self.catalogue
    }

    fn tenant_providers(&self) -> &Self::TenantProviders {
        &self.providers
    }

    fn invite_mail(&self) -> &Self::InviteMail {
        &self.invite_mail
    }

    fn sessions(&self) -> &Self::Sessions {
        &self.logins
    }

    fn fleets(&self) -> &Self::Fleets {
        &self.fleets
    }

    fn secrets(&self) -> &Self::Secrets {
        &self.secrets
    }

    fn approvals(&self) -> &Self::Approvals {
        &self.approvals
    }

    fn grants(&self) -> &Self::Grants {
        &self.grants
    }

    fn memories(&self) -> &Self::Memories {
        &self.memories
    }

    fn schedules(&self) -> &Self::Schedules {
        &self.schedules
    }

    fn ingress(&self) -> &Self::Ingress {
        &self.ingress
    }

    fn connectors(&self) -> &Self::Connectors {
        &self.connectors
    }

    fn steering(&self) -> &Self::Steering {
        &self.steering
    }

    fn events(&self) -> &Self::Events {
        &self.events
    }

    fn preferences(&self) -> &Self::Preferences {
        &self.preferences
    }

    fn runner_commands(&self) -> &afd_bus::Handle {
        &self.runner_commands
    }

    fn tasks(&self) -> &dyn afd_fleet::task::TaskLifecycle {
        &self.tasks
    }

    fn authoring(&self) -> &afd_setup::Drafts {
        &self.authoring
    }

    fn authoring_setup(&self) -> &afd_setup::Setup {
        &self.authoring_setup
    }

    fn creations(&self) -> &afd_setup::Creations {
        &self.creations
    }

    /// The deployment's own admin workspace, when a suite configured one.
    ///
    /// Answered from a field rather than as a constant `None`, because both
    /// answers are reachable states of a real daemon: a deployment with no
    /// admin workspace refuses every App delivery as unconfigured, and one with
    /// it serves them.
    fn platform_admin_workspace(&self) -> Option<&af_core::id::Uuid7> {
        self.platform_admin.as_ref()
    }

    /// The same fixed base the device-flow surface signs its login links with.
    ///
    /// A real URL rather than a placeholder, because it is half of what a
    /// connect proves: the `redirect_uri` a code is minted against is built
    /// from this, and a base that is not a URL would make every connect refuse
    /// for a reason no test was about.
    fn dashboard(&self) -> &afd_connector::Dashboard {
        &self.dashboard_base
    }

    fn analytics(&self) -> &af_observability::Analytics {
        &self.analytics
    }

    fn live(&self) -> &afd_sse::Live {
        &self.live
    }

    /// A fixed deployment, which is what a real one is too: read from
    /// configuration in the binary rather than from the request.
    fn deployment(&self) -> &str {
        DEPLOYMENT
    }
}

impl RunnerPlane for Fleet {
    type Runs = NoWork;
    type RunnerStreams = Self;
    type Fleets = Fleets;
    type Schedules = SchedulePlane;
    type Steering = Steer;
    type Events = History;

    fn runs(&self) -> &Self::Runs {
        &self.runs
    }

    fn runner_streams(&self) -> &Self::RunnerStreams {
        self
    }

    fn fleets(&self) -> &Self::Fleets {
        &self.fleets
    }

    fn schedules(&self) -> &Self::Schedules {
        &self.schedules
    }

    fn steering(&self) -> &Self::Steering {
        &self.steering
    }

    fn events(&self) -> &Self::Events {
        &self.events
    }

    fn runners(&self) -> &afd_runner::Runners {
        &self.runners
    }

    fn tasks(&self) -> &dyn afd_fleet::task::TaskLifecycle {
        &self.tasks
    }

    fn interjector(&self) -> &afd_outbound::Interjector {
        &self.interjector
    }
}

impl OperatorPlane for Fleet {
    fn runners(&self) -> &afd_runner::Runners {
        &self.runners
    }

    fn runner_commands(&self) -> &afd_bus::Handle {
        &self.runner_commands
    }

    fn runner_run_history(&self) -> &afd_fleet_ops::RunnerRunHistory {
        &self.runner_run_history
    }

    fn models(&self) -> &afd_admin::Models {
        &self.admin_models
    }

    fn platform_keys(&self) -> &afd_admin::PlatformKeys {
        &self.platform_keys
    }
}

impl IngressPlane for Fleet {
    type Ingress = HarnessIngress;
    type Approvals = Inbox;
    type Schedules = SchedulePlane;
    type Connectors = Connectors;
    type Signups = afd_tenant::signup::Signups;
    type SignupMetadata = RecordingWriteback;

    fn ingress(&self) -> &Self::Ingress {
        &self.ingress
    }

    fn approvals(&self) -> &Self::Approvals {
        &self.approvals
    }

    fn schedules(&self) -> &Self::Schedules {
        &self.schedules
    }

    fn connectors(&self) -> &Self::Connectors {
        &self.connectors
    }

    fn signups(&self) -> &Self::Signups {
        &self.signups
    }

    fn signup_metadata(&self) -> &Self::SignupMetadata {
        &self.signup_writeback
    }

    /// The fixture scheduler: blank signing keys unless a suite set them, which
    /// is the fail-closed default — see `fleet_credentials::fixed_scheduler`.
    fn scheduler(&self) -> &dyn afd_cron::Scheduler {
        self.scheduler.as_ref()
    }

    fn tasks(&self) -> &dyn afd_fleet::task::TaskLifecycle {
        &self.tasks
    }

    /// The deployment's own admin workspace, when a suite configured one.
    ///
    /// Answered from a field rather than as a constant `None`, because both
    /// answers are reachable states of a real daemon: a deployment with no
    /// admin workspace refuses every App delivery as unconfigured, and one with
    /// it serves them.
    fn platform_admin_workspace(&self) -> Option<&af_core::id::Uuid7> {
        self.platform_admin.as_ref()
    }

    fn identity_webhook_secret(&self) -> Option<&af_crypto::secret::SecretBytes> {
        self.identity_webhook_secret.as_ref()
    }
}
