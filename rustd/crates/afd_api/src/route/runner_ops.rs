//! The operator's view over runners — a tenant looking AT the fleet's hosts.
//!
//! Split from [`super::runner`] along the guard boundary, which is a real one:
//! everything here is `Guard::Bearer`, a tenant principal reading or cordoning
//! runners, and everything there is `Guard::RunnerBearer`, a runner speaking
//! for itself. Keeping them in one table meant every arm restating which of
//! the two planes it belonged to.

use afd_auth::Scope;

use super::path::fleet_runner_path;
use super::{Guard, RouteClass, RouteMeta, Scopes};

const RUNNER_ENROLL: &[Scope] = &[Scope::RunnerEnroll];
const RUNNER_READ: &[Scope] = &[Scope::RunnerRead];
const RUNNER_WRITE: &[Scope] = &[Scope::RunnerWrite];
const STREAM_READ: &[Scope] = &[Scope::StreamRead];

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
    /// One runner's events.
    Events,
    /// One runner's leases.
    Leases,
    /// The live streams open on this instance.
    Streams,
}

impl RunnerOpsRoute {
    /// Every operator route over runners.
    pub const ALL: &'static [Self] = &[
        Self::Register,
        Self::List,
        Self::Get,
        Self::Patch,
        Self::Events,
        Self::Leases,
        Self::Streams,
    ];

    /// Enrolment is held independently of read and write because it is
    /// uniquely dangerous: the host it creates then receives every tenant's
    /// inline secrets, so it is separately grantable and separately revocable
    /// rather than folded into a `runner:admin` rung nobody could withhold.
    #[must_use]
    pub const fn meta(self) -> RouteMeta {
        let (template, scopes) = match self {
            Self::Register => ("/v1/runners", Scopes::Always(RUNNER_ENROLL)),
            Self::List => ("/v1/fleets/runners", Scopes::Always(RUNNER_READ)),
            Self::Get => (fleet_runner_path!(""), Scopes::Always(RUNNER_READ)),
            Self::Patch => (fleet_runner_path!(""), Scopes::Always(RUNNER_WRITE)),
            Self::Events => (fleet_runner_path!("/events"), Scopes::Always(RUNNER_READ)),
            Self::Leases => (fleet_runner_path!("/leases"), Scopes::Always(RUNNER_READ)),
            Self::Streams => ("/v1/fleets/streams", Scopes::Always(STREAM_READ)),
        };
        RouteMeta::new(Guard::Bearer, RouteClass::Api, template, scopes)
    }
}
