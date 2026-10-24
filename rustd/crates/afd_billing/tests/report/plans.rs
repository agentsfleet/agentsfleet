use super::fixture::{self, Fixture, Line};
use afd_billing::biller::WorkKey;
use afd_billing::report;
use afd_db::test_util::TestDatabase;
use afd_db::{DbRole, Migrator};
use serde_json::Value;
use sqlx::AssertSqlSafe;

const SIZES: [u32; 4] = [512, 1024, 2048, 4096];
const PAGE: u32 = 10;
const INDEX: &str = "idx_usage_lines_tenant_id_created_at_id";
const DETAIL_INDEX: &str = "uq_usage_lines_work_type_work_id_seq";
const FOREIGN_MULTIPLIER: usize = 64;
const OLD_MULTIPLIER: usize = 4;

#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn report_queries_bound_scans_to_tenant_and_period() {
    // This proof owns ANALYZE statistics; it must not race the shared lane's writers.
    let database = TestDatabase::create().await;
    let migrator = database.open(DbRole::Migrator, &[]).await;
    Migrator::new().run(&migrator).await.unwrap();
    migrator.close().await;
    let f = Fixture::open(&database).await;
    let mut previous = 0;
    for size in SIZES {
        seed_growth(&f, size - previous).await;
        sqlx::query("ANALYZE billing.usage_lines")
            .execute(&mut *f.admin.acquire().await.unwrap())
            .await
            .unwrap();
        assert_scans(&f, size).await;
        previous = size;
    }
    assert_detail_plan(&f).await;
    f.close().await;
    database.cleanup().await;
}

async fn seed_growth(f: &Fixture, growth: u32) {
    let growth = usize::try_from(growth).unwrap();
    let mut rows: Vec<_> = (0..growth)
        .map(|_| Line::runtime(&f.tenant, &f.fleet, 1))
        .collect();
    rows.extend((0..growth * FOREIGN_MULTIPLIER).map(|_| Line::setup(&f.foreign, 999)));
    rows.extend((0..growth * OLD_MULTIPLIER).map(|_| {
        let mut row = Line::setup(&f.tenant, 888);
        row.at = fixture::BEFORE;
        row.settled = Some(fixture::BEFORE);
        row
    }));
    fixture::seed(&f.admin, &rows).await;
}

async fn assert_scans(f: &Fixture, size: u32) {
    let request = fixture::request(PAGE);
    let mut connection = f.api.acquire().await.unwrap();
    let (aggregate,): (Value,) = report::aggregates_query(explain, &f.tenant, &request)
        .fetch_one(&mut *connection)
        .await
        .unwrap();
    let (activities,): (Value,) = report::activities_query(explain, &f.tenant, &request)
        .fetch_one(&mut *connection)
        .await
        .unwrap();
    for (kind, plan) in [("totals", aggregate), ("activities", activities)] {
        let root = root(&plan);
        let index = find(root, "Index Name", INDEX).expect("report uses the tenant/time index");
        let condition = index.get("Index Cond").and_then(Value::as_str).unwrap();
        assert!(
            condition.contains("tenant_id") && condition.contains("created_at"),
            "{plan:#}"
        );
        let scan = find(root, "Relation Name", "usage_lines").unwrap();
        assert_eq!(
            scan.get("Actual Rows").and_then(Value::as_f64),
            Some(f64::from(size)),
            "{plan:#}"
        );
        assert_eq!(
            scan.get("Actual Loops").and_then(Value::as_f64),
            Some(1.0),
            "{plan:#}"
        );
        assert_eq!(
            scan.get("Rows Removed by Filter")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
            0.0,
            "{plan:#}"
        );
        println!("report_query={kind} scoped_rows={size} scanned_rows={size} index={INDEX}");
    }
}

async fn assert_detail_plan(f: &Fixture) {
    let request = fixture::request(PAGE);
    let report = f.billing.report(&f.tenant, &request).await.unwrap();
    let activity = report.activities.first().expect("a seeded report activity");
    let work = WorkKey::FleetLease(afd_core::id::Uuid7::parse(&activity.work_id).unwrap());
    let (plan,): (Value,) = report::charges_query(explain, &f.tenant, &work, &request, None)
        .fetch_one(&mut *f.api.acquire().await.unwrap())
        .await
        .unwrap();
    let index = find(root(&plan), "Index Name", DETAIL_INDEX)
        .expect("detail query uses the work/sequence index");
    let condition = index.get("Index Cond").and_then(Value::as_str).unwrap();
    assert!(
        condition.contains("work_type") && condition.contains("work_id"),
        "{plan:#}"
    );
    assert_eq!(
        index.get("Actual Rows").and_then(Value::as_f64),
        Some(1.0),
        "{plan:#}"
    );
    println!("report_query=details scanned_rows=1 index={DETAIL_INDEX}");
}

fn explain(statement: &'static str) -> AssertSqlSafe<String> {
    AssertSqlSafe(format!("EXPLAIN (ANALYZE, FORMAT JSON) {statement}"))
}

fn root(plan: &Value) -> &Value {
    plan.as_array()
        .and_then(|rows| rows.first())
        .and_then(|row| row.get("Plan"))
        .expect("EXPLAIN returns its first root plan")
}

fn find<'a>(plan: &'a Value, field: &str, expected: &str) -> Option<&'a Value> {
    if plan.get(field).and_then(Value::as_str) == Some(expected) {
        return Some(plan);
    }
    plan.get("Plans")?
        .as_array()?
        .iter()
        .find_map(|child| find(child, field, expected))
}
