//! The operator view over runners — a tenant looking at the fleet's hosts.
//!
//! Split from [`super::runner`] along the guard boundary, which is a real one:
//! everything here is `Guard::Bearer`, a tenant principal reading or cordoning
//! runners, and everything there is `Guard::RunnerBearer`, a runner speaking
//! for itself. Keeping them in one table meant every arm restating which of
//! the two planes it belonged to.

use afd_auth::Scope;
use afd_wire::paths;

use super::path::fleet_runner_path;
use super::{Guard, RouteClass, RouteMeta, Scopes, Verb};

const RUNNER_ENROLL: &[Scope] = &[Scope::RunnerEnroll];
const RUNNER_READ: &[Scope] = &[Scope::RunnerRead];
const RUNNER_WRITE: &[Scope] = &[Scope::RunnerWrite];

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
    ];

    /// The verbs this operator route serves.
    #[must_use]
    pub const fn verbs(self) -> &'static [Verb] {
        match self {
            Self::Register => &[Verb::Post],
            Self::List | Self::Get | Self::Events | Self::Leases => &[Verb::Get],
            Self::Patch => &[Verb::Patch],
            Self::Delete => &[Verb::Delete],
        }
    }

    /// Enrolment is held independently of read and write because it is
    /// uniquely dangerous: the host it creates then receives every tenant's
    /// inline secrets, so it is separately grantable and separately revocable
    /// rather than folded into a `runner:admin` rung nobody could withhold.
    #[must_use]
    pub const fn meta(self) -> RouteMeta {
        let (template, scopes) = match self {
            Self::Register => (paths::RUNNERS, Scopes::Always(RUNNER_ENROLL)),
            Self::List => ("/v1/fleets/runners", Scopes::Always(RUNNER_READ)),
            // These identities share one axum path. Both carry the same
            // method-sensitive metadata so merging them cannot retain a
            // cheaper GET-only gate for PATCH.
            Self::Get | Self::Patch | Self::Delete => (
                fleet_runner_path!(""),
                Scopes::rw(RUNNER_READ, RUNNER_WRITE),
            ),
            Self::Events => (fleet_runner_path!("/events"), Scopes::Always(RUNNER_READ)),
            Self::Leases => (fleet_runner_path!("/leases"), Scopes::Always(RUNNER_READ)),
        };
        RouteMeta::new(Guard::Bearer, RouteClass::Api, template, scopes)
    }
}
