//! The services a signed-delivery handler is served through.

use af_core::id::Uuid7;
use af_crypto::secret::SecretBytes;
use afd_http::services::{
    FleetSchedules, Shared, WebhookIngress, WorkspaceApprovals, WorkspaceConnectors,
};

use super::{SignupMetadata, Signups};

/// The ingress plane: deliveries a sender signs, proven by a signature over the
/// body before any handler runs.
pub trait IngressPlane: Shared {
    /// What the signed-ingress routes act through: the concrete store holds
    /// the turn ledger and the bus it notifies through.
    type Ingress: WebhookIngress;

    /// What a signed delivery is resolved, verified and appended through.
    fn ingress(&self) -> &Self::Ingress;

    /// The approval inbox a signed approval decision lands in.
    type Approvals: WorkspaceApprovals;

    /// The queue of approval gates.
    fn approvals(&self) -> &Self::Approvals;

    /// What a scheduler's fire acts through.
    type Schedules: FleetSchedules;

    /// What a schedule is read, written and fired through.
    fn schedules(&self) -> &Self::Schedules;

    /// The external scheduler: it registers schedules and proves the fires it
    /// sends back. A fire it does not verify is refused.
    fn scheduler(&self) -> &dyn afd_cron::Scheduler;

    /// What a provider's connector event acts through.
    type Connectors: WorkspaceConnectors;

    /// What a connect is started, finished, read and forgotten through.
    fn connectors(&self) -> &Self::Connectors;

    /// Authoritative task lifecycle over the same fleet run and turn stores.
    fn tasks(&self) -> &dyn afd_fleet::task::TaskLifecycle;

    /// The workspace whose vault holds this deployment's own platform secrets.
    ///
    /// `None` for a deployment that configured none, and every surface that
    /// needs one fails closed. The App ingress is why this crosses the seam:
    /// its signing secret belongs to the deployment, so it must be verified
    /// before there is a binding to reach it through.
    fn platform_admin_workspace(&self) -> Option<&Uuid7>;

    /// Opening a personal account from a verified signup event.
    type Signups: Signups;

    /// The account-opening plane.
    fn signups(&self) -> &Self::Signups;

    /// What tells the identity provider a new account's tenant: the concrete
    /// writer holds an HTTP client built at boot.
    type SignupMetadata: SignupMetadata;

    /// The signup writeback surface.
    fn signup_metadata(&self) -> &Self::SignupMetadata;

    /// What a signup event's signature is checked against. `None` refuses
    /// every delivery: fail-closed is the only safe answer for a route that
    /// creates accounts.
    fn identity_webhook_secret(&self) -> Option<&SecretBytes>;
}
