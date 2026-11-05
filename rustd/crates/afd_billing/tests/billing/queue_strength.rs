//! Many tenants, many works at once, through the billing queue and around it.
//!
//! The money must come out exact either way: every wallet down by exactly the
//! lines it settled, nothing still held, and a redelivered settlement charging
//! nothing. What differs is how long a caller waits; the test measures that and
//! prints it, and asserts only the money.
#![expect(
    clippy::expect_used,
    clippy::print_stdout,
    reason = "test target: a failed precondition fails loudly, and the measurement is its output"
)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use afd_billing::Nanos;
use afd_billing::biller::{Account, Biller, Queued, Release, Reservation, WorkKey};
use afd_db::test_util::mint_uuid7;
use jiff::Timestamp;

use crate::biller::{LINE, ledger, rated, settlement};
use crate::lane::Lane;

const TENANTS: usize = 8;
const WORKS_PER_TENANT: u16 = 16;
const LINES_PER_WORK: u32 = 2;
const DEPOSIT: i64 = 1_000_000_000;
const DRAIN_LIMIT: Duration = Duration::from_secs(60);

struct Tenant {
    lane: Lane,
    payer: Account,
}

async fn tenants() -> Vec<Arc<Tenant>> {
    let mut made = Vec::with_capacity(TENANTS);
    for _ in 0..TENANTS {
        let lane = Lane::create().await;
        lane.wallet(DEPOSIT, None, Timestamp::now()).await;
        let payer = Account {
            tenant: lane.tenant.clone(),
            workspace: lane.workspace.clone(),
        };
        made.push(Arc::new(Tenant { lane, payer }));
    }
    made
}

/// Every work's caller-side time: hold at claim, settle, release, and for every
/// other work a redelivered settlement.
async fn run(biller: Arc<dyn Biller>, tenants: &[Arc<Tenant>]) -> Vec<Duration> {
    let mut works = Vec::new();
    for tenant in tenants {
        for index in 0..WORKS_PER_TENANT {
            let biller = Arc::clone(&biller);
            let tenant = Arc::clone(tenant);
            works.push(tokio::spawn(async move {
                let work = WorkKey::FleetLease(mint_uuid7());
                let lines = (1..=LINES_PER_WORK).map(rated).collect::<Vec<_>>();
                let charge = settlement(&tenant.lane, &tenant.payer, &work, lines);
                let ask = Reservation {
                    work: work.clone(),
                    payer: tenant.payer.clone(),
                    ceiling: Nanos::from_i64(LINE.as_i64() * i64::from(LINES_PER_WORK)),
                };
                let started = Instant::now();
                biller.reserve_detached(&ask).await.expect("hold answers");
                biller.settle(&charge).await.expect("settle answers");
                if index % 2 == 0 {
                    biller
                        .settle(&charge)
                        .await
                        .expect("redelivered settle answers");
                }
                biller
                    .release(&work, Release::Completed)
                    .await
                    .expect("release answers");
                started.elapsed()
            }));
        }
    }
    let mut waited = Vec::with_capacity(works.len());
    for work in works {
        waited.push(work.await.expect("work task"));
    }
    waited
}

/// Waits until every wallet shows exactly its settled lines and no hold.
async fn drained(tenants: &[Arc<Tenant>]) -> Duration {
    let started = Instant::now();
    let charged = LINE.as_i64() * i64::from(LINES_PER_WORK) * i64::from(WORKS_PER_TENANT);
    loop {
        let mut settled = true;
        for tenant in tenants {
            let balance = ledger(&tenant.lane)
                .balance(&tenant.payer)
                .await
                .expect("balance answers");
            settled &= balance.balance.as_i64() == DEPOSIT - charged && balance.held.as_i64() == 0;
        }
        if settled {
            return started.elapsed();
        }
        assert!(
            started.elapsed() < DRAIN_LIMIT,
            "the queue did not settle every wallet exactly"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn percentile(sorted: &[Duration], at: usize) -> Duration {
    let index = (sorted.len() * at / 100).min(sorted.len().saturating_sub(1));
    sorted.get(index).copied().unwrap_or_default()
}

fn summary(label: &str, mut waited: Vec<Duration>, drain: Duration) {
    waited.sort();
    println!(
        "{label}: works={} caller p50={:?} p95={:?} p99={:?} max={:?} drain={drain:?}",
        waited.len(),
        percentile(&waited, 50),
        percentile(&waited, 95),
        percentile(&waited, 99),
        waited.last().copied().unwrap_or_default(),
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "requires live Postgres"]
async fn many_tenants_settle_exactly_once_with_or_without_the_queue() {
    let direct_tenants = tenants().await;
    let first = direct_tenants.first().expect("a tenant");
    let direct: Arc<dyn Biller> = Arc::new(ledger(&first.lane));
    let direct_waited = run(direct, &direct_tenants).await;
    let direct_drain = drained(&direct_tenants).await;

    let queued_tenants = tenants().await;
    let queued: Arc<dyn Biller> = Arc::new(Queued::in_memory(Arc::new(ledger(
        &queued_tenants.first().expect("a tenant").lane,
    ))));
    let queued_waited = run(queued, &queued_tenants).await;
    let queued_drain = drained(&queued_tenants).await;

    summary("direct", direct_waited, direct_drain);
    summary("queued", queued_waited, queued_drain);
}
