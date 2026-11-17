//! The report reads the rollup through its unique keys, bounded by tenant and
//! UTC day: its scans return only the tenant's in-period rows however many
//! other tenants and older days the tables hold.

use super::fixture::{self, Fixture, Line};
use af_wire::usage::LineKind;
use afd_billing::report;
use afd_db::test_util::TestDatabase;
use afd_db::{DbRole, Migrator};
use futures_util::StreamExt as _;
use serde_json::Value;
use sqlx::AssertSqlSafe;

/// The tenant's in-period runs, each on a fleet of its own: one usage row and
/// one run row apiece.
const SIZES: [usize; 4] = [64, 128, 256, 512];
const USAGE_INDEX: &str = "uq_usage_daily_tenant_id_day_work_type_fleet_id_model";
const RUNS_INDEX: &str = "uq_runs_daily_tenant_id_day_work_type_fleet_id";
/// Another tenant's runs per tenant run: rows in both tables.
const FOREIGN_RUNS: usize = 16;
/// Another tenant's usage rows per tenant run, from one long run.
const FOREIGN_USAGE: usize = 64;
/// The tenant's usage rows before the period per in-period run.
const OLD_USAGE: usize = 4;
/// Slices in one foreign run: what one relay pass could settle together.
const RUN_SLICES: usize = 1_024;
/// Settlements in flight at once while the tables grow.
const SETTLING: usize = 16;

#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn report_reads_bound_rollup_scans_to_tenant_and_period() {
    // This proof owns ANALYZE statistics; it must not race the shared lane's writers.
    let database = TestDatabase::create().await;
    let migrator = database.open(DbRole::Migrator, &[]).await;
    Migrator::new().run(&migrator).await.unwrap();
    migrator.close().await;
    let f = Fixture::open(&database).await;
    let mut previous = 0;
    for size in SIZES {
        grow_by(&f, size - previous).await;
        sqlx::query("ANALYZE billing.usage_daily, billing.runs_daily")
            .execute(&mut *f.admin.acquire().await.unwrap())
            .await
            .unwrap();
        assert_scans(&f, size).await;
        previous = size;
    }
    f.close().await;
    database.cleanup().await;
}

/// Adds `runs` in-period runs for the tenant, sixteen times as many for the
/// other tenant, and wider usage rows elsewhere, all through the settle path.
async fn grow_by(f: &Fixture, runs: usize) {
    let mut lines: Vec<Line> = (0..runs)
        .map(|_| Line::runtime(&f.tenant, &fixture::id(), 1))
        .collect();
    lines.extend((0..runs * FOREIGN_RUNS).map(|_| Line::runtime(&f.foreign, &fixture::id(), 1)));
    // Long runs whose slices carry many fleet identifiers: many usage rows from
    // few settlements, which is all this proof needs of them. Each run holds
    // at most what one relay pass settles, never a month in one statement.
    let foreign: Vec<Vec<Line>> = (0..runs * FOREIGN_USAGE)
        .step_by(RUN_SLICES)
        .map(|from| {
            let slices = RUN_SLICES.min(runs * FOREIGN_USAGE - from);
            spread(&Line::runtime(&f.foreign, &fixture::id(), 1), slices)
        })
        .collect();
    let mut old = Line::runtime(&f.tenant, &fixture::id(), 1);
    old.at = fixture::BEFORE;
    let old = spread(&old, runs * OLD_USAGE);
    fixture::seed(&f.admin, &lines).await;
    fixture::seed(&f.admin, &foreign.concat()).await;
    fixture::seed(&f.admin, &old).await;
    let works = lines
        .iter()
        .chain(foreign.iter().filter_map(|run| run.first()))
        .chain(old.first())
        .map(Line::work);
    futures_util::stream::iter(works)
        .for_each_concurrent(SETTLING, |work| async move {
            fixture::settle(&f.biller, &f.admin, &work).await;
        })
        .await;
}

/// `count` runtime slices of `first`'s run, each on a fleet of its own.
fn spread(first: &Line, count: usize) -> Vec<Line> {
    (0..count)
        .map(|slice| {
            let mut line = first.next(LineKind::Runtime, i64::try_from(slice).unwrap(), 1);
            line.fleet = Some(fixture::id());
            line
        })
        .collect()
}

async fn assert_scans(f: &Fixture, size: usize) {
    let request = fixture::request();
    let (plan,): (Value,) = report::aggregates_query(explain, &f.tenant, &request)
        .fetch_one(&mut *f.api.acquire().await.unwrap())
        .await
        .unwrap();
    let root = root(&plan);
    for (table, index) in [("usage_daily", USAGE_INDEX), ("runs_daily", RUNS_INDEX)] {
        let probe = find(root, "Index Name", index);
        assert!(probe.is_some(), "{table} is read through {index}: {plan:#}");
        let probe = probe.unwrap();
        let condition = probe.get("Index Cond").and_then(Value::as_str).unwrap();
        assert!(
            condition.contains("tenant_id") && condition.contains("day"),
            "{plan:#}"
        );
        let scan = find(root, "Relation Name", table).unwrap();
        assert_eq!(
            scan.get("Actual Rows").and_then(Value::as_f64),
            Some(f64::from(u32::try_from(size).unwrap())),
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
        println!("report_query={table} scoped_rows={size} scanned_rows={size} index={index}");
    }
}

pub(super) fn explain(statement: &'static str) -> AssertSqlSafe<String> {
    AssertSqlSafe(format!(
        "EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {statement}"
    ))
}

pub(super) fn root(plan: &Value) -> &Value {
    plan.as_array()
        .and_then(|rows| rows.first())
        .and_then(|row| row.get("Plan"))
        .expect("EXPLAIN returns its first root plan")
}

pub(super) fn find<'a>(plan: &'a Value, field: &str, expected: &str) -> Option<&'a Value> {
    if plan.get(field).and_then(Value::as_str) == Some(expected) {
        return Some(plan);
    }
    plan.get("Plans")?
        .as_array()?
        .iter()
        .find_map(|child| find(child, field, expected))
}
