//! Runner management: platform runners under `/v1/admin/runners`, and a tenant's
//! own runners under `/v1/tenants/me/runners`, served by one handler set.
//!
//! Split from [`super::runner`] along the guard boundary, which is a real one:
//! everything here is `Guard::Bearer`, a tenant principal reading or cordoning
//! runners, and everything there is `Guard::RunnerBearer`, a runner speaking
//! for itself. Keeping them in one table meant every arm restating which of
//! the two planes it belonged to.

use af_wire::paths;
use afd_auth::Scope;

use super::path::{admin_runner_path, tenant_runner_path};
use super::{Guard, RouteClass, RouteMeta, Scopes, Verb};

const RUNNER_REGISTER: &[Scope] = &[Scope::RunnerRegister];
const RUNNER_READ: &[Scope] = &[Scope::RunnerRead];
const RUNNER_WRITE: &[Scope] = &[Scope::RunnerWrite];
const RUNNER_TENANT: &[Scope] = &[Scope::RunnerTenant];

/// An operator-plane route over runners.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RunnerOpsRoute {
    /// Enrol a trusted runner, minting its `agt_r` token.
    Register,
    /// Every runner this tenant can see.
    List,
    /// One runner.
    Get,
    /// Cordon or patch one runner.
    Patch,
    /// Retire one revoked runner's record.
    Delete,
    /// One runner's events.
    Events,
    /// One runner's leases.
    Leases,
    /// Register one of the calling tenant's runners.
    TenantRegister,
    /// The calling tenant's runners.
    TenantList,
    /// One of the calling tenant's runners.
    TenantGet,
    /// Cordon, rotate or edit one of the calling tenant's runners.
    TenantPatch,
    /// Retire one of the calling tenant's revoked runners.
    TenantDelete,
}

// `/v1/fleets/streams`, a per-instance operator diagnostic over the
// Server-Sent Events (SSE) streams, is deliberately absent, by Indy's call
// while merging M179 into M178: nothing consumes it (no UI, no CLI, absent from
// the public OpenAPI document by its own carve-out), and serving it would mean
// carrying a live-stream census whose only reader is the endpoint itself.
// Recorded in M179's Dimension 4.4 rather than left to be noticed.

impl RunnerOpsRoute {
    /// Every operator route over runners.
    pub const ALL: &'static [Self] = &[
        Self::Register,
        Self::List,
        Self::Get,
        Self::Patch,
        Self::Delete,
        Self::Events,
        Self::Leases,
        Self::TenantRegister,
        Self::TenantList,
        Self::TenantGet,
        Self::TenantPatch,
        Self::TenantDelete,
    ];

    /// The verbs this operator route serves.
    #[must_use]
    pub const fn verbs(self) -> &'static [Verb] {
        match self {
            Self::Register | Self::TenantRegister => &[Verb::Post],
            Self::List
            | Self::Get
            | Self::Events
            | Self::Leases
            | Self::TenantList
            | Self::TenantGet => &[Verb::Get],
            Self::Patch | Self::TenantPatch => &[Verb::Patch],
            Self::Delete | Self::TenantDelete => &[Verb::Delete],
        }
    }

    /// Registration is held independently of read and write because it is
    /// uniquely dangerous: the host it creates then receives every tenant's
    /// inline secrets, so it is separately grantable and separately revocable
    /// rather than folded into a `runner:admin` rung nobody could withhold.
    #[must_use]
    pub const fn meta(self) -> RouteMeta {
        let (template, scopes) = match self {
            Self::Register | Self::List => (
                paths::ADMIN_RUNNERS,
                Scopes::rw(RUNNER_READ, RUNNER_REGISTER),
            ),
            // These identities share one axum path. Both carry the same
            // method-sensitive metadata so merging them cannot retain a
            // cheaper GET-only gate for PATCH.
            Self::Get | Self::Patch | Self::Delete => (
                admin_runner_path!(""),
                Scopes::rw(RUNNER_READ, RUNNER_WRITE),
            ),
            Self::Events => (admin_runner_path!("/events"), Scopes::Always(RUNNER_READ)),
            Self::Leases => (admin_runner_path!("/leases"), Scopes::Always(RUNNER_READ)),
            Self::TenantRegister | Self::TenantList => {
                (paths::TENANT_RUNNERS, Scopes::Always(RUNNER_TENANT))
            }
            Self::TenantGet | Self::TenantPatch | Self::TenantDelete => {
                (tenant_runner_path!(""), Scopes::Always(RUNNER_TENANT))
            }
        };
        RouteMeta::new(Guard::Bearer, RouteClass::Api, template, scopes)
    }
}
