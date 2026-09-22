//! Slot 916's key: the arbiter that stops one fleet's charges landing on
//! another's row.
//!
//! Split from `integration_ledger_identity.rs`, which owns slot 915's column.
//! Two slots, two concerns, and the file was over the length cap with both.
//! The fixture rows are still seeded by that module's `seed_charges`, because
//! an upgrade test proves nothing about a table it populated differently from
//! the way the other upgrade test populates it.
//!
//! Marked `#[ignore]`; `make test-integration-rustd` runs them.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_db::Migrator;
use afd_db::config::DbRole;
use afd_db::migration::MIGRATIONS;
use afd_db::test_util::TestDatabase;
use sqlx::Row as _;

use crate::integration_ledger_identity::{CHARGES, seed_charges};

/// The slot that scopes the ledger's arbiter to the fleet.
const LEDGER_FLEET_SCOPED_KEY: i32 = 916;

/// The arbiter slot 916 installs, and the one it retires.
const FLEET_SCOPED_KEY: &str = "uq_usage_ledger_event_id_charge_type_fleet_id";
const RETIRED_KEY: &str = "uq_usage_ledger_event_id_charge_type";

/// The ledger's key and nullability, as the APPLIED schema holds them.
///
/// Both slot-916 tests ask the identical question of two differently-built
/// databases, and the whole point is that the answers agree: a fresh install
/// and an upgraded one must not diverge. One helper is how that stays true
/// when one of the two assertions is later tightened.
async fn assert_fleet_scoped_key(db: &afd_db::Db) {
    let mut connection = db.acquire().await.expect("a pooled connection");

    let constraints: Vec<(String, String)> = sqlx::query(
        "SELECT conname::text, pg_get_constraintdef(oid)::text
         FROM pg_constraint
         WHERE conrelid = 'billing.usage_ledger'::regclass AND contype = 'u'",
    )
    .fetch_all(&mut *connection)
    .await
    .expect("reading the constraint catalogue")
    .iter()
    .map(|row| {
        (
            row.try_get::<String, _>(0).expect("conname decodes"),
            row.try_get::<String, _>(1).expect("definition decodes"),
        )
    })
    .collect();

    let installed = constraints
        .iter()
        .find(|(name, _)| name == FLEET_SCOPED_KEY)
        .unwrap_or_else(|| {
            panic!("slot 916 must install {FLEET_SCOPED_KEY}: saw {constraints:#?}")
        });
    assert_eq!(
        installed.1, "UNIQUE (event_id, charge_type, fleet_id)",
        "the arbiter is event-led and the fleet comes LAST, deliberately: \
         leading with the fleet was tried and made the planner take this \
         index for the budget drain, demoting last_charged_at from an index \
         condition to a filter over every row a fleet was ever charged for. \
         Uniqueness is identical either way, so the order is chosen for the \
         read it tempts. Do not reorder to fix this failure"
    );
    assert!(
        !constraints.iter().any(|(name, _)| name == RETIRED_KEY),
        "{RETIRED_KEY} must be gone: leaving it would keep arbitrating a \
         charge by an event id two fleets can both hold"
    );

    let not_null: bool = sqlx::query(
        "SELECT attnotnull FROM pg_attribute
         WHERE attrelid = 'billing.usage_ledger'::regclass AND attname = 'fleet_id'",
    )
    .fetch_one(&mut *connection)
    .await
    .expect("reading the column catalogue")
    .try_get(0)
    .expect("attnotnull decodes");
    assert!(
        not_null,
        "fleet_id must be NOT NULL: Postgres treats NULLs as distinct in a \
         unique index, so a nullable column inside the arbiter is a hole in it"
    );
}

/// Dimension 2.1. A fresh install arbitrates a charge by its fleet.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn ledger_key_shape_on_fresh_bootstrap() {
    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;
    Migrator::new()
        .run(&db)
        .await
        .expect("the canonical list applies to a fresh database");

    assert_fleet_scoped_key(&db).await;

    database.cleanup().await;
}

/// Dimension 2.2. A populated ledger upgrades to the same shape, losing nothing.
///
/// The dangerous statement in slot 916 is `SET NOT NULL`, which rewrites no
/// data but reads all of it: a database holding one null fleet stops there. It
/// runs FIRST in the slot precisely so that a stop leaves both uniqueness
/// constraints intact rather than a half-swapped key. What this proves is the
/// other half — that a database whose rows are well-formed keeps every one of
/// them, and ends identical to a fresh install.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn ledger_key_shape_after_upgrade() {
    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;

    let boundary = MIGRATIONS
        .iter()
        .position(|migration| migration.version() == LEDGER_FLEET_SCOPED_KEY)
        .expect("slot 916 is registered");
    let head = MIGRATIONS
        .get(..boundary)
        .expect("the slot's own index is within its own list");
    Migrator::new()
        .with_migrations(head)
        .run(&db)
        .await
        .expect("the list up to slot 916 applies to a fresh database");

    seed_charges(&db).await;

    // Up to and INCLUDING 916, rather than the whole canonical list. The
    // subject is what slot 916 does to a populated ledger, and running
    // everything after it too would make the assertion below a statement about
    // whichever slot happens to be last — it was, and slot 917 broke it.
    let through = MIGRATIONS
        .get(..=boundary)
        .expect("the slot's own index is within its own list");
    let upgrade = Migrator::new()
        .with_migrations(through)
        .run(&db)
        .await
        .expect("slot 916 applies to a populated ledger");
    assert_eq!(
        upgrade.applied,
        vec![LEDGER_FLEET_SCOPED_KEY],
        "the upgrade must apply slot 916 and nothing else"
    );

    assert_fleet_scoped_key(&db).await;

    let surviving: i64 = sqlx::query("SELECT count(*) FROM billing.usage_ledger")
        .fetch_one(&mut *db.acquire().await.expect("a pooled connection"))
        .await
        .expect("counting the upgraded ledger")
        .try_get(0)
        .expect("count answers a bigint");
    assert_eq!(
        surviving,
        i64::try_from(CHARGES.len()).expect("two charges fit in an i64"),
        "the upgrade lost a charge — the wallet was already debited for it"
    );

    database.cleanup().await;
}
