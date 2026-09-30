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

use afd_core::clock;
use afd_core::event::status;
use afd_core::id::Uuid7;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_db::{Db, Migrator};
use afd_dragonfly::streams::FleetStreams;
use afd_events::{History, QUEUED_READ_TEXT, Steer};
use sqlx::{AssertSqlSafe, Row as _};

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
      (id, workspace_id, tenant_id, name, source_markdown, config_json, status,
       created_at, updated_at)
    SELECT uuidv7(), workspace.id, $1::uuid, 'queued-plan-' || f, '# probe',
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

fn uuid(text: &str) -> Uuid7 {
    Uuid7::parse(text).expect("the lane mints canonical identifiers")
}

/// Dimension 2.1: an admitted, undelivered steer leads the first page as a
/// waiting row carrying its body, and a later page does not repeat it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_thread_read_includes_queued_steers() {
    let lane = EventsLane::open().await;
    let steered = Steer::new(lane.admissions())
        .append(&lane.fleet, &lane.workspace, ACTOR, BODY, None)
        .await
        .expect("the steer is admitted");
    let history = History::new(lane.database.clone());
    let (workspace, fleet) = (uuid(&lane.workspace), uuid(&lane.fleet));

    let page = history
        .thread_page(&workspace, &fleet, None, PAGE)
        .await
        .expect("the thread reads");
    let first = page.first().expect("the waiting message is on the page");
    assert_eq!(first.row.event_id, steered.event_id);
    assert_eq!(first.row.status, status::QUEUED);
    assert_eq!(first.row.actor, ACTOR);
    assert_eq!(first.request_json, BODY);
    assert_eq!(first.response_text, None);

    clean(&lane, &FleetStreams::new(lane.queue.clone())).await;
    lane.cleanup().await;
}

/// Dimension 2.2: once a runner has it, the event is one history row and no
/// longer waiting.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_thread_read_dedupes_leased_steer() {
    let lane = EventsLane::open().await;
    let steered = Steer::new(lane.admissions())
        .append(&lane.fleet, &lane.workspace, ACTOR, BODY, None)
        .await
        .expect("the steer is admitted");
    let admitted_at = steered
        .admitted_at
        .expect("a fresh admission has an instant");
    let mut connection = lane.connection().await;
    sqlx::query(MARK_DELIVERED)
        .bind(lane.fleet.as_str())
        .bind(clock::now().as_millis())
        .execute(&mut *connection)
        .await
        .expect("the lease's delivery stamp lands");
    drop(connection);
    lane.seed_event(&steered.event_id, admitted_at).await;

    let page = History::new(lane.database.clone())
        .thread_page(&uuid(&lane.workspace), &uuid(&lane.fleet), None, PAGE)
        .await
        .expect("the thread reads");
    let rows: Vec<_> = page
        .iter()
        .filter(|row| row.row.event_id == steered.event_id)
        .collect();
    assert_eq!(rows.len(), 1, "one event, one row");
    assert_ne!(
        rows.first().map(|row| row.row.status.as_str()),
        Some(status::QUEUED)
    );

    clean(&lane, &FleetStreams::new(lane.queue.clone())).await;
    lane.cleanup().await;
}

/// The waiting read probes an index of in-flight work alone under a generic
/// plan, over a ledger whose history is almost all delivered: the fleet is an
/// index condition, nothing is sorted, and no table is scanned whole.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_queued_read_plans_on_the_undelivered_index() {
    let database = TestDatabase::create().await;
    let db: Db = database.open(DbRole::Migrator, &[]).await;
    Migrator::new()
        .run(&db)
        .await
        .expect("the private database migrates");
    let tenant = mint_id();
    let mut connection = db.acquire().await.expect("a private connection");
    sqlx::query(
        "INSERT INTO core.tenants (id, name, created_at, updated_at) \
         VALUES ($1::uuid, 'queued-plan', 1, 1)",
    )
    .bind(tenant.as_str())
    .execute(&mut *connection)
    .await
    .expect("the tenant inserts");
    sqlx::query(SEED_LEDGER)
        .bind(tenant.as_str())
        .bind(1_700_000_000_000_i64)
        .bind(40_i32)
        .bind(500_i32)
        .execute(&mut *connection)
        .await
        .expect("the ledger history inserts");
    sqlx::query("ANALYZE core.fleet_admissions")
        .execute(&mut *connection)
        .await
        .expect("statistics refresh");

    // The simple protocol, because the placeholders stay unbound; the text is
    // this crate's own constant with a keyword in front of it.
    let plan = sqlx::raw_sql(AssertSqlSafe(format!(
        "EXPLAIN (GENERIC_PLAN) {QUEUED_READ_TEXT}"
    )))
    .fetch_all(&mut *connection)
    .await
    .expect("the waiting read explains")
    .iter()
    .map(|row| row.try_get::<String, _>(0).expect("a plan line is text"))
    .collect::<Vec<_>>()
    .join("\n");
    assert!(
        IN_FLIGHT_INDEXES.iter().any(|index| plan.contains(index)),
        "{plan}"
    );
    assert!(
        plan.lines()
            .any(|line| line.trim_start().starts_with("Index Cond:") && line.contains("fleet_id")),
        "the fleet bounds the scan: {plan}"
    );
    assert!(!plan.contains("Seq Scan"), "{plan}");
    assert!(
        !plan.contains("Sort"),
        "the order comes off the index: {plan}"
    );

    drop(connection);
    database.cleanup().await;
}
