//! Routes beneath a fleet inside one workspace.
//!
//! Split from [`super::workspace`] along the seam the paths already draw:
//! everything here is addressed by a fleet id as well as a workspace id, and
//! everything there is not. The split keeps both tables to a size a reviewer
//! can hold, which matters more here than anywhere else — this is where the
//! capability ladder does most of its work.

use afd_auth::Scope;

use super::path::fleet_path;
use super::{Guard, RouteClass, RouteMeta, Scopes, Verb};

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
    /// Apply a reviewed private improvement to this fleet's setup.
    Revisions,
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
    /// One tool call of an event, in full.
    ToolCall,
    /// What the fleet remembers.
    Memories,
    /// One memory entry.
    Memory,
    /// The fleet's two shared-memory grants.
    MemoryAccess,
    /// The fleet's integration grants.
    Grants,
    /// Revoking one grant.
    Grant,
}

impl FleetRoute {
    /// Every fleet route.
    pub const ALL: &'static [Self] = &[
        Self::Detail,
        Self::Revisions,
        Self::Messages,
        Self::Schedules,
        Self::Schedule,
        Self::ScheduleSync,
        Self::Events,
        Self::EventsStream,
        Self::Event,
        Self::ToolCall,
        Self::Memories,
        Self::Memory,
        Self::MemoryAccess,
        Self::Grants,
        Self::Grant,
    ];

    /// The verbs this route identity serves.
    ///
    /// No `PUT` sits beside either `PATCH`. A fleet and a schedule are both
    /// edited field by field, and a whole-row replace would let a caller
    /// silently drop the upstream handle the schedule sync reconciles against.
    #[must_use]
    pub const fn verbs(self) -> &'static [Verb] {
        match self {
            Self::Events
            | Self::EventsStream
            | Self::Event
            | Self::ToolCall
            | Self::Memories
            | Self::Grants => &[Verb::Get],
            Self::Memory | Self::Grant => &[Verb::Delete],
            Self::ScheduleSync | Self::Revisions => &[Verb::Post],
            Self::MemoryAccess => &[Verb::Patch],
            Self::Messages | Self::Schedules => &[Verb::Get, Verb::Post],
            Self::Detail | Self::Schedule => &[Verb::Get, Verb::Patch, Verb::Delete],
        }
    }

    /// The route's guard, class, template and scope rungs, as one table row.
    #[must_use]
    pub const fn meta(self) -> RouteMeta {
        RouteMeta::new(Guard::Bearer, self.class(), self.template(), self.scopes())
    }

    /// The event tail streams; everything else answers once. Exhaustive
    /// rather than a wildcard, so a new route has to say which it is.
    const fn class(self) -> RouteClass {
        match self {
            Self::EventsStream => RouteClass::Stream,
            Self::Detail
            | Self::Revisions
            | Self::Messages
            | Self::Schedules
            | Self::Schedule
            | Self::ScheduleSync
            | Self::Events
            | Self::Event
            | Self::ToolCall
            | Self::Memories
            | Self::Memory
            | Self::MemoryAccess
            | Self::Grants
            | Self::Grant => RouteClass::Api,
        }
    }

    /// Where the route is mounted, under the fleet.
    const fn template(self) -> &'static str {
        match self {
            Self::Detail => fleet_path!(""),
            Self::Revisions => fleet_path!("/revisions"),
            Self::Messages => fleet_path!("/messages"),
            Self::Schedules => fleet_path!("/schedules"),
            Self::Schedule => fleet_path!("/schedules/{schedule_id}"),
            // `/sync` as its own segment rather than the custom verb
            // `…/{schedule_id}:sync`. The same choice the approval decision
            // took — see [`crate::route::WorkspaceRoute::ApprovalResolve`].
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
            Self::ScheduleSync => fleet_path!("/schedules/{schedule_id}/sync"),
            Self::Events => fleet_path!("/events"),
            Self::EventsStream => fleet_path!("/events/stream"),
            Self::Event => fleet_path!("/events/{event_id}"),
            Self::ToolCall => fleet_path!("/events/{event_id}/tool-calls/{call_id}"),
            Self::Memories => fleet_path!("/memories"),
            Self::Memory => fleet_path!("/memories/{key}"),
            Self::MemoryAccess => fleet_path!("/memory-access"),
            Self::Grants => fleet_path!("/integration-grants"),
            Self::Grant => fleet_path!("/integration-grants/{grant_id}"),
        }
    }

    /// Reading a thread is a read and steering it is a write; deleting the
    /// fleet outranks both. Forgetting a memory takes the write scope because
    /// it mutates what the fleet knows — it is not a lifecycle transition, so
    /// it is not `fleet:admin`.
    const fn scopes(self) -> Scopes {
        match self {
            Self::Detail => Scopes::rwa(FLEET_READ, FLEET_WRITE, FLEET_ADMIN),
            // Who may read and publish shared memory is an edit to the fleet,
            // not a lifecycle transition, so it takes the write scope.
            Self::Revisions | Self::Memory | Self::MemoryAccess => Scopes::Always(FLEET_WRITE),
            Self::Messages => Scopes::rw(FLEET_READ, FLEET_WRITE),
            Self::Schedules | Self::Schedule => Scopes::rw(SCHEDULE_READ, SCHEDULE_WRITE),
            Self::ScheduleSync => Scopes::Always(SCHEDULE_WRITE),
            Self::Events | Self::EventsStream | Self::Event | Self::ToolCall | Self::Memories => {
                Scopes::Always(FLEET_READ)
            }
            Self::Grants => Scopes::Always(GRANT_READ),
            Self::Grant => Scopes::Always(GRANT_WRITE),
        }
    }
}
