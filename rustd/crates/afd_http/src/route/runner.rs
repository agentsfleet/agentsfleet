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
use afd_wire::paths;

use super::{Guard, RouteClass, RouteMeta, Scopes, Verb};

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
    /// Searching a fleet's memory past the window a run was seeded with.
    MemoryRecall,
    /// Fetching a fleet bundle by content hash.
    Bundle,
    /// Keeping each finished tool call's full arguments and output.
    ToolCalls,
    /// The schedules of the fleet a held lease runs: listing them, and a
    /// fleet creating its own.
    Schedules,
    /// One schedule the fleet created: changing it, and deleting it.
    Schedule,
    /// One schedule's runs: reading them, and creating one to fire it now.
    ScheduleRuns,
    /// A line said to the event's thread before the answer.
    Messages,
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
        Self::MemoryRecall,
        Self::Bundle,
        Self::ToolCalls,
        Self::Schedules,
        Self::Schedule,
        Self::ScheduleRuns,
        Self::Messages,
    ];

    /// The verbs this route identity serves.
    ///
    /// Reads are the runner asking what it has been given — its own record, a
    /// fleet's memory, a bundle by content hash. Reporting is a `POST` because
    /// each one appends a fact rather than replacing a resource. The schedules
    /// are the one resource a runner manages: a collection it lists and adds
    /// to, members it edits and removes, and runs it reads and creates.
    #[must_use]
    pub const fn verbs(self) -> &'static [Verb] {
        match self {
            Self::Schedules | Self::ScheduleRuns => &[Verb::Get, Verb::Post],
            Self::Schedule => &[Verb::Patch, Verb::Delete],
            Self::SelfRecord | Self::MemoryHydrate | Self::Bundle => &[Verb::Get],
            Self::Heartbeat
            | Self::Lease
            | Self::Report
            | Self::CredentialsMint
            | Self::Activity
            | Self::Renew
            | Self::MemoryCapture
            | Self::MemoryRecall
            | Self::ToolCalls
            | Self::Messages => &[Verb::Post],
        }
    }

    /// Hydrate and capture share a path and differ by method, so they share an
    /// arm here. They stay two routes because they are two operations — one
    /// reads a fleet's memory at lease start, the other writes it back — and
    /// collapsing them would lose the distinction every other table keys on.
    #[must_use]
    pub const fn meta(self) -> RouteMeta {
        let template = match self {
            Self::SelfRecord => paths::RUNNER_SELF,
            Self::Heartbeat => paths::RUNNER_HEARTBEATS,
            Self::Lease => paths::RUNNER_LEASES,
            Self::Report => paths::RUNNER_REPORTS,
            Self::CredentialsMint => paths::RUNNER_CREDENTIALS_MINT,
            Self::Activity => paths::LEASE_ACTIVITY,
            Self::Renew => paths::LEASE_RENEW,
            Self::MemoryHydrate | Self::MemoryCapture => paths::RUNNER_MEMORY_FLEET,
            Self::MemoryRecall => paths::RUNNER_MEMORY_RECALL,
            Self::Bundle => paths::RUNNER_BUNDLE,
            Self::ToolCalls => paths::LEASE_TOOL_CALLS,
            Self::Schedules => paths::LEASE_SCHEDULES,
            Self::Schedule => paths::LEASE_SCHEDULE,
            Self::ScheduleRuns => paths::LEASE_SCHEDULE_RUNS,
            Self::Messages => paths::LEASE_MESSAGES,
        };
        RouteMeta::new(
            Guard::RunnerBearer,
            RouteClass::Api,
            template,
            Scopes::Always(RUNNER_SELF),
        )
    }
}
