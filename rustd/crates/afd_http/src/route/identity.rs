//! The exhaustive route identity and its family traversal.

use super::{
    AdminRoute, AuthRoute, ConnectorRoute, FleetRoute, OpsRoute, RouteMeta, RunnerOpsRoute,
    RunnerRoute, TenantRoute, WebhookRoute, WorkspaceRoute,
};

/// Every route this daemon knows, grouped by the surface it belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Route {
    /// Liveness and readiness probes.
    Ops(OpsRoute),
    /// The device-flow login surface and identity events.
    Auth(AuthRoute),
    /// Tenant-scoped self-service: billing, credentials, model registry.
    Tenant(TenantRoute),
    /// The platform plane, held by platform-scoped principals only.
    Admin(AdminRoute),
    /// Inbound deliveries authenticated by signature rather than bearer.
    Webhook(WebhookRoute),
    /// A workspace's own surface: secrets, events, approvals, preferences.
    Workspace(WorkspaceRoute),
    /// Everything addressed by a fleet id as well as a workspace id.
    Fleet(FleetRoute),
    /// Third-party connector authorisation flows.
    Connector(ConnectorRoute),
    /// The runner plane — a runner speaking for itself.
    Runner(RunnerRoute),
    /// The operator's view over runners, held by a tenant principal.
    RunnerOps(RunnerOpsRoute),
}

impl Route {
    /// Every route, in family order.
    ///
    /// An iterator rather than a `const` array because the families are
    /// distinct types; chaining them costs nothing and keeps each family's
    /// roster owned by the family.
    pub fn all() -> impl Iterator<Item = Self> {
        (OpsRoute::ALL.iter().copied().map(Self::Ops))
            .chain(AuthRoute::ALL.iter().copied().map(Self::Auth))
            .chain(TenantRoute::ALL.iter().copied().map(Self::Tenant))
            .chain(AdminRoute::ALL.iter().copied().map(Self::Admin))
            .chain(WebhookRoute::ALL.iter().copied().map(Self::Webhook))
            .chain(WorkspaceRoute::ALL.iter().copied().map(Self::Workspace))
            .chain(FleetRoute::ALL.iter().copied().map(Self::Fleet))
            .chain(ConnectorRoute::ALL.iter().copied().map(Self::Connector))
            .chain(RunnerRoute::ALL.iter().copied().map(Self::Runner))
            .chain(RunnerOpsRoute::ALL.iter().copied().map(Self::RunnerOps))
    }

    /// Everything the shell needs to know about this route.
    #[must_use]
    pub const fn meta(self) -> RouteMeta {
        match self {
            Self::Ops(route) => route.meta(),
            Self::Auth(route) => route.meta(),
            Self::Tenant(route) => route.meta(),
            Self::Admin(route) => route.meta(),
            Self::Webhook(route) => route.meta(),
            Self::Workspace(route) => route.meta(),
            Self::Fleet(route) => route.meta(),
            Self::Connector(route) => route.meta(),
            Self::Runner(route) => route.meta(),
            Self::RunnerOps(route) => route.meta(),
        }
    }
}
