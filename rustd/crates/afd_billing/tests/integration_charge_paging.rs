//! Tenant charge paging bounds its setup-history work before detail joins.
#![cfg(feature = "test-util")]
#![expect(
    clippy::unwrap_used,
    reason = "live-schema test preconditions fail loudly"
)]

use afd_billing::sql::{CHARGE_TYPE_AUTHORING, posture};
use afd_db::test_util::{TestDatabase, mint_id};
use afd_db::{Db, DbRole, Migrator};
use jiff_sqlx::ToSqlx as _;
use serde_json::Value;
use sqlx::AssertSqlSafe;

#[path = "charge_paging/fixture.rs"]
mod fixture;

/// The first charges page, as the store runs it.
const FIRST: &str = include_str!("../src/sql/select_tenant_charges_first.sql");
/// A later charges page, as the store runs it.
const AFTER: &str = include_str!("../src/sql/select_tenant_charges_after.sql");

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
    let tenant = fixture::tenant();
    let foreign = fixture::tenant();
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
    let statement = boundary.map_or(FIRST, |_| AFTER);
    let text = AssertSqlSafe(format!("EXPLAIN (ANALYZE, FORMAT JSON) {statement}"));
    let mut query = sqlx::query_scalar::<_, Value>(text).bind(tenant);
    if let Some(second) = boundary {
        let at = jiff::Timestamp::from_second(second).unwrap();
        query = query.bind(at.to_sqlx()).bind(mint_id());
    }
    query
        .bind(i64::from(PAGE))
        .bind(CHARGE_TYPE_AUTHORING)
        .bind(posture::PLATFORM)
        .bind(posture::SELF_MANAGED)
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
async fn every_settlement_takes_its_tenant_from_its_attempt() {
    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;
    Migrator::new().run(&db).await.unwrap();
    let tenant = fixture::tenant();
    fixture::attempts(&db, &tenant, 1, 3).await;
    fixture::settlements(&db, &tenant, 1, 3).await;
    let counts: (i64, i64) = sqlx::query_as("SELECT count(*), count(*) FILTER (WHERE s.tenant_id=a.tenant_id) FROM billing.authoring_settlements s JOIN authoring.attempts a ON a.id=s.attempt_id WHERE a.tenant_id=$1::uuid")
        .bind(&tenant).fetch_one(&mut *db.acquire().await.unwrap()).await.unwrap();
    assert_eq!(
        counts,
        (3, 3),
        "a writer that names no tenant still files the attempt's"
    );
    db.close().await;
    database.cleanup().await;
}
