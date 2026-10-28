//! Tenant charge paging bounds its usage-line work before detail joins.
#![cfg(feature = "test-util")]
#![expect(
    clippy::unwrap_used,
    reason = "live-schema test preconditions fail loudly"
)]
#![expect(
    clippy::print_stdout,
    reason = "the lane prints its measured shape for the operator"
)]

use afd_billing::sql::CHARGE_TYPE_AUTHORING;
use afd_billing::tenant::cursor::Boundary;
use afd_billing::tenant::{self, Billing, ChargeRow};
use afd_core::id::Uuid7;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_db::{Db, DbRole, Migrator};
use jiff::Timestamp;
use serde_json::Value;
use sqlx::AssertSqlSafe;

#[path = "charge_paging/fixture.rs"]
mod fixture;

const PAGE: u32 = 10;
const HISTORY_SIZES: [i64; 4] = [128, 256, 512, 1024];
const PAGING_INDEX: &str = "idx_usage_lines_tenant_id_created_at_id";

#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn charge_pages_bound_usage_history_before_detail_joins() {
    // The plan proof owns schema statistics; concurrent tests must not change them.
    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;
    Migrator::new().run(&db).await.unwrap();
    let tenant = Uuid7::parse(&fixture::tenant()).unwrap();
    let foreign = fixture::tenant();
    let billing = Billing::new(db.clone());
    let mut previous = 0;
    for size in HISTORY_SIZES {
        for owner in [tenant.as_str(), &foreign] {
            fixture::attempts(&db, owner, previous + 1, size).await;
            fixture::lines(&db, owner, previous + 1, size).await;
        }
        sqlx::query("ANALYZE billing.usage_lines")
            .execute(&mut *db.acquire().await.unwrap())
            .await
            .unwrap();
        for boundary in [None, Some(boundary_at(size / 2))] {
            let plan = explain(&db, &tenant, boundary.as_ref()).await;
            let root = plan.get(0).unwrap().get("Plan").unwrap();
            assert_bounded_page(root, boundary.is_some());
            let rows = billing
                .charges(&tenant, PAGE, boundary.as_ref())
                .await
                .unwrap();
            assert_page_rows(&rows, &tenant, size, boundary.as_ref());
            println!("history={size} boundary={boundary:?} line_index_rows={PAGE}");
        }
        previous = size;
    }
    db.close().await;
    database.cleanup().await;
}

/// A cursor boundary at second `second`, its id breaking the tie within it.
fn boundary_at(second: i64) -> Boundary {
    Boundary {
        created_at: Timestamp::from_second(second).unwrap(),
        id: Uuid7::parse(&mint_id()).unwrap(),
    }
}

/// The plan of the page the store runs: its statement and its seven binds, run
/// through `EXPLAIN` by the store's own query builder.
async fn explain(db: &Db, tenant: &Uuid7, boundary: Option<&Boundary>) -> Value {
    let (plan,): (Value,) = tenant::charges_query(
        |statement| AssertSqlSafe(format!("EXPLAIN (ANALYZE, FORMAT JSON) {statement}")),
        tenant,
        PAGE,
        boundary,
    )
    .fetch_one(&mut *db.acquire().await.unwrap())
    .await
    .unwrap();
    plan
}

/// The page the store answers: full, the tenant's own, newest first, and
/// strictly older than the cursor when there is one.
fn assert_page_rows(rows: &[ChargeRow], tenant: &Uuid7, size: i64, boundary: Option<&Boundary>) {
    assert_eq!(rows.len(), usize::try_from(PAGE).unwrap(), "{rows:#?}");
    for row in rows {
        assert_eq!(row.tenant_id, tenant.as_str(), "{row:#?}");
        assert_eq!(row.charge_type, CHARGE_TYPE_AUTHORING, "{row:#?}");
    }
    for (newer, older) in rows.iter().zip(rows.iter().skip(1)) {
        assert!(
            (older.created_at, &older.id) < (newer.created_at, &newer.id),
            "newest first: {newer:#?} then {older:#?}"
        );
    }
    let newest = rows.first().unwrap();
    match boundary {
        None => assert_eq!(newest.created_at, Timestamp::from_second(size).unwrap()),
        Some(after) => assert!(
            (newest.created_at, newest.id.as_str()) < (after.created_at, after.id.as_str()),
            "strictly older than the cursor: {newest:#?} after {after:?}"
        ),
    }
}

fn assert_bounded_page(plan: &Value, after: bool) {
    let scan = line_scan(plan).unwrap();
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

fn line_scan(plan: &Value) -> Option<&Value> {
    if plan["Relation Name"] == "usage_lines" {
        return Some(plan);
    }
    plan.get("Plans")?.as_array()?.iter().find_map(line_scan)
}
