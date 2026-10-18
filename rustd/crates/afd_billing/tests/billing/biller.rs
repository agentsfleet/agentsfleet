//! The billing lifecycle: the conformance suite on both billers, the outage
//! policy, and the usage log's once-per-line rule.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::sync::Arc;

use afd_billing::biller::conformance::{self, BALANCE};
use afd_billing::biller::{
    Account, Attribution, Biller, Hold, OutagePolicy, RecordingBiller, Reservation, Settlement,
    WithOutagePolicy, WorkKey,
};
use afd_billing::ledger::{LedgerBiller, log_lines};
use afd_billing::limits::fleet_spend;
use afd_billing::price::RatedLine;
use afd_billing::{Nanos, Posture};
use afd_core::clock::SystemClock;
use afd_crypto::entropy::Entropy;
use afd_db::test_util::{mint_id, mint_uuid7, unreachable_db};
use afd_wire::usage::UsageLine;
use jiff::Timestamp;
use sqlx::Connection as _;

use crate::lane::Lane;

/// A line's charge in the usage-log checks.
const LINE: Nanos = Nanos::from_i64(25_000);

fn ledger(lane: &Lane) -> LedgerBiller {
    LedgerBiller::new(lane.pool.clone(), Entropy::new(), Arc::new(SystemClock))
}

async fn funded_lane() -> (Lane, Account) {
    let lane = Lane::create().await;
    lane.wallet(BALANCE.as_i64(), None, Timestamp::now()).await;
    let payer = Account {
        tenant: lane.tenant.clone(),
        workspace: lane.workspace.clone(),
    };
    (lane, payer)
}

fn rated(seq: u32) -> RatedLine {
    RatedLine {
        usage: UsageLine {
            seq,
            at: Timestamp::now(),
            input_tokens: 10,
            cached_input_tokens: 0,
            cache_write_tokens: 0,
            cache_write_5m_tokens: 0,
            cache_write_1h_tokens: 0,
            output_tokens: 5,
            runtime_ms: 0,
        },
        price: None,
        charged: LINE,
        unpriced_tokens: 0,
    }
}

fn settlement(lane: &Lane, payer: &Account, work: &WorkKey, lines: Vec<RatedLine>) -> Settlement {
    Settlement {
        work: work.clone(),
        payer: payer.clone(),
        attribution: Attribution::Fleet {
            fleet: lane.fleet.clone(),
            fleet_name: crate::lane::FLEET_NAME.to_owned(),
            event_id: mint_id(),
        },
        provider: crate::lane::PROVIDER.to_owned(),
        model: "biller-suite".to_owned(),
        posture: Posture::Platform,
        lines,
    }
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_every_biller_passes_the_conformance_suite() {
    let lane = Lane::create().await;
    let biller = ledger(&lane);
    conformance::every_check(&biller, || async { funded_lane().await.1 }).await;
}

#[tokio::test]
async fn the_recording_biller_passes_the_conformance_suite() {
    let biller = RecordingBiller::new();
    conformance::every_check(&biller, || {
        let payer = Account {
            tenant: mint_uuid7(),
            workspace: mint_uuid7(),
        };
        biller.fund(&payer.tenant, BALANCE);
        async move { payer }
    })
    .await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_concurrent_holds_cannot_exceed_balance() {
    let (lane, payer) = funded_lane().await;
    conformance::racing_holds_never_exceed_the_balance(&ledger(&lane), payer).await;
}

#[tokio::test]
async fn test_billing_outage_fails_open() {
    let ask = Reservation {
        work: WorkKey::FleetLease(mint_uuid7()),
        payer: Account {
            tenant: mint_uuid7(),
            workspace: mint_uuid7(),
        },
        ceiling: LINE,
    };
    let unreachable = || LedgerBiller::new(unreachable_db(), Entropy::new(), Arc::new(SystemClock));
    let open = WithOutagePolicy::new(unreachable(), OutagePolicy::Open);
    assert_eq!(
        open.reserve(&ask).await.expect("an open policy answers"),
        Hold::Unconfirmed
    );
    let closed = WithOutagePolicy::new(unreachable(), OutagePolicy::Closed);
    let refused = closed
        .reserve(&ask)
        .await
        .expect_err("a closed policy refuses");
    assert!(
        refused.is_datastore_unavailable(),
        "the refusal names the outage"
    );
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_usage_lines_deduplicate_on_sequence() {
    let (lane, payer) = funded_lane().await;
    let biller = ledger(&lane);
    let work = WorkKey::FleetLease(mint_uuid7());
    let first = settlement(&lane, &payer, &work, vec![rated(0)]);
    biller.settle(&first).await.expect("settles");
    biller.settle(&first).await.expect("settles again");
    let next = settlement(&lane, &payer, &work, vec![rated(0), rated(1)]);
    let receipt = biller.settle(&next).await.expect("settles a new line");
    assert_eq!(receipt.settled_lines, 1, "only the new sequence is charged");
    let rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM billing.usage_lines WHERE work_type = $1 AND work_id = $2::uuid",
    )
    .bind(work.work_type())
    .bind(work.id().as_str())
    .fetch_one(&mut *lane.pool.acquire().await.expect("the lane answers"))
    .await
    .expect("counts lines");
    assert_eq!(rows, 2, "each (work, seq) is one row");
    let balance = biller.balance(&payer).await.expect("balance answers");
    assert_eq!(
        balance.balance.as_i64(),
        BALANCE.as_i64() - LINE.as_i64() * 2
    );
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_unsettled_lines_are_resettled() {
    let (lane, payer) = funded_lane().await;
    let biller = ledger(&lane);
    let work = WorkKey::FleetLease(mint_uuid7());
    let lines = settlement(&lane, &payer, &work, vec![rated(0)]);
    // The work's transaction logs the line and commits; the settlement never runs.
    let mut connection = lane.pool.acquire().await.expect("the lane answers");
    let mut transaction = connection.begin().await.expect("a transaction opens");
    log_lines(&mut *transaction, &Entropy::new(), Timestamp::now(), &lines)
        .await
        .expect("logs the line");
    transaction.commit().await.expect("the work commits");
    drop(connection);
    let resettled = biller.settle(&lines).await.expect("the retry settles");
    assert_eq!(resettled.charged, LINE, "the logged line is charged once");
    let again = biller.settle(&lines).await.expect("a second retry answers");
    assert_eq!(again.charged, Nanos::ZERO, "a second pass charges nothing");
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_fleets_spend_is_the_sum_of_its_lines() {
    let (lane, payer) = funded_lane().await;
    let biller = ledger(&lane);
    let work = WorkKey::FleetLease(mint_uuid7());
    biller
        .settle(&settlement(
            &lane,
            &payer,
            &work,
            vec![rated(0), rated(1), rated(2)],
        ))
        .await
        .expect("settles");
    let spend = fleet_spend(
        &mut *lane.pool.acquire().await.expect("the lane answers"),
        &lane.workspace,
        &lane.fleet,
    )
    .await
    .expect("reads the spend");
    assert_eq!(
        spend.day.as_i64(),
        LINE.as_i64() * 3,
        "today's spend is every line"
    );
    assert_eq!(
        spend.month, spend.day,
        "this month's spend includes today's"
    );
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_settlement_past_the_balance_takes_it_all_and_marks_it_exhausted() {
    let (lane, payer) = funded_lane().await;
    let biller = ledger(&lane);
    conformance::a_short_settlement_writes_off_the_rest(&biller, payer.clone()).await;
    let balance = biller.balance(&payer).await.expect("balance answers");
    assert!(
        balance.exhausted_at.is_some(),
        "a spent balance records when it ran out"
    );
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

/// `exhausted_at` follows the balance through drain, top-up and drain again.
///
/// The debit is the one writer of the flag on the money path: it sets it when
/// a settlement empties the balance and clears it when one leaves the balance
/// positive, so a tenant topped up after running dry reads as exhausted only
/// until its next charge lands with credit to spare.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn exhaustion_follows_the_balance_through_a_top_up() {
    let lane = Lane::create().await;
    lane.wallet(LINE.as_i64(), None, Timestamp::now()).await;
    let payer = Account {
        tenant: lane.tenant.clone(),
        workspace: lane.workspace.clone(),
    };
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

/// Usage that happened settles even for a tenant with no wallet row: nothing
/// is taken and the whole charge is written off, so the line never sits
/// unsettled in front of the sweeper's oldest-first scan.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_tenant_without_a_wallet_settles_by_writing_the_charge_off() {
    let lane = Lane::create().await;
    let payer = Account {
        tenant: lane.tenant.clone(),
        workspace: lane.workspace.clone(),
    };
    let biller = ledger(&lane);
    let work = WorkKey::FleetLease(mint_uuid7());

    let receipt = biller
        .settle(&settlement(&lane, &payer, &work, vec![rated(0)]))
        .await
        .expect("a missing wallet still settles");
    assert_eq!(receipt.settled_lines, 1);
    assert_eq!(receipt.taken, Nanos::ZERO);
    assert_eq!(receipt.written_off, LINE);

    let again = biller
        .settle(&settlement(&lane, &payer, &work, vec![rated(0)]))
        .await
        .expect("a retry answers");
    assert_eq!(again.settled_lines, 0, "the line is settled once");
    biller
        .release(&work, afd_billing::biller::Release::Completed)
        .await
        .expect("a release without a wallet completes");
}
