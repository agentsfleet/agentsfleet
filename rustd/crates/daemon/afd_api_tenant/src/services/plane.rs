//! The services a tenant handler is served through.

use af_core::id::Uuid7;
use af_observability::Analytics;
use afd_connector::Dashboard;
use afd_http::services::{
    FleetSchedules, FleetSteering, Shared, WebhookIngress, WorkspaceApprovals, WorkspaceConnectors,
    WorkspaceFleets, WorkspaceTurns,
};
use afd_sse::Live;

use super::{
    DeviceFlow, FleetGrants, FleetMemories, InviteMail, ModelCatalogue, TenantBilling, TenantKeys,
    TenantModelEntries, TenantProviders, TenantTeam, TenantWorkspaces, TerminalCredentials,
    WorkspacePreferences, WorkspaceSecrets,
};

/// The tenant plane: a person's account, workspaces, fleets, setup and the
/// connectors and streams over them.
///
/// Each seam whose store holds something a suite cannot build — a bus
/// connection, an outbound client, a process key — is an associated type, so a
/// suite proving the refusal matrix substitutes its own; a store over a pool
/// and an entropy source is concrete only when its seam is inside the type.
pub trait TenantPlane: Shared {
    /// The tenant's workspace directory — its list, and the create. A second
    /// seam over what production holds as one value with
    /// [`Shared::workspaces`]: the ownership stub owns exactly one workspace,
    /// while this one refuses like every other store.
    type WorkspaceDirectory: TenantWorkspaces;
    /// The workspace directory the tenant-plane verbs act through.
    fn workspace_directory(&self) -> &Self::WorkspaceDirectory;

    /// The tenant's own api-keys.
    type ApiKeys: TenantKeys;
    /// The tenant api-key store.
    fn api_keys(&self) -> &Self::ApiKeys;

    /// The people in an account: invites into it and the members who accepted.
    type Team: TenantTeam;
    /// The account's invite and member store.
    fn team(&self) -> &Self::Team;

    /// A person's own command-line credentials.
    type CliCredentials: TerminalCredentials;
    /// The command-line credential store.
    fn cli_credentials(&self) -> &Self::CliCredentials;

    /// The tenant's billing reads.
    type Billing: TenantBilling;
    /// The billing read surface.
    fn billing(&self) -> &Self::Billing;

    /// The priced model catalogue's read surface.
    type Catalogue: ModelCatalogue;
    /// The catalogue the `/v1/models` read acts through.
    fn catalogue(&self) -> &Self::Catalogue;

    /// The tenant's own provider selection, read and written. Bound by both
    /// surfaces the store answers: the registry's `active` flag is computed
    /// against the selection, so a handler holding one and not the other could
    /// not render a row.
    type TenantProviders: TenantProviders + TenantModelEntries;
    /// The store `/v1/tenants/me/provider` acts through.
    fn tenant_providers(&self) -> &Self::TenantProviders;

    /// What an invite's email is sent through: the production mailer reads
    /// the relay out of a vault and dials it, and a suite must reach neither.
    type InviteMail: InviteMail;
    /// The invite email sender.
    fn invite_mail(&self) -> &Self::InviteMail;

    /// The device-flow login surface: the concrete one holds its store, a
    /// pepper and an entropy source.
    type Sessions: DeviceFlow;
    /// The device-flow login surface.
    fn sessions(&self) -> &Self::Sessions;

    /// The workspace's fleets surface.
    type Fleets: WorkspaceFleets;
    /// The workspace's fleets: list, install, read, edit, purge.
    fn fleets(&self) -> &Self::Fleets;

    /// The workspace secret surface.
    type Secrets: WorkspaceSecrets;
    /// The workspace's secrets: store, list, replace, delete.
    fn secrets(&self) -> &Self::Secrets;

    /// The approval inbox.
    type Approvals: WorkspaceApprovals;
    /// The operator's queue of approval gates.
    fn approvals(&self) -> &Self::Approvals;

    /// The integration-grant routes' store.
    type Grants: FleetGrants;
    /// A fleet's standing permissions to reach a third party.
    fn grants(&self) -> &Self::Grants;

    /// The fleet memory routes' store: the same `Memories` the runner plane's
    /// capture writes with.
    type Memories: FleetMemories;
    /// A fleet's durable memory: the page, and the forget.
    fn memories(&self) -> &Self::Memories;

    /// What the schedules surface acts through: the concrete plane holds a bus
    /// connection and an outbound HTTP client.
    type Schedules: FleetSchedules;
    /// What a schedule is read, written and fired through.
    fn schedules(&self) -> &Self::Schedules;

    /// The signed-ingress store, asked here for the deployment's platform
    /// secrets a connect signs its state with.
    type Ingress: WebhookIngress;
    /// What a signed delivery is resolved, verified and appended through.
    fn ingress(&self) -> &Self::Ingress;

    /// What the connector routes act through.
    type Connectors: WorkspaceConnectors;
    /// What a connect is started, finished, read and forgotten through.
    fn connectors(&self) -> &Self::Connectors;

    /// What the steer verb acts through.
    type Steering: FleetSteering;
    /// A fleet's message ingress.
    fn steering(&self) -> &Self::Steering;

    /// What the event-history routes act through.
    type Events: WorkspaceTurns;
    /// The narrative log a workspace and its fleets wrote.
    fn events(&self) -> &Self::Events;

    /// What the preference and onboarding surfaces act through.
    type Preferences: WorkspacePreferences;
    /// One person's dashboard preferences, and the checklist over them.
    fn preferences(&self) -> &Self::Preferences;

    /// The process-wide publisher retained even when live subscriptions are
    /// unavailable: a fleet Archive's runner commands go through it.
    fn runner_commands(&self) -> &afd_bus::Handle;

    /// Authoritative task lifecycle over the same fleet run and turn stores.
    fn tasks(&self) -> &dyn afd_fleet::task::TaskLifecycle;

    /// Creator-private setup shared by browser and terminal conversation clients.
    fn authoring(&self) -> &afd_setup::Drafts;
    /// Human-reviewed native setup over the same private state and process key.
    fn authoring_setup(&self) -> &afd_setup::Setup;
    /// Retained native Save, checked activation and initial-run recovery.
    fn creations(&self) -> &afd_setup::Creations;

    /// The workspace whose vault holds this deployment's own platform secrets,
    /// or `None` for a deployment that configured none: every surface that
    /// needs one fails closed rather than guessing.
    fn platform_admin_workspace(&self) -> Option<&Uuid7>;

    /// Where a PERSON goes, as distinct from where this daemon answers: every
    /// connector redirect and an invite's accept link are built from it,
    /// parsed at boot so no request re-parses it.
    fn dashboard(&self) -> &Dashboard;

    /// Where this instance's product events go; it carries its own absence.
    fn analytics(&self) -> &Analytics;

    /// What both live-stream routes act through; `afd_sse::Live::detached`
    /// lets a suite build one with no broker behind it.
    fn live(&self) -> &Live;

    /// This deployment's own base URL, as a minted credential records it: a
    /// credential and the deployment that minted it are one fact, so a
    /// client-asserted `Host` header would let the two disagree.
    fn deployment(&self) -> &str;
}
