//! A fleet's thread shows a message that waits for a runner, and shows it once.
//!
//! A steer is admitted and put on the fleet's queue; no runner takes it. The
//! first page of the thread carries it as a `queued` row with its body. Once a
//! lease stamps the admission delivered and writes the history row, the same
//! event appears once, as the history row. The waiting read is planned against
//! a private database, so its index is the one the statement was written for.
//!
//! Marked `#[ignore]`; `make test-integration-rustd` runs it.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

mod paging;
mod plans;
mod read;
mod receipt_recovery;

use afd_core::clock;
use afd_core::event::status;
use afd_core::id::Uuid7;
use afd_dragonfly::streams::FleetStreams;
use afd_events::{Cursor, History, QUEUED_READ_TEXTS, Steer};

use crate::integration_list_plans::{explain_generic, private_database};
use crate::integration_steer_retry::clean;
use crate::support::EventsLane;

/// A person's actor, as a steer records it.
const ACTOR: &str = "steer:user_waiting";

/// What they typed, as the route stores it.
const BODY: &str = r#"{"message":"wait for me"}"#;

/// A first page's size, as the dashboard asks for it.
const PAGE: i64 = 26;

/// What a lease writes to the admission when a runner takes it.
const MARK_DELIVERED: &str = "UPDATE core.fleet_admissions SET delivered_at = $2 \
     WHERE fleet_id = $1::uuid AND delivered_at IS NULL";

/// The two indexes that hold only undelivered admissions, keyed on the fleet
/// and the admission order (`schema/910`, `schema/914`).
const IN_FLIGHT_INDEXES: [&str; 2] = [
    "idx_fleet_admissions_undelivered",
    "idx_fleet_admissions_delivery_lookup",
];

/// Ledger history, most of it delivered, across many fleets of one workspace:
/// `$1` tenant, `$2` instant, `$3` fleets, `$4` admissions per fleet.
const SEED_LEDGER: &str = "
WITH workspace AS (
    INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at)
    VALUES (uuidv7(), $1::uuid, 'queued-plan', 'queued-plan', $2)
    RETURNING id
), fleets AS (
    INSERT INTO core.fleets
      (id, workspace_id, tenant_id, name, config_json, status,
       created_at, updated_at)
    SELECT uuidv7(), workspace.id, $1::uuid, 'queued-plan-' || f,
           '{}'::jsonb, 'active', $2, $2
      FROM workspace, generate_series(1, $3) f
    RETURNING id, workspace_id
)
INSERT INTO core.fleet_admissions
  (id, fleet_id, workspace_id, producer, producer_key, payload_digest, actor,
   event_type, request_json, event_created_at, receipt, delivered_at,
   replay_count, created_at, updated_at)
SELECT uuidv7(), fleets.id, fleets.workspace_id, 'steer', fleets.id || ':' || a,
       'digest', 'steer:api', 'chat', '{}', $2 + a, a || '-0',
       CASE WHEN a % 50 = 0 THEN NULL ELSE $2 + a END, 0, $2 + a, $2 + a
  FROM fleets, generate_series(1, $4) a";

/// A page smaller than what waits, so the walk must cross a page boundary.
const SMALL_PAGE: i64 = 2;

/// More waiting messages than [`SMALL_PAGE`] holds.
const WAITING: usize = 3;

fn uuid(text: &str) -> Uuid7 {
    Uuid7::parse(text).expect("the lane mints canonical identifiers")
}
