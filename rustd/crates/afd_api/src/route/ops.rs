//! Liveness and readiness. The only two routes this milestone serves.

use super::{Guard, RouteClass, RouteMeta, Scopes};

/// The operational probes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OpsRoute {
    /// Liveness: the process is up and answering.
    Healthz,
    /// Readiness: every dependency this instance needs is reachable.
    Readyz,
}

impl OpsRoute {
    /// Every ops route.
    pub const ALL: &'static [Self] = &[Self::Healthz, Self::Readyz];

    /// Both probes are unauthenticated and never shed. An orchestrator that
    /// cannot reach `/readyz` because the instance is busy learns nothing it
    /// can act on, which is the one case where shedding is worse than serving.
    #[must_use]
    pub const fn meta(self) -> RouteMeta {
        let template = match self {
            Self::Healthz => "/healthz",
            Self::Readyz => "/readyz",
        };
        RouteMeta::new(
            Guard::Open,
            RouteClass::Ops,
            template,
            Scopes::Always(super::NONE),
        )
    }
}
