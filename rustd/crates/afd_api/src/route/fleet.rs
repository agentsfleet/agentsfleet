//! Routes beneath one fleet inside one workspace.
//!
//! Split from [`super::workspace`] along the seam the paths already draw:
//! everything here is addressed by a fleet id as well as a workspace id, and
//! everything there is not. The split keeps both tables to a size a reviewer
//! can hold, which matters more here than anywhere else — this is where the
//! capability ladder does most of its work.

use afd_auth::Scope;

use super::path::fleet_path;
use super::{Guard, RouteClass, RouteMeta, Scopes};

const FLEET_READ: &[Scope] = &[Scope::FleetRead];
const FLEET_WRITE: &[Scope] = &[Scope::FleetWrite];
const FLEET_ADMIN: &[Scope] = &[Scope::FleetAdmin];
const SCHEDULE_READ: &[Scope] = &[Scope::ScheduleRead];
const SCHEDULE_WRITE: &[Scope] = &[Scope::ScheduleWrite];
const GRANT_READ: &[Scope] = &[Scope::GrantRead];
const GRANT_WRITE: &[Scope] = &[Scope::GrantWrite];

/// A route addressed by workspace AND fleet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FleetRoute {
    /// One fleet: read it, steer it, or delete it.
    Detail,
    /// The fleet's message thread.
    Messages,
    /// The fleet's hosted schedules.
    Schedules,
    /// One hosted schedule.
    Schedule,
    /// Forcing a schedule to sync now.
    ScheduleSync,
    /// The fleet's event history.
    Events,
    /// The fleet's live event stream.
    EventsStream,
    /// One event.
    Event,
    /// What the fleet remembers.
    Memories,
    /// One memory entry.
    Memory,
    /// The fleet's integration grants.
    Grants,
    /// Revoking one grant.
    Grant,
}

impl FleetRoute {
    /// Every fleet route.
    pub const ALL: &'static [Self] = &[
        Self::Detail,
        Self::Messages,
        Self::Schedules,
        Self::Schedule,
        Self::ScheduleSync,
        Self::Events,
        Self::EventsStream,
        Self::Event,
        Self::Memories,
        Self::Memory,
        Self::Grants,
        Self::Grant,
    ];

    /// Reading a thread is a read and steering it is a write; deleting the
    /// fleet outranks both. Forgetting a memory takes the write scope because
    /// it mutates what the fleet knows — it is not a lifecycle transition, so
    /// it is not `fleet:admin`.
    #[must_use]
    pub const fn meta(self) -> RouteMeta {
        let api = RouteClass::Api;
        let (class, template, scopes) = match self {
            Self::Detail => (
                api,
                fleet_path!(""),
                Scopes::rwa(FLEET_READ, FLEET_WRITE, FLEET_ADMIN),
            ),
            Self::Messages => (
                api,
                fleet_path!("/messages"),
                Scopes::rw(FLEET_READ, FLEET_WRITE),
            ),
            Self::Schedules => (
                api,
                fleet_path!("/schedules"),
                Scopes::rw(SCHEDULE_READ, SCHEDULE_WRITE),
            ),
            Self::Schedule => (
                api,
                fleet_path!("/schedules/{schedule_id}"),
                Scopes::rw(SCHEDULE_READ, SCHEDULE_WRITE),
            ),
            // `/sync` as its own segment, where the Zig daemon and the shipped
            // clients spell it `…/{schedule_id}:sync`. A deliberate divergence,
            // and the same one the approval decision took — see
            // [`crate::route::WorkspaceRoute::ApprovalResolve`].
            //
            // The reason is the router: `matchit` refuses any literal after a
            // parameter inside one segment (`tree.rs:783`, "Prefixes after route
            // parameters are not supported"), so a custom verb cannot be part of
            // a pattern here at all. The alternative was capturing the whole leaf
            // and stripping the suffix in the handler, which works but leaves the
            // verb invisible to the route table — and this table exists so that
            // every fact about a route is stated in one place.
            //
            // A published-surface change, so it travels with its clients:
            // `cli/src/lib/api-paths.ts` and `public/openapi.json` name the new
            // spelling in the same diff.
            Self::ScheduleSync => (
                api,
                fleet_path!("/schedules/{schedule_id}/sync"),
                Scopes::Always(SCHEDULE_WRITE),
            ),
            Self::Events => (api, fleet_path!("/events"), Scopes::Always(FLEET_READ)),
            Self::EventsStream => (
                RouteClass::Stream,
                fleet_path!("/events/stream"),
                Scopes::Always(FLEET_READ),
            ),
            Self::Event => (
                api,
                fleet_path!("/events/{event_id}"),
                Scopes::Always(FLEET_READ),
            ),
            Self::Memories => (api, fleet_path!("/memories"), Scopes::Always(FLEET_READ)),
            Self::Memory => (
                api,
                fleet_path!("/memories/{key}"),
                Scopes::Always(FLEET_WRITE),
            ),
            Self::Grants => (
                api,
                fleet_path!("/integration-grants"),
                Scopes::Always(GRANT_READ),
            ),
            Self::Grant => (
                api,
                fleet_path!("/integration-grants/{grant_id}"),
                Scopes::Always(GRANT_WRITE),
            ),
        };
        RouteMeta::new(Guard::Bearer, class, template, scopes)
    }
}
