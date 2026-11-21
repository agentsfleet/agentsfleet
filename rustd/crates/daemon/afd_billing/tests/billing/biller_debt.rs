//! A balance past zero: the debt a logged line leaves, the checks it refuses,
//! the top-up that repays it, parallel work that spends past the balance
//! before its next check, and a payer with no wallet to debit at all.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_billing::Nanos;
use afd_billing::biller::conformance;
use afd_billing::biller::{Account, Ask, Biller, Funds, WorkKey};
use afd_billing::ledger::LedgerBiller;
use afd_db::test_util::mint_id;
use jiff::Timestamp;

use crate::biller::{LINE, funded_lane, ledger, log, rated, settlement};
use crate::lane::Lane;

/// A line half again past the balance: the wallet takes the whole charge and
/// owes the rest, and the payer is short of every check until a grant repays it.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_line_past_the_balance_leaves_a_debt() {
    let (lane, payer) = funded_lane().await;
    let past = conformance::BALANCE.as_i64() + conformance::BALANCE.as_i64() / 2;
    let mut costly = rated(0);
    costly.charged = Nanos::from_i64(past);
    log(
        &lane,
        &settlement(&lane, &payer, &WorkKey::FleetRun(mint_id()), vec![costly]),
    )
    .await;
    let biller = ledger(&lane);
    let owed = biller.balance(&payer).await.expect("answers");
    assert_eq!(
        owed.balance.as_i64(),
        conformance::BALANCE.as_i64() - past,
        "the wallet owes what it could not cover"
    );
    conformance::a_payer_in_debt_is_short(&biller, payer).await;
}

/// Raises the lane's wallet by `nanos`, as a credit grant would.
async fn top_up(lane: &Lane, nanos: i64) {
    sqlx::query(
        "UPDATE billing.wallets SET balance_nanos = balance_nanos + $2 WHERE tenant_id = $1",
    )
    .bind(&lane.tenant)
    .bind(nanos)
    .execute(&mut *lane.pool.acquire().await.expect("the lane answers"))
    .await
    .expect("topping up the wallet");
}

/// A wallet funded with one line, and the payer it belongs to.
async fn one_line_lane() -> (Lane, Account) {
    let lane = Lane::create().await;
    lane.wallet(LINE.as_i64(), None, Timestamp::now()).await;
    let payer = Account {
        tenant: lane.tenant.clone(),
        workspace: lane.workspace.clone(),
    };
    (lane, payer)
}

/// A check of `next` for a fresh piece of work.
fn ask(payer: &Account, next: Nanos) -> Ask {
    Ask {
        work: WorkKey::FleetRun(mint_id()),
        payer: payer.clone(),
        next,
    }
}

/// Logs one line that drains the balance to zero, then one that takes it a
/// line below.
async fn drain_into_debt(biller: &dyn Biller, lane: &Lane, payer: &Account) {
    let work = WorkKey::FleetRun(mint_id());
    log(lane, &settlement(lane, payer, &work, vec![rated(0)])).await;
    let drained = biller.balance(payer).await.expect("answers");
    assert_eq!(drained.balance, Nanos::ZERO, "drained to exactly zero");
    assert!(drained.exhausted_at.is_some(), "a drained balance ran out");

    log(lane, &settlement(lane, payer, &work, vec![rated(1)])).await;
    let owed = biller.balance(payer).await.expect("answers");
    assert_eq!(
        owed.balance.as_i64(),
        -LINE.as_i64(),
        "the debit takes the whole line, and the balance owes it"
    );
    assert!(owed.exhausted_at.is_some(), "a debt stays exhausted");
}

/// A balance that is debited past zero owes the rest, covers no check while
/// it does, and a top-up repays the debt before any of it is spendable.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_debt_refuses_checks_until_a_top_up_repays_it() {
    let (lane, payer) = one_line_lane().await;
    let biller = ledger(&lane);
    drain_into_debt(&biller, &lane, &payer).await;

    let one = Nanos::from_i64(1);
    let refused = biller.check(&ask(&payer, one)).await.expect("answers");
    assert_eq!(
        refused,
        Funds::Short {
            available: Nanos::ZERO,
            asked: one
        },
        "a payer in debt is refused admission"
    );

    top_up(&lane, LINE.as_i64() * 2).await;
    let repaid = biller.balance(&payer).await.expect("answers");
    assert_eq!(
        repaid.balance, LINE,
        "the top-up repays the debt first and leaves the rest"
    );
    let admitted = biller.check(&ask(&payer, LINE)).await.expect("answers");
    assert_eq!(
        admitted,
        Funds::Covered,
        "a repaid balance admits work again"
    );
}

/// `exhausted_at` follows the balance through drain, top-up and drain again.
///
/// The debit is the one writer of the flag on the money path: it sets it when
/// a logged line empties the balance and clears it when one leaves the balance
/// positive, so a tenant topped up after running dry reads as exhausted only
/// until its next charge lands with credit to spare.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn exhaustion_follows_the_balance_through_a_top_up() {
    let (lane, payer) = one_line_lane().await;
    let biller = ledger(&lane);
    let work = WorkKey::FleetRun(mint_id());
    let exhausted = async || {
        biller
            .balance(&payer)
            .await
            .expect("answers")
            .exhausted_at
            .is_some()
    };
    let charge = async |lines| log(&lane, &settlement(&lane, &payer, &work, lines)).await;

    charge(vec![rated(0)]).await;
    assert!(
        exhausted().await,
        "the debit that empties the balance sets it"
    );
    charge(vec![rated(1)]).await;
    assert!(exhausted().await, "a debit while exhausted keeps it");

    top_up(&lane, LINE.as_i64() * 3).await;
    charge(vec![rated(2)]).await;
    assert!(
        !exhausted().await,
        "a charge that leaves credit clears the flag"
    );

    charge(vec![rated(3), rated(4)]).await;
    assert!(exhausted().await, "running dry again sets it again");
}

/// Usage that happened is logged even for a tenant with no wallet row: there
/// is nothing to debit, so nothing is taken, and the line is still recorded.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_tenant_without_a_wallet_logs_with_nothing_taken() {
    let lane = Lane::create().await;
    let payer = Account {
        tenant: lane.tenant.clone(),
        workspace: lane.workspace.clone(),
    };
    let work = WorkKey::FleetRun(mint_id());
    let logged = log(&lane, &settlement(&lane, &payer, &work, vec![rated(0)])).await;
    assert_eq!(logged, LINE, "the line's cost is still recorded");
    let wallets: i64 =
        sqlx::query_scalar("SELECT count(*) FROM billing.wallets WHERE tenant_id = $1")
            .bind(&lane.tenant)
            .fetch_one(&mut *lane.pool.acquire().await.expect("the lane answers"))
            .await
            .expect("counts wallets");
    assert_eq!(wallets, 0, "no wallet is opened to take the debit");
}

/// Ten runs a payer starts at once, on a balance that funds three slices.
const PARALLEL_RUNS: usize = 10;
/// The balance those ten runs start against: three slices' worth.
const THREE_SLICES: i64 = 3;

/// A check of one slice for `work`.
fn slice(payer: &Account, work: &WorkKey) -> Ask {
    Ask {
        work: work.clone(),
        payer: payer.clone(),
        next: LINE,
    }
}

/// Every work's start check, made at once.
async fn start_together(biller: &LedgerBiller, payer: &Account, works: &[WorkKey]) -> Vec<Funds> {
    futures_util::future::join_all(works.iter().map(|work| {
        let (biller, ask) = (biller.clone(), slice(payer, work));
        async move { biller.check(&ask).await.expect("the start check answers") }
    }))
    .await
}

/// Ten runs start at the same moment against a balance that funds three of
/// their slices. Nothing is set aside, so every start check is covered. Each
/// run then logs one slice before its next check, which debits it: the
/// balance ends seven slices below zero, and every next check is refused
/// until a grant repays the debt.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn parallel_runs_overdraw_by_at_most_a_slice_each_then_stop() {
    let lane = Lane::isolated().await;
    let start = LINE.as_i64() * THREE_SLICES;
    lane.wallet(start, None, Timestamp::now()).await;
    let payer = Account {
        tenant: lane.tenant.clone(),
        workspace: lane.workspace.clone(),
    };
    let biller = ledger(&lane);
    let works: Vec<WorkKey> = (0..PARALLEL_RUNS)
        .map(|_| WorkKey::FleetRun(mint_id()))
        .collect();

    let starts = start_together(&biller, &payer, &works).await;
    assert!(
        starts.iter().all(|funds| *funds == Funds::Covered),
        "nothing is set aside, so every run starts: {starts:?}"
    );
    for work in &works {
        log(&lane, &settlement(&lane, &payer, work, vec![rated(0)])).await;
    }
    let owed = biller.balance(&payer).await.expect("answers");
    let spent = LINE.as_i64() * i64::try_from(PARALLEL_RUNS).expect("ten fits");
    assert_eq!(
        owed.balance.as_i64(),
        start - spent,
        "every slice is charged once, as it is logged"
    );
    for work in &works {
        assert_eq!(
            biller.check(&slice(&payer, work)).await.expect("answers"),
            Funds::Short {
                available: Nanos::ZERO,
                asked: LINE
            },
            "every run is stopped at its next check"
        );
    }
    lane.cleanup().await;
}
