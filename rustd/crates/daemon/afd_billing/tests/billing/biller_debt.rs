//! A balance past zero: the debt a settlement leaves, the checks it refuses,
//! the top-up that repays it, parallel work that spends past the balance
//! before its next check, and a payer with no wallet to debit at all.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_crypto::entropy::Entropy;
use afd_billing::Nanos;
use afd_billing::biller::conformance;
use afd_billing::biller::{Account, Ask, Biller, Funds, WorkKey};
use afd_billing::ledger::{LedgerBiller, log_lines, resettle};
use afd_db::test_util::mint_id;
use jiff::Timestamp;

use crate::biller::{LINE, funded_lane, ledger, rated, settlement};
use crate::lane::Lane;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_settlement_past_the_balance_leaves_a_debt() {
    let (lane, payer) = funded_lane().await;
    conformance::a_short_settlement_leaves_a_debt(&ledger(&lane), payer).await;
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
        work: WorkKey::FleetLease(mint_id()),
        payer: payer.clone(),
        next,
    }
}

/// Settles one line that drains the balance to zero, then one that takes it
/// a line below.
async fn drain_into_debt(biller: &dyn Biller, lane: &Lane, payer: &Account) {
    let work = WorkKey::FleetLease(mint_id());
    biller
        .settle(&settlement(lane, payer, &work, vec![rated(0)]))
        .await
        .expect("the first line drains the balance");
    let drained = biller.balance(payer).await.expect("answers");
    assert_eq!(drained.balance, Nanos::ZERO, "drained to exactly zero");
    assert!(drained.exhausted_at.is_some(), "a drained balance ran out");

    let receipt = biller
        .settle(&settlement(lane, payer, &work, vec![rated(1)]))
        .await
        .expect("a line past the balance settles");
    assert_eq!(receipt.taken, LINE, "the debit takes the whole line");
    let owed = biller.balance(payer).await.expect("answers");
    assert_eq!(
        owed.balance.as_i64(),
        -LINE.as_i64(),
        "the balance owes a line"
    );
    assert!(owed.exhausted_at.is_some(), "a debt stays exhausted");
}

/// A balance that settles past zero owes the rest, covers no check while it
/// does, and a top-up repays the debt before any of it is spendable.
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
/// a settlement empties the balance and clears it when one leaves the balance
/// positive, so a tenant topped up after running dry reads as exhausted only
/// until its next charge lands with credit to spare.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn exhaustion_follows_the_balance_through_a_top_up() {
    let (lane, payer) = one_line_lane().await;
    let biller = ledger(&lane);
    let work = WorkKey::FleetLease(mint_id());
    let exhausted = |balance: afd_billing::biller::Balance| balance.exhausted_at.is_some();

    biller
        .settle(&settlement(&lane, &payer, &work, vec![rated(0)]))
        .await
        .expect("the first line drains the balance");
    assert!(exhausted(biller.balance(&payer).await.expect("answers")));

    top_up(&lane, LINE.as_i64() * 2).await;
    biller
        .settle(&settlement(&lane, &payer, &work, vec![rated(1)]))
        .await
        .expect("a line after the top-up settles");
    assert!(
        !exhausted(biller.balance(&payer).await.expect("answers")),
        "a charge that leaves credit clears the flag"
    );

    biller
        .settle(&settlement(&lane, &payer, &work, vec![rated(2), rated(3)]))
        .await
        .expect("draining again settles");
    assert!(
        exhausted(biller.balance(&payer).await.expect("answers")),
        "running dry again sets it again"
    );
}

/// Usage that happened settles even for a tenant with no wallet row: there
/// is nothing to debit, so nothing is taken, and the line never sits
/// unsettled in front of the sweeper's oldest-first scan.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_tenant_without_a_wallet_settles_with_nothing_taken() {
    let lane = Lane::create().await;
    let payer = Account {
        tenant: lane.tenant.clone(),
        workspace: lane.workspace.clone(),
    };
    let biller = ledger(&lane);
    let work = WorkKey::FleetLease(mint_id());

    let capture = af_core::test_util::trace::Capture::install();
    let receipt = biller
        .settle(&settlement(&lane, &payer, &work, vec![rated(0)]))
        .await
        .expect("a missing wallet still settles");
    let warned = capture.only("billing_settled_without_wallet");
    assert_eq!(warned.field("tenant_id"), Some(payer.tenant.as_str()));
    assert_eq!(
        warned.field("work_type"),
        Some(afd_billing::sql::work::FLEET_LEASE)
    );
    assert_eq!(warned.field("work_id"), Some(work.id().as_str()));
    assert_eq!(
        warned.field("charged_nanos"),
        Some(LINE.as_i64().to_string().as_str())
    );
    drop(capture);
    assert_eq!(receipt.settled_lines, 1);
    assert_eq!(receipt.charged, LINE, "the line's cost is still recorded");
    assert_eq!(receipt.taken, Nanos::ZERO, "no wallet, nothing debited");

    let again = biller
        .settle(&settlement(&lane, &payer, &work, vec![rated(0)]))
        .await
        .expect("a retry answers");
    assert_eq!(again.settled_lines, 0, "the line is settled once");
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

/// Each work logs the one slice it spent before its next check.
async fn spend_a_slice_each(lane: &Lane, payer: &Account, works: &[WorkKey]) {
    for work in works {
        log_lines(
            &mut *lane.pool.acquire().await.expect("the lane answers"),
            &Entropy::new(),
            Timestamp::now(),
            &settlement(lane, payer, work, vec![rated(0)]),
        )
        .await
        .expect("each run logs the slice it spent");
    }
}

/// Ten runs start at the same moment against a balance that funds three of
/// their slices. Nothing is set aside, so every start check is covered. Each
/// run then spends one slice before its next check: the relay charges them
/// all, the balance ends seven slices below zero, and every next check is
/// refused until a grant repays the debt.
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
        .map(|_| WorkKey::FleetLease(mint_id()))
        .collect();

    let starts = start_together(&biller, &payer, &works).await;
    assert!(
        starts.iter().all(|funds| *funds == Funds::Covered),
        "nothing is set aside, so every run starts: {starts:?}"
    );
    spend_a_slice_each(&lane, &payer, &works).await;
    let first = works.first().expect("ten runs");
    let before_relay = biller.check(&slice(&payer, first)).await.expect("answers");
    assert!(
        matches!(before_relay, Funds::Short { .. }),
        "the logged slices count before the relay charges them: {before_relay:?}"
    );

    resettle(&lane.pool, &biller, None)
        .await
        .expect("the relay passes");
    let owed = biller.balance(&payer).await.expect("answers");
    let spent = LINE.as_i64() * i64::try_from(PARALLEL_RUNS).expect("ten fits");
    assert_eq!(
        owed.balance.as_i64(),
        start - spent,
        "every slice is charged once"
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
