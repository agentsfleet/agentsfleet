//! Every fact about a route, in one place.
//!
//! # What this replaces
//!
//! Four total switches over one union, in four files: `route_table.zig` (the
//! middleware chain), `route_scopes.zig` (required capabilities),
//! `route_admission.zig` (shed class) and `route_template.zig` (the span
//! template). Adding an endpoint meant editing four files, and the compiler
//! caught only three of those omissions — `route_admission.zig` had traded its
//! exhaustive match for an `else` arm and rebuilt the check as a runtime test
//! over two hand-maintained name lists. [`Route::meta`] states all four facts
//! at once, so a new route fails the build until every one of them is chosen.
//!
//! # Why the enum nests
//!
//! Eighty-one variants in one flat list is a thing nobody reads and therefore
//! nobody reviews. Grouping by family — the ten modules beside this one — keeps
//! each file to a surface a person can hold, and rustc stays exhaustive at BOTH
//! levels: a new family and a new route within a family each fail the build
//! until matched.
//!
//! Two of those families are splits the Zig union did not draw, and both follow
//! a seam that was already there. `workspace`/`fleet` divides on whether a route
//! is addressed by a fleet id; `runner`/`runner_ops` divides on the GUARD — a
//! runner speaking for itself versus a tenant operator looking at runners. Each
//! half then states its guard once instead of restating it per arm.
//!
//! # Why identity carries no payload
//!
//! The Zig union puts path parameters in the route itself
//! (`poll_auth_session: []const u8`), so every metadata switch is
//! payload-shaped and its own exhaustiveness test has to fabricate values with
//! `@unionInit(Route, f.name, undefined)` that it must never read. Here a
//! route is only an identity; parameters are the extractor's job at the
//! handler. There is nothing to fabricate, so nothing to get wrong.

mod admin;
mod auth;
mod connector;
mod fleet;
mod ops;
mod path;
mod runner;
mod runner_ops;
mod tenant;
mod webhook;
mod workspace;

use afd_auth::Scope;
use http::Method;

pub use self::admin::AdminRoute;
pub use self::auth::AuthRoute;
pub use self::connector::ConnectorRoute;
pub use self::fleet::FleetRoute;
pub use self::ops::OpsRoute;
pub use self::runner::RunnerRoute;
pub use self::runner_ops::RunnerOpsRoute;
pub use self::tenant::TenantRoute;
pub use self::webhook::WebhookRoute;
pub use self::workspace::WorkspaceRoute;

/// An HTTP verb a route identity serves.
///
/// Kept as a small copyable enum rather than storing [`http::Method`] values in
/// static slices. The inventory is compile-time data, and converting at the
/// router edge is cheaper and clearer than cloning an owned method throughout
/// tests and route metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Verb {
    /// Read a resource or collection.
    Get,
    /// Create beneath a collection.
    Post,
    /// Replace the addressed setting.
    Put,
    /// Partially update the addressed resource.
    Patch,
    /// Remove the addressed resource.
    Delete,
}

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

/// The four facts that used to live in four files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteMeta {
    /// What must be presented before a handler runs.
    pub guard: Guard,
    /// How this route behaves when the instance is over its ceiling.
    pub class: RouteClass,
    /// The low-cardinality `http.route` template. Never a concrete path: real
    /// paths carry workspace, fleet and secret identifiers, and exporting one
    /// would put tenant identity into span attributes and give the backend a
    /// distinct route value per request.
    pub template: &'static str,
    /// The capability this route requires, which may depend on the method.
    pub scopes: Scopes,
}

impl RouteMeta {
    /// Builds a route's metadata. Kept terse so each route reads as one line.
    #[must_use]
    pub const fn new(
        guard: Guard,
        class: RouteClass,
        template: &'static str,
        scopes: Scopes,
    ) -> Self {
        Self {
            guard,
            class,
            template,
            scopes,
        }
    }
}

/// What a request must present before its handler runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Guard {
    /// Nothing. The route is either a probe or authenticated by its payload.
    Open,
    /// A tenant-plane credential — session bearer, `agt_t`, or `afc_`.
    Bearer,
    /// A runner-plane credential (`agt_r`). Refused for tenant callers by
    /// [`afd_auth::Plane`], which is data rather than which router mounted it.
    RunnerBearer,
    /// An HMAC over the request body, keyed per fleet.
    WebhookHmac,
    /// The per-fleet webhook signature header.
    WebhookSignature,
    /// A Svix-signed delivery.
    Svix,
}

/// What happens to a request when the instance is at its ceiling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteClass {
    /// Never shed. If an instance is too loaded to answer `/readyz`, the
    /// answer an orchestrator needs is the one it cannot get.
    Ops,
    /// Long-lived Server-Sent Events, capped separately: one stream holds a
    /// connection for minutes, so counting it against the request ceiling
    /// would let a handful of dashboards close the API.
    Stream,
    /// An ordinary request, subject to the in-flight ceiling.
    Api,
}

/// The capability a route requires, which some routes vary by method.
///
/// `HEAD` is deliberately absent. agentsfleetd has never served it — the Zig
/// matchers switch on GET, POST and DELETE only — so the router refuses it
/// rather than letting a method with no rung here fall through to a write
/// scope, which is what the Zig `else` arm would have done had it ever routed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scopes {
    /// The same requirement whatever the method.
    Always(&'static [Scope]),
    /// Per-method rungs. Anything unnamed takes `otherwise`, which is always
    /// the more privileged of the pair: a method nobody thought about can only
    /// ever be refused too often, never too rarely.
    ByMethod {
        /// What a `GET` requires, when reads are cheaper than writes.
        get: Option<&'static [Scope]>,
        /// What a `DELETE` requires, when destruction outranks mutation.
        delete: Option<&'static [Scope]>,
        /// Everything else.
        otherwise: &'static [Scope],
    },
}

/// The empty requirement: authenticated, but no capability.
pub const NONE: &[Scope] = &[];

impl Scopes {
    /// A read rung and a write rung.
    #[must_use]
    pub const fn rw(read: &'static [Scope], write: &'static [Scope]) -> Self {
        Self::ByMethod {
            get: Some(read),
            delete: None,
            otherwise: write,
        }
    }

    /// Read, write, and a destructive rung above both.
    #[must_use]
    pub const fn rwa(
        read: &'static [Scope],
        write: &'static [Scope],
        admin: &'static [Scope],
    ) -> Self {
        Self::ByMethod {
            get: Some(read),
            delete: Some(admin),
            otherwise: write,
        }
    }

    /// A write rung with a destructive rung above it, and no cheaper read.
    #[must_use]
    pub const fn wa(write: &'static [Scope], admin: &'static [Scope]) -> Self {
        Self::ByMethod {
            get: None,
            delete: Some(admin),
            otherwise: write,
        }
    }

    /// The scopes `method` requires on this route.
    ///
    /// Any one of them satisfies it. The requirement is the MINIMAL scope and
    /// relies on the parse-time hierarchy closure, so a `fleet:admin` holder
    /// passes a `fleet:read` gate and the refusal names the least scope that
    /// would unblock the caller.
    #[must_use]
    pub fn required(self, method: &Method) -> &'static [Scope] {
        match self {
            Self::Always(scopes) => scopes,
            Self::ByMethod {
                get,
                delete,
                otherwise,
            } => match *method {
                Method::GET => get.unwrap_or(otherwise),
                Method::DELETE => delete.unwrap_or(otherwise),
                _ => otherwise,
            },
        }
    }
}
