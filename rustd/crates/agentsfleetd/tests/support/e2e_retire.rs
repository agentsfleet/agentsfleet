//! Stop a scenario fleet so later daemon scenarios cannot lease its work.
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use afd_fleet_lifecycle::FleetStatus;
use afd_state::sql::{LEASE_STATUS_ACTIVE, LEASE_STATUS_EXPIRED};
use agentsfleetd::serve::Booted;

/// Stopped rather than killed: an operator's stop is the status nothing sweeps
/// or purges, so retirement starts no further work of its own.
const RETIRED: FleetStatus = FleetStatus::Stopped;

pub(crate) async fn retire_fleet(booted: &Booted, fleet: &str) {
    let mut connection = booted
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query("UPDATE fleet.fleets SET status = $2 WHERE id = $1::uuid")
        .bind(fleet)
        .bind(RETIRED.as_str())
        .execute(&mut *connection)
        .await
        .expect("the fleet must retire");
    sqlx::query("UPDATE runner.leases SET status = $3 WHERE fleet_id = $1::uuid AND status = $2")
        .bind(fleet)
        .bind(LEASE_STATUS_ACTIVE)
        .bind(LEASE_STATUS_EXPIRED)
        .execute(&mut *connection)
        .await
        .expect("the fleet's open leases must expire");
}
