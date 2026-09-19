//! The two readers of a ledger row, after slot 915 changed what is in it.
//!
//! One is the budget drain, which the renew path runs every few seconds of
//! every live run. Its index led with `fleet_id` partly because the dropped
//! foreign key's `ON DELETE SET NULL` required that column to lead, and an
//! over-eager cleanup could read the retired justification as a reason to drop
//! the index. The claim worth pinning is the PLAN, not the index definition: a
//! definition test stays green against a plan that stopped using it.
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
use sqlx::Row as _;

use crate::queue;
use crate::report_seed;

use self::report_seed::{Held, held};

/// The index the drain must still be served by.
const FLEET_INDEX: &str = "idx_usage_ledger_fleet_id_workspace_id_last_charged_at";

/// A window floor old enough to cover every fixture row.
const FLOOR_MS: i64 = 0;

/// How many decoy charges the plan probe seeds, and across how many fleets.
///
/// Enough that `workspace_id` alone stops being selective. One fleet per
/// workspace is a fixture artefact, not the production shape, and against it
/// the planner reasonably picks `idx_usage_ledger_workspace_id` — which says
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
/// was actually choosing `idx_usage_ledger_workspace_id`. With several fleets
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
    .bind(FLOOR_MS)
    .bind(FLOOR_MS)
    .bind(charge::RECEIVE)
    .bind(charge::STAGE)
    .fetch_all(&mut *connection)
    .await
    .expect("the drain must be explainable");
    let plan: String = explained
        .iter()
        .map(|row| row.try_get::<String, _>(0).expect("a plan line is text"))
        .collect::<Vec<_>>()
        .join("\n");

    for statement in ["ROLLBACK", "ANALYZE billing.usage_ledger"] {
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
        !plan.contains("Seq Scan on usage_ledger"),
        "the drain fell back to a sequential scan, which on this table grows \
         with a fleet's lifetime spend:\n{plan}"
    );

    queue::clear_ready(held.fixtures.queue(), &held.fleet).await;
    held.fixtures.cleanup().await;
}

/// Charges for many fleets in one workspace, so the drain's index has a reason.
///
/// The decoy fleet identifiers name no `core.fleets` row, which slot 915 is
/// what permits: the column stopped being a foreign key, so a ledger row may
/// now name a fleet that does not exist. Seeding this way needs no fleet rows,
/// no install, and no cleanup beyond the enclosing rollback.
async fn seed_decoy_charges(
    connection: &mut sqlx::PgConnection,
    held: &Held,
    workspace: &str,
) {
    sqlx::query(
        "INSERT INTO billing.usage_ledger
           (id, tenant_id, workspace_id, fleet_id, event_id,
            charge_type, posture, model, event_created_at, created_at, last_charged_at)
         SELECT ('0199c900-0000-7000-8000-' || lpad(to_hex(g), 12, '0'))::uuid,
                $1::uuid, $2::uuid,
                ('0199c901-0000-7000-8000-' || lpad(to_hex(g % $3), 12, '0'))::uuid,
                'drain-probe-' || $2 || '-' || g, $4, 'platform', 'claude-opus-5',
                $5, $5, $5
         FROM generate_series(1, $6) g",
    )
    .bind(&held.tenant)
    .bind(workspace)
    .bind(DECOY_FLEETS)
    .bind(charge::RECEIVE)
    .bind(held.now.as_millis())
    .bind(DECOY_CHARGES)
    .execute(&mut *connection)
    .await
    .expect("the decoy charges must insert");

    sqlx::query("ANALYZE billing.usage_ledger")
        .execute(&mut *connection)
        .await
        .expect("the planner must see the seeded spread");
}

/// Dimension 3.1. The charges page reads the new column, both ways.
///
/// Through `Billing::charges` rather than against the statement text, because
/// the failure this guards is a mismatch BETWEEN the statement and the struct
/// that reads it — and `try_get("fleet_name")` on a projection that does not
/// select it is a runtime error no compiler catches.
///
/// Both states in one page: the fixture's receive charge carries the name it
/// captured, and a hand-seeded row carries none, standing for every charge
/// written before slot 915. A reader that unwrapped the column would pass the
/// first and fail the second.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_m201_charge_row_decodes_fleet_name() {
    let held = held().await;
    let tenant = Uuid7::parse(&held.tenant).expect("the fixture tenant is a v7 spelling");
    seed_nameless_charge(&held).await;

    let page = Billing::new(held.fixtures.database.clone())
        .charges(&tenant, CHARGES_LIMIT_DEFAULT, None)
        .await
        .expect("a tenant's charges page must read");

    let captured = page
        .iter()
        .find(|row| row.fleet_id.as_deref() == Some(held.fleet.as_str()))
        .expect("the fixture's receive charge must be on the page");
    assert_eq!(
        captured.fleet_name.as_deref(),
        Some(held.fleet.as_str()),
        "a charge whose name was captured must decode to Some — the fixture \
         names a fleet after its own identifier"
    );
    assert_eq!(
        captured.fleet_id.as_deref(),
        Some(held.fleet.as_str()),
        "the identifier the dashboard derives a callsign from must survive the read"
    );

    let legacy = page
        .iter()
        .find(|row| row.event_id == nameless_event(&held))
        .expect("the hand-seeded pre-915 charge must be on the page");
    assert_eq!(
        legacy.fleet_name, None,
        "a charge with no captured name must decode to None rather than erroring \
         — every row written before slot 915 is in that state"
    );

    queue::clear_ready(held.fixtures.queue(), &held.fleet).await;
    held.fixtures.cleanup().await;
}

/// Dimension 3.1. The second page reads the new column too.
///
/// `Billing::charges` runs one of TWO statements depending on whether the
/// caller is resuming, and both gained `fleet_name` at slot 915. Every other
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
    seed_nameless_charge(&held).await;

    let billing = Billing::new(held.fixtures.database.clone());
    // One row, so the fixture's own charges are guaranteed to sit beyond it.
    let first = billing
        .charges(&tenant, 1, None)
        .await
        .expect("the first page must read");
    let boundary = first
        .first()
        .map(|row| cursor::Boundary {
            recorded_at: row.recorded_at,
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

/// The event id of the row standing in for a charge written before slot 915.
fn nameless_event(held: &Held) -> String {
    format!("{}-pre-915", held.event_id)
}

/// The identifier that row is written under.
///
/// Minted rather than fixed, because the lane's database is shared and a
/// constant would make two concurrent suites collide on the primary key.
fn nameless_row_id(held: &Held) -> Uuid7 {
    let mut bytes = [0u8; afd_core::id::ENTROPY_LEN];
    afd_crypto::entropy::Entropy::new()
        .fill(&mut bytes)
        .expect("the host draws entropy");
    Uuid7::encode(held.now, bytes).expect("a well-formed identifier")
}

/// A charge with neither identifier nor name, as a pre-915 purge left them.
async fn seed_nameless_charge(held: &Held) {
    let workspace = workspace_of(held).await;
    let mut connection = held
        .fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO billing.usage_ledger
           (id, tenant_id, workspace_id, fleet_id, event_id,
            charge_type, posture, model, event_created_at, created_at, last_charged_at)
         VALUES ($1::uuid, $2::uuid, $3::uuid, NULL, $4, $5, 'platform',
                 'claude-opus-5', $6, $6, $6)",
    )
    .bind(nameless_row_id(held).as_str())
    .bind(&held.tenant)
    .bind(workspace.as_str())
    .bind(nameless_event(held))
    .bind(charge::RECEIVE)
    .bind(held.now.as_millis())
    .execute(&mut *connection)
    .await
    .expect("the pre-915 charge must insert");
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
        sqlx::query("SELECT workspace_id::text FROM core.fleets WHERE id = $1::uuid")
            .bind(&held.fleet)
            .fetch_one(&mut *connection)
            .await
            .expect("the fixture fleet must be readable")
            .try_get(0)
            .expect("workspace_id decodes as text");
    Uuid7::parse(&workspace).expect("the fixture workspace is a v7 spelling")
}
