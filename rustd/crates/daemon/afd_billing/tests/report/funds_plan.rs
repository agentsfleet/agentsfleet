//! The funds read is the wallet row alone: one row of `billing.wallets`, with
//! every logged line already in its balance and none of the tenant's usage
//! history read, however long that history grows.

use super::fixture::{self, Fixture, Line};
use super::plans::{explain, find, root};
use afd_billing::biller::{Account, Biller as _};
use afd_billing::ledger::{self, LedgerBiller};
use afd_db::test_util::TestDatabase;
use afd_db::{DbRole, Migrator};
use serde_json::Value;

/// The tenant's logged history at each measurement.
const HISTORY: [u32; 3] = [1_024, 4_096, 16_384];
/// Lines per write: a run logs its slices a renewal at a time, never a month
/// of them in one statement.
const CHUNK: u32 = HISTORY[0];

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn the_funds_check_reads_the_wallet() {
    // This proof owns ANALYZE statistics; it must not race the shared lane's writers.
    let database = TestDatabase::create().await;
    let migrator = database.open(DbRole::Migrator, &[]).await;
    Migrator::new().run(&migrator).await.unwrap();
    migrator.close().await;
    let f = Fixture::open(&database).await;
    let biller = LedgerBiller::new(f.api.clone());
    let payer = Account {
        tenant: f.tenant.clone(),
        workspace: f.tenant.clone(),
    };
    let run = Line::runtime(&f.tenant, &f.fleet, 1);
    let mut previous = 0;
    for size in HISTORY {
        for from in (previous..size).step_by(CHUNK as usize) {
            fixture::seed(&f.admin, &history(&run, from, size.min(from + CHUNK))).await;
        }
        sqlx::query("ANALYZE billing.wallets, billing.usage_lines")
            .execute(&mut *f.admin.acquire().await.unwrap())
            .await
            .unwrap();
        let balance = biller.balance(&payer).await.unwrap();
        assert_eq!(
            balance.balance.as_i64(),
            fixture::BALANCE - i64::from(size),
            "every logged line is already in the balance"
        );
        assert_reads_the_wallet(&f, size).await;
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

async fn assert_reads_the_wallet(f: &Fixture, history: u32) {
    let (plan,): (Value,) = ledger::balance_query(explain, &f.tenant)
        .fetch_one(&mut *f.api.acquire().await.unwrap())
        .await
        .unwrap();
    let root = root(&plan);
    assert!(
        find(root, "Relation Name", "usage_lines").is_none(),
        "the funds read reads no usage line: {plan:#}"
    );
    let scan = find(root, "Relation Name", "wallets");
    assert!(scan.is_some(), "the funds read reads the wallet: {plan:#}");
    let scan = scan.unwrap();
    assert_eq!(
        scan.get("Actual Rows").and_then(Value::as_f64),
        Some(1.0),
        "{plan:#}"
    );
    let blocks = |field| scan.get(field).and_then(Value::as_i64).unwrap_or(0);
    println!(
        "funds_read logged_history={history} scanned_rows=1 node={} shared_hit={} \
         shared_read={}",
        scan.get("Node Type").and_then(Value::as_str).unwrap_or("?"),
        blocks("Shared Hit Blocks"),
        blocks("Shared Read Blocks"),
    );
}
