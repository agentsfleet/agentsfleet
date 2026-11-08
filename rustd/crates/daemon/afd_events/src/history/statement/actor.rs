//! One actor's events in one fleet, the texts behind a schedule's run list.
//! Split from [`super`] at its length cap; they carry its column list.
//!
//! An equality, never a pattern. `idx_events_fleet_id_actor_created_at_event_id`
//! keys `(fleet_id, actor, created_at DESC, event_id DESC)`, so the fleet and
//! the actor are index conditions and the order comes off the index however
//! sparse the actor is in the fleet's history. A bound LIKE pattern cannot seek
//! that index under a generic plan, and one schedule's runs are sparse: a page
//! would read the fleet's whole history to fill.

/// One actor's first page in a fleet: `$3` actor, `$4` limit.
pub(in crate::history) const SELECT_FLEET_PAGE_OF_ACTOR: &str =
    include_str!("select_fleet_page_of_actor.sql");

/// One actor's page in a fleet after a cursor: `$3` cursor instant, `$4`
/// cursor event id, `$5` actor, `$6` limit.
pub(in crate::history) const SELECT_FLEET_PAGE_OF_ACTOR_AFTER: &str =
    include_str!("select_fleet_page_of_actor_after.sql");
