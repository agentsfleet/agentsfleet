//! The billing lifecycle: the conformance suite on both billers, the outage
//! policy, and the usage log's once-per-line rule.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::sync::Arc;

use af_core::clock::SystemClock;
use af_crypto::entropy::Entropy;
use af_wire::usage::UsageLine;
use afd_billing::biller::conformance::{self, BALANCE};
use afd_billing::biller::{
    Account, Ask, Attribution, Biller, Funds, OutagePolicy, RecordingBiller, Settlement,
    WithOutagePolicy, WorkKey,
};
use afd_billing::ledger::{LedgerBiller, log_lines, resettle};
use afd_billing::limits::fleet_spend;
use afd_billing::price::RatedLine;
use afd_billing::{Nanos, Posture};
use afd_db::test_util::{mint_id, mint_uuid7, unreachable_db};
use jiff::Timestamp;
use sqlx::Connection as _;

use crate::lane::Lane;

/// A line's charge in the usage-log checks.
pub(crate) const LINE: Nanos = Nanos::from_i64(25_000);

pub(crate) fn ledger(lane: &Lane) -> LedgerBiller {
    LedgerBiller::new(lane.pool.clone(), Entropy::new(), Arc::new(SystemClock))
}

pub(crate) async fn funded_lane() -> (Lane, Account) {
    let lane = Lane::create().await;
    lane.wallet(BALANCE.as_i64(), None, Timestamp::now()).await;
    let payer = Account {
        tenant: lane.tenant.clone(),
        workspace: lane.workspace.clone(),
    };
    (lane, payer)
}

pub(crate) fn rated(seq: u32) -> RatedLine {
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

pub(crate) fn settlement(
    lane: &Lane,
    payer: &Account,
    work: &WorkKey,
    lines: Vec<RatedLine>,
) -> Settlement {
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
async fn test_unsettled_lines_count_against_a_check() {
    let (lane, payer) = funded_lane().await;
    let biller = ledger(&lane);
    let work = WorkKey::FleetLease(mint_uuid7());
    let mut connection = lane.pool.acquire().await.expect("the lane answers");
    log_lines(
        &mut *connection,
        &Entropy::new(),
        Timestamp::now(),
        &settlement(&lane, &payer, &work, vec![rated(0)]),
    )
    .await
    .expect("the work logs its line");
    drop(connection);
    let left = Nanos::from_i64(BALANCE.as_i64() - LINE.as_i64());
    let check = |next: Nanos| {
        let (biller, payer, work) = (biller.clone(), payer.clone(), work.clone());
        async move {
            biller
                .check(&Ask { work, payer, next })
                .await
                .expect("the check answers")
        }
    };
    assert_eq!(
        check(left).await,
        Funds::Covered,
        "what the logged line leaves is covered"
    );
    let past = Nanos::from_i64(left.as_i64() + 1);
    assert_eq!(
        check(past).await,
        Funds::Short {
            available: left,
            asked: past
        },
        "the uncharged line counts before the relay settles it"
    );
}

#[tokio::test]
async fn test_billing_outage_fails_open() {
    let ask = Ask {
        work: WorkKey::FleetLease(mint_uuid7()),
        payer: Account {
            tenant: mint_uuid7(),
            workspace: mint_uuid7(),
        },
        next: LINE,
    };
    let unreachable = || LedgerBiller::new(unreachable_db(), Entropy::new(), Arc::new(SystemClock));
    let open = WithOutagePolicy::new(unreachable(), OutagePolicy::Open);
    assert_eq!(
        open.check(&ask).await.expect("an open policy answers"),
        Funds::Unconfirmed
    );
    let closed = WithOutagePolicy::new(unreachable(), OutagePolicy::Closed);
    let refused = closed
        .check(&ask)
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
    // Its own database: the pass reads the oldest unsettled lines it can see.
    let lane = Lane::isolated().await;
    lane.wallet(BALANCE.as_i64(), None, Timestamp::now()).await;
    let payer = Account {
        tenant: lane.tenant.clone(),
        workspace: lane.workspace.clone(),
    };
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
    assert_eq!(
        settled_at(&lane, &work).await,
        vec![None],
        "logged, not settled"
    );
    let balance = |biller: &LedgerBiller| {
        let biller = biller.clone();
        let payer = payer.clone();
        async move {
            biller
                .balance(&payer)
                .await
                .expect("balance answers")
                .balance
                .as_i64()
        }
    };
    assert_eq!(
        balance(&biller).await,
        BALANCE.as_i64(),
        "nothing charged yet"
    );

    // The relay's own pass, not a hand-rolled retry.
    let swept = resettle(&lane.pool, &biller, None)
        .await
        .expect("the pass reads");
    assert!(swept.settled >= 1, "{swept:?}");
    let stamped = settled_at(&lane, &work).await;
    assert!(
        matches!(stamped.as_slice(), [Some(_)]),
        "the sweeper settled the line: {stamped:?}"
    );
    assert_eq!(
        balance(&biller).await,
        BALANCE.as_i64() - LINE.as_i64(),
        "the logged line is charged once"
    );

    resettle(&lane.pool, &biller, None)
        .await
        .expect("the pass reads");
    assert_eq!(
        settled_at(&lane, &work).await,
        stamped,
        "a second pass settles nothing"
    );
    assert_eq!(
        balance(&biller).await,
        BALANCE.as_i64() - LINE.as_i64(),
        "a second pass charges nothing"
    );
    lane.cleanup().await;
}

/// `settled_at` of each of `work`'s lines, oldest first, as text.
async fn settled_at(lane: &Lane, work: &WorkKey) -> Vec<Option<String>> {
    sqlx::query_scalar(
        "SELECT settled_at::text FROM billing.usage_lines \
          WHERE work_type = $1 AND work_id = $2::uuid ORDER BY seq",
    )
    .bind(work.work_type())
    .bind(work.id().as_str())
    .fetch_all(&mut *lane.pool.acquire().await.expect("the lane answers"))
    .await
    .expect("reads the lines")
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
