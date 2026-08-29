//! The runner plane: a host speaking for itself with an `agt_r` token.
//!
//! Every route here is `Guard::RunnerBearer` and every one requires
//! [`afd_auth::Scope::RunnerSelf`]. A tenant credential arriving here is
//! refused before any lookup by [`afd_auth::Plane`] — the boundary is data,
//! not which middleware happened to be mounted, which is what makes it a fact
//! the type system can hold rather than a wiring convention.
//!
//! The operator's view over runners is [`super::runner_ops`].

use afd_auth::Scope;

use super::path::runner_path;
use super::{Guard, RouteClass, RouteMeta, Scopes};

/// What a runner may do on its own behalf. One scope, because the plane IS the
/// authorisation: a runner token is not a capability a person hands out.
const RUNNER_SELF: &[Scope] = &[Scope::RunnerSelf];

/// A runner-plane route.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RunnerRoute {
    /// The runner reading its own record.
    SelfRecord,
    /// The runner's heartbeat.
    Heartbeat,
    /// Claiming a lease.
    Lease,
    /// Reporting on a lease.
    Report,
    /// Minting the per-lease credentials a fleet needs.
    CredentialsMint,
    /// Reporting activity against a held lease.
    Activity,
    /// Renewing a held lease.
    Renew,
    /// Loading a fleet's memory at lease start.
    MemoryHydrate,
    /// Writing a fleet's memory back.
    MemoryCapture,
    /// Fetching a fleet bundle by content hash.
    Bundle,
}

impl RunnerRoute {
    /// Every runner-plane route.
    pub const ALL: &'static [Self] = &[
        Self::SelfRecord,
        Self::Heartbeat,
        Self::Lease,
        Self::Report,
        Self::CredentialsMint,
        Self::Activity,
        Self::Renew,
        Self::MemoryHydrate,
        Self::MemoryCapture,
        Self::Bundle,
    ];

    /// Hydrate and capture share a path and differ by method, so they share an
    /// arm here. They stay two routes because they are two operations — one
    /// reads a fleet's memory at lease start, the other writes it back — and
    /// collapsing them would lose the distinction every other table keys on.
    #[must_use]
    pub const fn meta(self) -> RouteMeta {
        let template = match self {
            Self::SelfRecord => runner_path!("/me"),
            Self::Heartbeat => runner_path!("/me/heartbeats"),
            Self::Lease => runner_path!("/me/leases"),
            Self::Report => runner_path!("/me/reports"),
            Self::CredentialsMint => runner_path!("/me/credentials/mint"),
            Self::Activity => runner_path!("/me/leases/{lease_id}/activity"),
            Self::Renew => runner_path!("/me/leases/{lease_id}/renew"),
            Self::MemoryHydrate | Self::MemoryCapture => runner_path!("/me/memory/{fleet_id}"),
            Self::Bundle => runner_path!("/me/bundles/{content_hash}"),
        };
        RouteMeta::new(
            Guard::RunnerBearer,
            RouteClass::Api,
            template,
            Scopes::Always(RUNNER_SELF),
        )
    }
}
