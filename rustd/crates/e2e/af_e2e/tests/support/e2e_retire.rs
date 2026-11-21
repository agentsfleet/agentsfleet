//! Archive a scenario fleet so later daemon scenarios cannot run its work.
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use af_core::id::Uuid7;
use afd_fleet_lifecycle::FleetStatus;
use afd_state::sql::{RUN_STATUS_ACTIVE, RUN_STATUS_EXPIRED};
use agentsfleetd::serve::Booted;

/// Archived: terminal, and nothing sweeps or purges it, so retirement starts no
/// further work of its own.
const RETIRED: FleetStatus = FleetStatus::Archived;

pub(crate) async fn retire_fleet(booted: &Booted, fleet: &Uuid7) {
    let mut connection = booted
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query("UPDATE fleet.fleets SET status = $2 WHERE id = $1")
        .bind(fleet)
        .bind(RETIRED.as_str())
        .execute(&mut *connection)
        .await
        .expect("the fleet must retire");
    sqlx::query("UPDATE thread.runs SET status = $3 WHERE fleet_id = $1 AND status = $2")
        .bind(fleet)
        .bind(RUN_STATUS_ACTIVE)
        .bind(RUN_STATUS_EXPIRED)
        .execute(&mut *connection)
        .await
        .expect("the fleet's open runs must expire");
}
