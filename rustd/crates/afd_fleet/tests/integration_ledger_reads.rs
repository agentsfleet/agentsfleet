//! The two readers of a ledger row.
//!
//! One is the budget drain, which the renew path runs every few seconds of
//! every live run, through an index led by `fleet_id`. The claim worth pinning
//! is the PLAN, not the index definition: a definition test stays green
//! against a plan that stopped using it.
//!
//! The other is the charges page, which gained a column. A `SELECT` listing a
//! column the row struct does not read, or a struct reading one the statement
//! does not select, fails at runtime and nowhere earlier — `try_get` by name is
//! not checked at compile time.
//!
//! Marked `#[ignore]`; `make test-integration-rustd` runs them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_billing::sql::{SELECT_BUDGET_DRAIN, charge};
use afd_billing::tenant::{Billing, CHARGES_LIMIT_DEFAULT, cursor};
use afd_core::id::Uuid7;
use afd_db::test_util::mint_id;
use sqlx::Row as _;

use crate::queue;
use crate::report_seed;

use self::report_seed::{Held, held};

/// The index the drain must still be served by.
const FLEET_INDEX: &str = "idx_fleet_charges_fleet_id_workspace_id_updated_at";

/// How many decoy charges the plan probe seeds, and across how many fleets.
///
/// Enough that `workspace_id` alone stops being selective. One fleet per
/// workspace is a fixture artefact, not the production shape, and against it
/// the planner reasonably picks `idx_fleet_charges_workspace_id` — which says
/// nothing about whether the drain's own index survived.
const DECOY_CHARGES: i32 = 600;
/// The number of distinct fleets those charges are spread over.
const DECOY_FLEETS: i32 = 60;

/// Dimension 1.2. The budget drain still reaches its index.
///
/// The index led with `fleet_id` partly because the dropped foreign key's
/// `ON DELETE SET NULL` required it to, so an over-eager cleanup could read the
/// retired justification as a reason to drop the index. What is asserted is the
/// PLAN rather than the definition: a definition test stays green against an
/// index the planner has stopped choosing.
///
/// The probe seeds a realistic spread first. `enable_seqscan` is deliberately
/// NOT touched — priced-out alternatives would prove only that SOME index is
/// usable, and the first version of this test passed that way while the planner
/// was actually choosing `idx_fleet_charges_workspace_id`. With several fleets
/// in one workspace the composite index wins on cost, which is the production
/// condition and the only one where the assertion means anything.
///
/// Seeding rides a transaction that rolls back, so the lane's shared table is
/// unchanged. `ANALYZE` is the exception: its statistics are not transactional,
/// so the probe re-runs it afterwards rather than leaving the planner believing
/// in six hundred rows that no longer exist.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_m201_budget_drain_plan_unchanged() {
    let held = held().await;
    let workspace = workspace_of(&held).await;
    let mut connection = held
        .fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");

    sqlx::query("BEGIN")
        .execute(&mut *connection)
        .await
        .expect("the planner probe opens");
    seed_decoy_charges(&mut connection, &held, workspace.as_str()).await;

    // The statement is this workspace's own constant with a keyword in front
    // of it, never input — which is what `AssertSqlSafe` asks the caller to
    // say out loud.
    let explained = sqlx::query(sqlx::AssertSqlSafe(format!(
        "EXPLAIN {SELECT_BUDGET_DRAIN}"
    )))
    .bind(workspace.as_str())
    .bind(&held.fleet)
    .bind(afd_core::timing::DAY)
    .bind(charge::STAGE)
    .bind(charge::RECEIVE)
    .fetch_all(&mut *connection)
    .await
    .expect("the drain must be explainable");
    let plan: String = explained
        .iter()
        .map(|row| row.try_get::<String, _>(0).expect("a plan line is text"))
        .collect::<Vec<_>>()
        .join("\n");

    for statement in ["ROLLBACK", "ANALYZE billing.fleet_charges"] {
        sqlx::query(statement)
            .execute(&mut *connection)
            .await
            .expect("the planner probe cleans up after itself");
    }
    drop(connection);

    assert!(
        plan.contains(FLEET_INDEX),
        "the drain no longer reaches {FLEET_INDEX} — dropping the foreign key \
         took the index or the planner's reason to choose it:\n{plan}"
    );
    assert!(
        !plan.contains("Seq Scan on fleet_charges"),
        "the drain fell back to a sequential scan, which on this table grows \
         with a fleet's lifetime spend:\n{plan}"
    );

    queue::clear_ready(held.fixtures.queue(), &held.fleet).await;
    held.fixtures.cleanup().await;
}

/// Charges for many fleets in one workspace, so the drain's index has a reason.
///
/// The decoy fleet identifiers name no `fleet.fleets` row, which the column
/// permits: it is no foreign key, so a ledger row may name a fleet that does
/// not exist. Seeding this way needs no fleet rows,
/// no install, and no cleanup beyond the enclosing rollback.
async fn seed_decoy_charges(connection: &mut sqlx::PgConnection, held: &Held, workspace: &str) {
    sqlx::query(
        "INSERT INTO billing.fleet_charges
           (id, tenant_id, workspace_id, fleet_id, fleet_name, event_id,
            charge_type, posture, model, charged_nanos, event_created_at, created_at, updated_at)
         SELECT ('0199c900-0000-7000-8000-' || lpad(to_hex(g), 12, '0'))::uuid,
                $1::uuid, $2::uuid,
                ('0199c901-0000-7000-8000-' || lpad(to_hex(g % $3), 12, '0'))::uuid,
                'decoy', 'drain-probe-' || $2 || '-' || g, $4, 'platform', 'claude-opus-5',
                0, now(), now(), now()
         FROM generate_series(1, $5) g",
    )
    .bind(&held.tenant)
    .bind(workspace)
    .bind(DECOY_FLEETS)
    .bind(charge::RECEIVE)
    .bind(DECOY_CHARGES)
    .execute(&mut *connection)
    .await
    .expect("the decoy charges must insert");

    sqlx::query("ANALYZE billing.fleet_charges")
        .execute(&mut *connection)
        .await
        .expect("the planner must see the seeded spread");
}

/// Dimension 3.1. The charges page reads the captured name.
///
/// Through `Billing::charges` rather than against the statement text, because
/// the failure this guards is a mismatch BETWEEN the statement and the struct
/// that reads it, which no compiler catches.
///
/// Two names on one page: the fixture's receive charge carries the fleet's
/// own, and a second charge carries the one captured when it was written,
/// which a rename since leaves standing.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_m201_charge_row_decodes_fleet_name() {
    let held = held().await;
    let tenant = Uuid7::parse(&held.tenant).expect("the fixture tenant is a v7 spelling");
    seed_second_charge(&held).await;

    let page = Billing::new(held.fixtures.database.clone())
        .charges(&tenant, CHARGES_LIMIT_DEFAULT, None)
        .await
        .expect("a tenant's charges page must read");

    // Found by its event, not its fleet: both rows name the same fleet.
    let captured = page
        .iter()
        .find(|row| row.event_id == held.event_id)
        .expect("the fixture's receive charge must be on the page");
    assert_eq!(
        captured.fleet_name, held.fleet,
        "the fixture names a fleet after its own identifier"
    );
    assert_eq!(
        captured.fleet_id.as_str(),
        held.fleet.as_str(),
        "the identifier the dashboard derives a callsign from must survive the read"
    );
    let second = page
        .iter()
        .find(|row| row.event_id == second_event(&held))
        .expect("the hand-seeded charge must be on the page");
    assert_eq!(
        second.fleet_name, CAPTURED_NAME,
        "a charge keeps the name it captured, whatever the fleet is called now"
    );

    queue::clear_ready(held.fixtures.queue(), &held.fleet).await;
    held.fixtures.cleanup().await;
}

/// Dimension 3.1. The second page reads the new column too.
///
/// `Billing::charges` runs one of TWO statements depending on whether the
/// caller is resuming, and both select `fleet_name`. Every other
/// test here takes the first page, which leaves the resumed one — a separate
/// `SELECT` list, maintained by hand beside its twin — proven by nothing.
///
/// The failure it guards is invisible until a tenant has more charges than one
/// page holds: `ChargeRow::read` calls `try_get("fleet_name")` by name, so a
/// projection missing the column fails at RUNTIME, on page two, for the
/// heaviest users only. No compiler checks a column name against a struct.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_m201_charge_row_decodes_fleet_name_when_resuming() {
    let held = held().await;
    let tenant = Uuid7::parse(&held.tenant).expect("the fixture tenant is a v7 spelling");
    seed_second_charge(&held).await;

    let billing = Billing::new(held.fixtures.database.clone());
    // One row, so the fixture's own charges are guaranteed to sit beyond it.
    let first = billing
        .charges(&tenant, 1, None)
        .await
        .expect("the first page must read");
    let boundary = first
        .first()
        .map(|row| cursor::Boundary {
            created_at: row.created_at,
            id: row.id.clone(),
        })
        .expect("the tenant has charges, so the first page is not empty");

    let resumed = billing
        .charges(&tenant, CHARGES_LIMIT_DEFAULT, Some(&boundary))
        .await
        .expect("the resumed page must read — a projection missing fleet_name fails here");

    assert!(
        !resumed.is_empty(),
        "the fixture writes more than one charge, so resuming past the first \
         must still return rows — otherwise this proves nothing about the \
         statement it is meant to exercise"
    );
    assert!(
        resumed.iter().all(|row| row.id != boundary.id),
        "a resumed page must not repeat the row it resumed from"
    );

    queue::clear_ready(held.fixtures.queue(), &held.fleet).await;
    held.fixtures.cleanup().await;
}

/// The name the second charge captured, before a rename nobody replayed.
const CAPTURED_NAME: &str = "renamed-since";

/// The event id of the second charge.
fn second_event(held: &Held) -> String {
    format!("{}-second", held.event_id)
}

/// A second charge on the fixture's fleet, under the name it captured.
async fn seed_second_charge(held: &Held) {
    let workspace = workspace_of(held).await;
    let mut connection = held
        .fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO billing.fleet_charges
           (id, tenant_id, workspace_id, fleet_id, fleet_name, event_id,
            charge_type, posture, model, charged_nanos, event_created_at, created_at, updated_at)
         VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5, $6, $7, 'platform',
                 'claude-opus-5', 0, now(), now(), now())",
    )
    .bind(mint_id())
    .bind(&held.tenant)
    .bind(workspace.as_str())
    .bind(&held.fleet)
    .bind(CAPTURED_NAME)
    .bind(second_event(held))
    .bind(charge::RECEIVE)
    .execute(&mut *connection)
    .await
    .expect("the second charge must insert");
}

/// The workspace the fixture's fleet belongs to.
async fn workspace_of(held: &Held) -> Uuid7 {
    let mut connection = held
        .fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    let workspace: String =
        sqlx::query_scalar("SELECT workspace_id::text FROM fleet.fleets WHERE id = $1::uuid")
            .bind(&held.fleet)
            .fetch_one(&mut *connection)
            .await
            .expect("the fixture fleet must be readable");
    Uuid7::parse(&workspace).expect("the fixture workspace is a v7 spelling")
}
