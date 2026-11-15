//! The funds read reaches a tenant's unsettled lines through the partial
//! index `idx_usage_lines_tenant_id_unsettled`, and reads only those lines
//! however long the tenant's settled history grows.

use super::fixture::{self, Fixture, Line};
use super::plans::{explain, find, root};
use afd_billing::ledger;
use afd_db::test_util::TestDatabase;
use afd_db::{DbRole, Migrator};
use serde_json::Value;

const INDEX: &str = "idx_usage_lines_tenant_id_unsettled";
/// The tenant's settled history at each measurement.
const HISTORY: [u32; 4] = [1_024, 4_096, 16_384, 65_536];
/// Slices settled per pass: the relay settles a run as its lines arrive, never
/// a month of them in one statement.
const CHUNK: usize = HISTORY[0] as usize;
/// The tenant's lines the relay has not charged yet: a few seconds' worth.
const UNSETTLED: u32 = 8;
/// Other tenants with lines of their own: the relay's backlog spans them all.
const FOREIGN_TENANTS: u32 = 64;
/// Each other tenant's settled history.
const FOREIGN_HISTORY: u32 = 64;

#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn the_funds_read_scans_only_unsettled_lines_however_long_the_history() {
    // This proof owns ANALYZE statistics; it must not race the shared lane's writers.
    let database = TestDatabase::create().await;
    let migrator = database.open(DbRole::Migrator, &[]).await;
    Migrator::new().run(&migrator).await.unwrap();
    migrator.close().await;
    let f = Fixture::open(&database).await;
    let pending: Vec<_> = (0..UNSETTLED)
        .map(|_| Line::runtime(&f.tenant, &f.fleet, 1))
        .collect();
    fixture::seed(&f.admin, &pending).await;
    for _ in 0..FOREIGN_TENANTS {
        let other = fixture::id();
        fixture::fund(&f.admin, &other).await;
        let backlog: Vec<_> = (0..UNSETTLED).map(|_| Line::setup(&other, 1)).collect();
        fixture::seed(&f.admin, &backlog).await;
        fixture::record(&f, &history(&Line::setup(&other, 1), 0, FOREIGN_HISTORY)).await;
    }
    // One long run's slices, settled a chunk at a time as the relay meets them.
    let run = Line::runtime(&f.tenant, &f.fleet, 1);
    let mut previous = 0;
    for size in HISTORY {
        for from in (previous..size).step_by(CHUNK) {
            let until = size.min(from + HISTORY[0]);
            fixture::record(&f, &history(&run, from, until)).await;
        }
        // Settling updates a line out of the partial index; its old entry
        // goes when vacuum reclaims the old version, as autovacuum does.
        sqlx::query("VACUUM (ANALYZE) billing.usage_lines")
            .execute(&mut *f.admin.acquire().await.unwrap())
            .await
            .unwrap();
        assert_reads_unsettled(&f, size).await;
        previous = size;
    }
    f.close().await;
    database.cleanup().await;
}

/// Lines `from..until` of `first`'s work, each its own slice.
fn history(first: &Line, from: u32, until: u32) -> Vec<Line> {
    (from..until)
        .map(|sequence| first.next(first.kind, i64::from(sequence), 1))
        .collect()
}

async fn assert_reads_unsettled(f: &Fixture, history: u32) {
    let (plan,): (Value,) = ledger::balance_query(explain, &f.tenant)
        .fetch_one(&mut *f.api.acquire().await.unwrap())
        .await
        .unwrap();
    let scan = find(root(&plan), "Index Name", INDEX);
    assert!(scan.is_some(), "the funds read uses {INDEX}: {plan:#}");
    let scan = scan.unwrap();
    let condition = scan.get("Index Cond").and_then(Value::as_str).unwrap();
    assert!(condition.contains("tenant_id"), "{plan:#}");
    assert_eq!(
        scan.get("Actual Rows").and_then(Value::as_f64),
        Some(f64::from(UNSETTLED)),
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
    let blocks = |field| scan.get(field).and_then(Value::as_i64).unwrap_or(0);
    println!(
        "funds_read settled_history={history} unsettled={UNSETTLED} scanned_rows={UNSETTLED} \
         index={INDEX} node={} shared_hit={} shared_read={}",
        scan.get("Node Type").and_then(Value::as_str).unwrap_or("?"),
        blocks("Shared Hit Blocks"),
        blocks("Shared Read Blocks"),
    );
}
