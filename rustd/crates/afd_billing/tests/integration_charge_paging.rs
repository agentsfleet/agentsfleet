//! Tenant charge paging work bounds and live-schema accounting preservation.
#![cfg(feature = "test-util")]
#![expect(
    clippy::unwrap_used,
    reason = "live-schema test preconditions fail loudly"
)]

use afd_db::test_util::{TestDatabase, mint_id};
use afd_db::{Db, DbRole, Migrator};
use serde_json::Value;
use sqlx::AssertSqlSafe;

use tenant_sql::{SELECT_TENANT_CHARGES_PAGE_AFTER, SELECT_TENANT_CHARGES_PAGE_FIRST};

#[path = "charge_paging/fixture.rs"]
mod fixture;
#[path = "../src/tenant_sql.rs"]
#[expect(
    dead_code,
    reason = "the wallet statement is outside this paging plan proof"
)]
mod tenant_sql;

const PAGE: i32 = 10;
const HISTORY_SIZES: [i64; 4] = [128, 256, 512, 1024];
const PAGING_INDEX: &str = "idx_authoring_settlements_tenant_id_created_at_id";

#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn charge_pages_bound_setup_history_before_detail_joins() {
    // The plan proof owns schema statistics; concurrent tests must not change them.
    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;
    Migrator::new().run(&db).await.unwrap();
    let tenant = fixture::seed(&db).await;
    let foreign = fixture::seed(&db).await;
    let mut previous = 0;
    for size in HISTORY_SIZES {
        for owner in [&tenant, &foreign] {
            fixture::attempts(&db, owner, previous + 1, size).await;
            fixture::settlements(&db, owner, previous + 1, size).await;
        }
        sqlx::query("ANALYZE billing.authoring_settlements")
            .execute(&mut *db.acquire().await.unwrap())
            .await
            .unwrap();
        for boundary in [None, Some(size / 2)] {
            let plan = explain(&db, &tenant, boundary).await;
            let root = plan.get(0).unwrap().get("Plan").unwrap();
            assert_bounded_setup(root, boundary.is_some());
            println!("history={size} boundary={boundary:?} setup_index_rows={PAGE}");
        }
        previous = size;
    }
    db.close().await;
    database.cleanup().await;
}

async fn explain(db: &Db, tenant: &str, boundary: Option<i64>) -> Value {
    let statement = boundary.map_or(SELECT_TENANT_CHARGES_PAGE_FIRST, |_| {
        SELECT_TENANT_CHARGES_PAGE_AFTER
    });
    let text = AssertSqlSafe(format!("EXPLAIN (ANALYZE, FORMAT JSON) {statement}"));
    let mut query = sqlx::query_scalar::<_, Value>(text).bind(tenant);
    if let Some(time) = boundary {
        query = query.bind(time).bind(mint_id());
    }
    query
        .bind(PAGE)
        .fetch_one(&mut *db.acquire().await.unwrap())
        .await
        .unwrap()
}

fn assert_bounded_setup(plan: &Value, after: bool) {
    let scan = setup_scan(plan).unwrap();
    assert_eq!(scan["Index Name"], PAGING_INDEX, "{plan:#}");
    let condition = scan["Index Cond"].as_str().unwrap();
    assert!(condition.contains("tenant_id"), "{plan:#}");
    if after {
        assert!(condition.contains("created_at"), "{plan:#}");
    }
    assert_eq!(
        scan["Actual Rows"].as_f64(),
        Some(f64::from(PAGE)),
        "{plan:#}"
    );
    assert_eq!(scan["Actual Loops"].as_f64(), Some(1.0), "{plan:#}");
    assert_eq!(
        scan.get("Rows Removed by Filter")
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
        0.0,
        "{plan:#}"
    );
}

fn setup_scan(plan: &Value) -> Option<&Value> {
    if plan["Relation Name"] == "authoring_settlements" {
        return Some(plan);
    }
    plan.get("Plans")?.as_array()?.iter().find_map(setup_scan)
}

#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn paging_migration_preserves_charges_and_derives_each_settlement_tenant() {
    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;
    Migrator::new()
        .with_migrations(fixture::through(957))
        .run(&db)
        .await
        .unwrap();
    let tenant = fixture::seed(&db).await;
    fixture::attempts(&db, &tenant, 1, 3).await;
    fixture::settlements(&db, &tenant, 1, 2).await;
    let before = fixture::snapshot(&db, &tenant).await;
    assert_eq!(Migrator::new().run(&db).await.unwrap().applied, [958]);
    assert_eq!(fixture::snapshot(&db, &tenant).await, before);
    assert_tenant(&db, &tenant, 2).await;
    fixture::settlements(&db, &tenant, 3, 3).await;
    assert_tenant(&db, &tenant, 3).await;
    assert_eq!(
        Migrator::new().run(&db).await.unwrap().applied,
        [] as [i32; 0]
    );
    db.close().await;
    database.cleanup().await;
}

async fn assert_tenant(db: &Db, tenant: &str, expected: i64) {
    let counts: (i64, i64) = sqlx::query_as("SELECT count(*), count(*) FILTER (WHERE s.tenant_id=a.tenant_id) FROM billing.authoring_settlements s JOIN fleet.authoring_attempts a ON a.id=s.attempt_id WHERE a.tenant_id=$1::uuid")
        .bind(tenant).fetch_one(&mut *db.acquire().await.unwrap()).await.unwrap();
    assert_eq!(counts, (expected, expected));
}
