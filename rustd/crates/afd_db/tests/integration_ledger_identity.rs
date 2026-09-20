//! Slot 915's column, and what applying it does to a ledger that already holds
//! charges.
//!
//! The rest of this crate's migration suite asks whether the canonical list
//! applies cleanly to an empty database. Neither question this file asks can be
//! answered that way. A column's nullability and default are properties of the
//! APPLIED schema, not of the file that asks for them — `ADD COLUMN IF NOT
//! EXISTS` on a table that already has a differently-shaped `fleet_name` is a
//! no-op that reports success. And a fresh database has no rows for an upgrade
//! to damage, which is the only interesting thing an upgrade can do.
//!
//! So the second test builds the database every deployed instance actually is:
//! migrated to the slot BEFORE this one, populated, and only then upgraded.
//!
//! Marked `#[ignore]`; `make test-integration-rustd` runs them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_db::Migrator;
use afd_db::config::DbRole;
use afd_db::migration::MIGRATIONS;
use afd_db::test_util::TestDatabase;
use sqlx::Row as _;

/// The slot under test.
const LEDGER_IDENTITY: i32 = 915;

/// Identifiers the `uuidv7` CHECK on each table accepts.
///
/// Hand-written rather than minted, because a migration test is about the
/// database and borrowing an identifier generator would make it about that too.
/// The version nibble — the first character of the third group — is what every
/// `ck_*_id_uuidv7` constraint reads.
const TENANT: &str = "01990000-0000-7000-8000-000000000001";
/// The workspace the seeded charges belong to.
const WORKSPACE: &str = "01990000-0000-7000-8000-000000000002";
/// The fleet they are charged against, still alive at upgrade time.
const FLEET: &str = "01990000-0000-7000-8000-000000000003";
/// The two charges written before the upgrade.
const CHARGES: [&str; 2] = [
    "01990000-0000-7000-8000-000000000004",
    "01990000-0000-7000-8000-000000000005",
];

/// What the pre-upgrade rows are worth, in nanos.
///
/// Non-zero and distinct from any structural default, so a column silently
/// rewritten by the upgrade reads as zero rather than as itself.
const CHARGED_NANOS: i64 = 4_242;

/// An instant every timestamp column on the seeded rows carries.
const AT: i64 = 1_760_000_000_000;

/// Dimension 2.1. The column the surfaces read is the column the slot asked for.
///
/// Read from `information_schema` rather than by selecting the column, because
/// "it is there" is the weakest of the three claims. A `fleet_name` that arrived
/// NOT NULL would refuse every pre-915 row on the next write; one with a default
/// would fabricate a name a reader could not tell from a captured one.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_m201_ledger_carries_fleet_name_column() {
    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;
    Migrator::new()
        .run(&db)
        .await
        .expect("the canonical list applies to a fresh database");

    let column = sqlx::query(
        "SELECT data_type, is_nullable, column_default
         FROM information_schema.columns
         WHERE table_schema = 'billing'
           AND table_name = 'usage_ledger'
           AND column_name = 'fleet_name'",
    )
    .fetch_optional(&mut *db.acquire().await.expect("a pooled connection"))
    .await
    .expect("reading the column catalogue")
    .expect("billing.usage_ledger.fleet_name must exist after slot 915");

    assert_eq!(
        column
            .try_get::<String, _>("data_type")
            .expect("data_type decodes"),
        "text",
        "fleet_name must be text — a fleet's name is not a bounded vocabulary"
    );
    assert_eq!(
        column
            .try_get::<String, _>("is_nullable")
            .expect("is_nullable decodes"),
        "YES",
        "fleet_name must be nullable: every row written before slot 915 has no \
         name to carry, and a charge whose fleet row was unreadable still has \
         to be written"
    );
    assert_eq!(
        column
            .try_get::<Option<String>, _>("column_default")
            .expect("column_default decodes"),
        None,
        "fleet_name must have no default — there is no value that would be \
         true, and a placeholder is a fabricated name a reader cannot tell \
         from a captured one"
    );

    database.cleanup().await;
}

/// Dimension 2.1. Upgrading a populated ledger keeps every row and every value.
///
/// The slot does two things to a live table — it drops a foreign key by
/// catalogue lookup and adds a column — and the dangerous one is the lookup. A
/// predicate that matched too widely would drop the tenant or workspace
/// reference beside the fleet one; a rewrite of the table would be the kind of
/// migration that loses a charge the wallet was already debited for.
///
/// Migrating to 914 first is what makes this an UPGRADE rather than a fresh
/// install. Slicing the canonical list is safe because `MIGRATIONS` is static:
/// the sub-slice is the same data the migrator would have read, stopped early.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_m201_upgrade_from_populated_ledger() {
    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;

    let boundary = MIGRATIONS
        .iter()
        .position(|migration| migration.version() == LEDGER_IDENTITY)
        .expect("slot 915 is registered");
    // `get`, not a slice index: the boundary is derived from a `position` on
    // this same list, so it cannot be out of range — but the lane denies
    // `clippy::indexing_slicing`, and a test that panics on a slice bound
    // reports the panic rather than the schema fault it exists to catch.
    let head = MIGRATIONS
        .get(..boundary)
        .expect("the slot's own index is within its own list");
    let applied = Migrator::new()
        .with_migrations(head)
        .run(&db)
        .await
        .expect("the list up to slot 915 applies to a fresh database");
    assert!(
        !applied.applied.contains(&LEDGER_IDENTITY),
        "the pre-upgrade database must not already carry slot 915, or this \
         test seeds rows into the schema it is meant to be upgrading from"
    );

    seed_charges(&db).await;

    let upgrade = Migrator::new()
        .run(&db)
        .await
        .expect("slot 915 applies to a populated ledger");
    assert_eq!(
        upgrade.applied,
        vec![LEDGER_IDENTITY],
        "the upgrade must apply slot 915 and nothing else"
    );

    let mut connection = db.acquire().await.expect("a pooled connection");
    let rows = sqlx::query(
        "SELECT id::text, fleet_id::text, fleet_name, credit_deducted_nanos
         FROM billing.usage_ledger
         ORDER BY id",
    )
    .fetch_all(&mut *connection)
    .await
    .expect("reading the upgraded ledger");

    assert_eq!(
        rows.len(),
        CHARGES.len(),
        "the upgrade lost a charge — the wallet was already debited for it"
    );
    for (row, expected) in rows.iter().zip(CHARGES) {
        assert_eq!(
            row.try_get::<String, _>("id").expect("id decodes"),
            expected,
            "the upgraded rows are not the rows that were seeded"
        );
        assert_eq!(
            row.try_get::<Option<String>, _>("fleet_id")
                .expect("fleet_id decodes"),
            Some(FLEET.to_owned()),
            "the upgrade must leave an existing charge's fleet_id alone"
        );
        assert_eq!(
            row.try_get::<i64, _>("credit_deducted_nanos")
                .expect("credit_deducted_nanos decodes"),
            CHARGED_NANOS,
            "the upgrade must not rewrite what a charge was worth"
        );
        assert_eq!(
            row.try_get::<Option<String>, _>("fleet_name")
                .expect("fleet_name decodes"),
            None,
            "a row that predates the capture must read NULL — the slot \
             deliberately does not backfill, and a fabricated name here would \
             be indistinguishable from one the daemon actually captured"
        );
    }
    drop(connection);

    database.cleanup().await;
}

/// A tenant, a workspace, a live fleet, and two charges against it.
///
/// Written through the schema as it stands at slot 914, where `fleet_id` is
/// still a foreign key — so the fleet row is a precondition here, not decoration.
async fn seed_charges(db: &afd_db::Db) {
    let mut connection = db.acquire().await.expect("a pooled connection");

    sqlx::query(
        "INSERT INTO core.tenants (id, name, created_at, updated_at)
         VALUES ($1::uuid, 'fixture', $2, $2)",
    )
    .bind(TENANT)
    .bind(AT)
    .execute(&mut *connection)
    .await
    .expect("seeding a tenant");

    sqlx::query(
        "INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at)
         VALUES ($1::uuid, $2::uuid, 'fixture', 'fixture', $3)",
    )
    .bind(WORKSPACE)
    .bind(TENANT)
    .bind(AT)
    .execute(&mut *connection)
    .await
    .expect("seeding a workspace");

    sqlx::query(
        "INSERT INTO core.fleets
           (id, workspace_id, tenant_id, name, source_markdown, config_json,
            status, created_at, updated_at)
         VALUES ($1::uuid, $2::uuid, $3::uuid, 'deploy-bot', '# fixture',
                 '{}'::jsonb, 'installed', $4, $4)",
    )
    .bind(FLEET)
    .bind(WORKSPACE)
    .bind(TENANT)
    .bind(AT)
    .execute(&mut *connection)
    .await
    .expect("seeding a fleet");

    for charge in CHARGES {
        sqlx::query(
            "INSERT INTO billing.usage_ledger
               (id, tenant_id, workspace_id, fleet_id, event_id,
                charge_type, posture, model, credit_deducted_nanos,
                event_created_at, created_at, last_charged_at)
             VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $1, 'receive',
                     'platform', 'claude-opus-5', $5, $6, $6, $6)",
        )
        .bind(charge)
        .bind(TENANT)
        .bind(WORKSPACE)
        .bind(FLEET)
        .bind(CHARGED_NANOS)
        .bind(AT)
        .execute(&mut *connection)
        .await
        .expect("seeding a charge");
    }
}
