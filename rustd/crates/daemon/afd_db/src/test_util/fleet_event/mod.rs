//! The `fleet.events` row a suite seeds, spelled once.
//!
//! A runner lease names the event it executes through a foreign key, so every
//! suite that seeds a lease first needs an event row. They build a
//! [`FleetEventSeed`] rather than each spelling the insert, and the column list
//! changes here alone.
//!
//! The actor, type and status are arguments because their spellings live above
//! this crate; the request body is an empty object, which every reader of the
//! column accepts. A lease names the event by its `event_id`, the table's key.

use af_core::id::Uuid7;
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;
use sqlx::PgExecutor;

use crate::{Result, error};

/// Every column an event seed sets.
const INSERT_FLEET_EVENT: &str = include_str!("sql/insert.sql");

const CONTEXT: &str = "seed a fleet event";

/// The request body an event is seeded with.
const EMPTY_REQUEST: &str = "{}";

/// Who sent an event, what kind it is, and where it stands.
#[derive(Debug, Clone, Copy)]
pub struct EventKind<'a> {
    /// The actor, as the column stores it.
    pub actor: &'a str,
    /// The event type, as its stored spelling.
    pub event_type: &'a str,
    /// The status, as its stored spelling.
    pub status: &'a str,
}

/// One event row under a fleet.
#[derive(Debug, Clone, Copy)]
#[must_use = "a seed writes nothing until `insert` runs"]
pub struct FleetEventSeed<'a> {
    fleet_id: &'a Uuid7,
    workspace_id: &'a Uuid7,
    event_id: &'a Uuid7,
    kind: EventKind<'a>,
    created_at: Option<Timestamp>,
}

impl<'a> FleetEventSeed<'a> {
    /// The event `event_id`, stamped by the database's `now()`.
    pub const fn new(
        fleet_id: &'a Uuid7,
        workspace_id: &'a Uuid7,
        event_id: &'a Uuid7,
        kind: EventKind<'a>,
    ) -> Self {
        Self {
            fleet_id,
            workspace_id,
            event_id,
            kind,
            created_at: None,
        }
    }

    /// Stamps `created_at` and `updated_at` with `instant`.
    pub const fn at(mut self, instant: Timestamp) -> Self {
        self.created_at = Some(instant);
        self
    }

    /// Writes the row and answers its `event_id`.
    ///
    /// # Errors
    /// [`crate::Error`] when the insert fails: a missing fleet or workspace, or
    /// a producer identifier the fleet already holds.
    pub async fn insert<'e>(self, executor: impl PgExecutor<'e>) -> Result<Uuid7> {
        sqlx::query_scalar(INSERT_FLEET_EVENT)
            .bind(self.fleet_id)
            .bind(self.workspace_id)
            .bind(self.event_id)
            .bind(self.kind.actor)
            .bind(self.kind.event_type)
            .bind(self.kind.status)
            .bind(EMPTY_REQUEST)
            .bind(self.created_at.map(Timestamp::to_sqlx))
            .fetch_one(executor)
            .await
            .map_err(|source| error::query(CONTEXT, source))
    }
}

#[cfg(test)]
mod tests {
    use super::{EventKind, FleetEventSeed};

    #[test]
    fn a_seed_stamps_nothing_until_the_suite_backdates_it() {
        let kind = EventKind {
            actor: "test",
            event_type: "chat",
            status: "received",
        };
        let (fleet, workspace, event) = (
            crate::test_util::mint_id(),
            crate::test_util::mint_id(),
            crate::test_util::mint_id(),
        );
        let seed = FleetEventSeed::new(&fleet, &workspace, &event, kind);

        assert_eq!((seed.event_id, seed.created_at), (&event, None));
    }
}
