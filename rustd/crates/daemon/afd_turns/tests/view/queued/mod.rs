//! A fleet's thread shows a message that waits for a runner, and shows it once.
//!
//! A steer is admitted and put on the fleet's queue; no runner takes it. The
//! first page of the thread carries it as a `queued` row with its body. Once a
//! run stamps the turn's fence and opens its event, the same turn appears once,
//! as the history row. The waiting read is planned against a private database,
//! so its index is the one the statement was written for.
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
mod retry;
mod snapshot;

use af_core::id::Uuid7;
use af_core::turn::status;
use afd_turns::view::{Cursor, History, QUEUED_READ_TEXTS, Steer};

use crate::integration_list_plans::{explain_generic, private_database};
use crate::support::TurnsLane;

/// A person's actor, as a steer records it.
const ACTOR: &str = "steer:user_waiting";

/// What they typed, spelled as `jsonb` prints it back, so the stored body reads
/// back byte for byte.
const BODY: &str = r#"{"message": "wait for me"}"#;

/// A first page's size, as the dashboard asks for it.
const PAGE: i64 = 26;

/// What an assignment writes to the turn when a runner takes it: its fence.
const MARK_DELIVERED: &str = "UPDATE thread.turns SET run_fence = 1 \
     WHERE thread_id = $1 AND run_fence IS NULL";

/// The index the thread read walks, waiting turns and opened ones alike: the
/// thread's turns newest first (`afd_db/schema/20261108125100_thread_turns.sql`).
const IN_FLIGHT_INDEXES: [&str; 1] = ["idx_turns_thread_id_created_at_id"];

/// Ledger history, most of it delivered and settled, across many fleets of one workspace:
/// `$1` tenant, `$2` instant, `$3` fleets, `$4` turns per fleet. Each
/// turn is a millisecond after the one before it.
const SEED_LEDGER: &str = "
WITH workspace AS (
    INSERT INTO identity.workspaces (id, tenant_id, name, created_at)
    VALUES (uuidv7(), $1, 'queued-plan', $2::timestamptz)
    RETURNING id
), fleets AS (
    INSERT INTO fleet.fleets
      (id, workspace_id, tenant_id, name, status, required_tags,
       memory_reads_workspace, memory_publishes_workspace, resource_version,
       created_at, updated_at)
    SELECT uuidv7(), workspace.id, $1, 'queued-plan-' || f, 'active',
           '{}'::text[], false, false, 1, $2::timestamptz, $2::timestamptz
      FROM workspace, generate_series(1, $3) f
    RETURNING id, workspace_id
), threads AS (
    INSERT INTO thread.threads (id, fleet_id, fencing_token, run_expires_at, created_at, updated_at)
    SELECT id, id, 0, $2::timestamptz, $2::timestamptz, $2::timestamptz FROM fleets
)
INSERT INTO thread.turns
  (id, thread_id, workspace_id, producer, producer_key, payload_digest, actor,
   kind, status, request_json, run_fence,
   settled_at, created_at, updated_at)
SELECT uuidv7(), fleets.id, fleets.workspace_id, 'steer', fleets.id || ':' || a,
       'digest', 'steer:api', 'chat', 'queued', '{}',
       CASE WHEN a % 50 = 0 THEN NULL ELSE 1 END,
       CASE WHEN a % 50 = 0 THEN NULL
            ELSE $2::timestamptz + a * interval '1 millisecond' END,
       $2::timestamptz + a * interval '1 millisecond',
       $2::timestamptz + a * interval '1 millisecond'
  FROM fleets, generate_series(1, $4) a";

/// A page smaller than what waits, so the walk must cross a page boundary.
const SMALL_PAGE: i64 = 2;

/// More waiting messages than [`SMALL_PAGE`] holds.
const WAITING: usize = 3;

fn uuid(text: &str) -> Uuid7 {
    Uuid7::parse(text).expect("the lane mints canonical identifiers")
}
