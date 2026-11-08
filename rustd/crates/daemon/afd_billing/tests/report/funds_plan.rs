//! The funds read reaches a tenant's unsettled lines through the partial
//! index `idx_usage_lines_tenant_id_unsettled`, and reads only those lines
//! however long the tenant's settled history grows.

use super::fixture::{self, Fixture, Line};
use super::plans::{explain, find, root};
use afd_billing::ledger;
use afd_db::test_util::{TenantSeed, TestDatabase};
use afd_db::{DbRole, Migrator};
use serde_json::Value;

const INDEX: &str = "idx_usage_lines_tenant_id_unsettled";
/// The tenant's settled history at each measurement.
const HISTORY: [u32; 4] = [1_024, 4_096, 16_384, 65_536];
/// The tenant's lines the relay has not charged yet: a few seconds' worth.
const UNSETTLED: u32 = 8;
/// Other tenants with lines of their own: the relay's backlog spans them all.
const FOREIGN_TENANTS: u32 = 64;
/// Each other tenant's settled history.
const FOREIGN_HISTORY: u32 = 64;

/// The tenant's wallet, which the funds read starts from.
const INSERT_WALLET: &str = "\
INSERT INTO billing.wallets (tenant_id, balance_nanos, created_at, updated_at)
VALUES ($1::uuid, 0, now(), now())";

#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn the_funds_read_scans_only_unsettled_lines_however_long_the_history() {
    // This proof owns ANALYZE statistics; it must not race the shared lane's writers.
    let database = TestDatabase::create().await;
    let migrator = database.open(DbRole::Migrator, &[]).await;
    Migrator::new().run(&migrator).await.unwrap();
    migrator.close().await;
    let f = Fixture::open(&database).await;
    wallet(&f).await;
    let mut pending: Vec<_> = (0..UNSETTLED)
        .map(|_| unsettled(Line::runtime(&f.tenant, &f.fleet, 1)))
        .collect();
    for _ in 0..FOREIGN_TENANTS {
        let other = fixture::id();
        pending.extend((0..UNSETTLED).map(|_| unsettled(Line::setup(&other, 1))));
        pending.extend((0..FOREIGN_HISTORY).map(|_| Line::setup(&other, 1)));
    }
    fixture::seed(&f.admin, &pending).await;
    let mut previous = 0;
    for size in HISTORY {
        let history: Vec<_> = (previous..size)
            .map(|_| Line::runtime(&f.tenant, &f.fleet, 1))
            .collect();
        fixture::seed(&f.admin, &history).await;
        sqlx::query("ANALYZE billing.usage_lines")
            .execute(&mut *f.admin.acquire().await.unwrap())
            .await
            .unwrap();
        assert_reads_unsettled(&f, size).await;
        previous = size;
    }
    f.close().await;
    database.cleanup().await;
}

async fn wallet(f: &Fixture) {
    let mut connection = f.admin.acquire().await.unwrap();
    TenantSeed::new(f.tenant.as_str())
        .insert(&mut *connection)
        .await
        .unwrap();
    sqlx::query(INSERT_WALLET)
        .bind(f.tenant.as_str())
        .execute(&mut *connection)
        .await
        .unwrap();
}

const fn unsettled(mut line: Line) -> Line {
    line.settled = None;
    line
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
