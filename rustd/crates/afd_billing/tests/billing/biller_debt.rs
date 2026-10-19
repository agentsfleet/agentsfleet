//! A balance past zero: the debt a settlement leaves, the holds it refuses,
//! the top-up that repays it, and a payer with no wallet to debit at all.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_billing::Nanos;
use afd_billing::biller::conformance;
use afd_billing::biller::{Account, Biller, Hold, Release, Reservation, WorkKey};
use afd_db::test_util::mint_uuid7;
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
        "UPDATE billing.wallets SET balance_nanos = balance_nanos + $2 WHERE tenant_id = $1::uuid",
    )
    .bind(lane.tenant.as_str())
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

/// A hold of `ceiling` for a fresh piece of work.
fn ask(payer: &Account, ceiling: Nanos) -> Reservation {
    Reservation {
        work: WorkKey::FleetLease(mint_uuid7()),
        payer: payer.clone(),
        ceiling,
    }
}

/// Settles one line that drains the balance to zero, then one that takes it
/// a line below.
async fn drain_into_debt(biller: &dyn Biller, lane: &Lane, payer: &Account) {
    let work = WorkKey::FleetLease(mint_uuid7());
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

/// A balance that settles past zero owes the rest, refuses every hold while
/// it does, and a top-up repays the debt before any of it is spendable.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_debt_refuses_holds_until_a_top_up_repays_it() {
    let (lane, payer) = one_line_lane().await;
    let biller = ledger(&lane);
    drain_into_debt(&biller, &lane, &payer).await;

    let one = Nanos::from_i64(1);
    let refused = biller.reserve(&ask(&payer, one)).await.expect("answers");
    assert_eq!(
        refused,
        Hold::Short {
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
    let admitted = biller.reserve(&ask(&payer, LINE)).await.expect("answers");
    assert_eq!(
        admitted,
        Hold::Held {
            ceiling: LINE,
            captured: Nanos::ZERO
        },
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
    let work = WorkKey::FleetLease(mint_uuid7());
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
    let work = WorkKey::FleetLease(mint_uuid7());

    let capture = afd_core::test_util::trace::Capture::install();
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
    biller
        .release(&work, Release::Completed)
        .await
        .expect("a release without a wallet completes");
}
