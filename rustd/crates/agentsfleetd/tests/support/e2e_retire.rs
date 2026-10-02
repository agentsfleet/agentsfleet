//! Retiring a finished scenario's fleet, so no later scenario can lease it.
//!
//! Split from `e2e.rs` by concern (RULE FLL): that file boots and seeds, this
//! one makes sure what a scenario leaves behind is inert.
//!
//! # Why clearing the mark was not enough
//!
//! Most scenarios lease their event and never report it, so the fleet ends
//! holding an `active` lease and an undelivered stream entry. Clearing the
//! readiness mark hid that for one poll at most: the next scenario's daemon
//! runs the reclaim sweeper, which re-marks any fleet holding an expired
//! `active` lease. The candidate query admits any `active` fleet with no
//! required tags, so the NEXT scenario's runner was handed the leftover. Every
//! leftover in the fleet's partition cost one of `poll_for_lease`'s visits, and
//! with enough of them the poll budget ran out before it reached the event
//! under test.
//!
//! So retirement takes the fleet out of every path that could offer it again:
//! a status the candidate query refuses, no `active` lease for the stranded
//! pass to surface, and no mark for a poll to sample.
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use afd_dragonfly::ReadyIndex;
use afd_fleet_lifecycle::FleetStatus;
use afd_state::sql::{LEASE_STATUS_ACTIVE, LEASE_STATUS_EXPIRED};
use agentsfleetd::serve::Booted;

/// Stopped rather than killed: an operator's stop is the status nothing sweeps
/// or purges, so retirement starts no further work of its own.
const RETIRED: FleetStatus = FleetStatus::Stopped;

/// Makes `fleet` unleasable, then clears its readiness mark.
///
/// Called after the scenario's daemon has stopped, so nothing re-marks the
/// fleet between the writes and the clear.
pub(crate) async fn retire_fleet(booted: &Booted, fleet: &str) {
    let mut connection = booted
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query("UPDATE core.fleets SET status = $2 WHERE id = $1::uuid")
        .bind(fleet)
        .bind(RETIRED.as_str())
        .execute(&mut *connection)
        .await
        .expect("the fleet must retire");
    sqlx::query(
        "UPDATE fleet.runner_leases SET status = $3 WHERE fleet_id = $1::uuid AND status = $2",
    )
    .bind(fleet)
    .bind(LEASE_STATUS_ACTIVE)
    .bind(LEASE_STATUS_EXPIRED)
    .execute(&mut *connection)
    .await
    .expect("the fleet's open leases must expire");
    ReadyIndex::new(booted.queue.clone())
        .force_clear(fleet)
        .await
        .expect("the fleet's readiness mark must clear");
}
