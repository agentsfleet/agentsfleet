//! The behaviour every [`Biller`] must show, as one suite any implementation
//! runs: `LedgerBiller` against a live database, `RecordingBiller` in memory,
//! and any later queue or third-party biller.
//!
//! The caller supplies `fresh`, which answers a new payer funded with exactly
//! [`BALANCE`], so each check starts from a clean account.

use std::time::Duration;

use afd_wire::usage::UsageLine;
use jiff::Timestamp;

use super::{Account, Attribution, Biller, Hold, Release, Reservation, Settlement, WorkKey};
use crate::price::RatedLine;
use crate::{Nanos, Posture};

/// What an answering biller's `expect` says, per call.
const RESERVES: &str = "reserve answers";
const SETTLES: &str = "settle answers";
const RELEASES: &str = "release answers";
const BALANCES: &str = "balance answers";
/// The provider, model and fleet name a conformance line names.
const NAMED: &str = "conformance";
/// What every fresh payer is funded with.
pub const BALANCE: Nanos = Nanos::from_i64(1_000_000);
/// A line's charge in the settle checks.
const LINE: Nanos = Nanos::from_i64(100_000);
/// What a hold in the checks asks for.
const CEILING: Nanos = Nanos::from_i64(400_000);
/// More than [`BALANCE`] in total once two of them race: 60% each.
const RACING: Nanos = Nanos::from_i64(600_000);
/// The runtime an expired lease reports: 300,000 nanos, under every ceiling here.
const EXPIRED_RUNTIME: Duration = Duration::from_secs(3);

/// Runs every check against `biller`.
///
/// # Panics
/// When `biller` breaks a rule of the lifecycle; the message names the rule.
pub async fn every_check<F, Fut>(biller: &dyn Biller, mut fresh: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Account>,
{
    reserving_twice_holds_once(biller, fresh().await).await;
    a_short_balance_answers_short(biller, fresh().await).await;
    racing_holds_never_exceed_the_balance(biller, fresh().await).await;
    settling_twice_charges_once(biller, fresh().await).await;
    a_released_work_reserves_no_more(biller, fresh().await).await;
    an_expired_lease_bills_its_last_runtime(biller, fresh().await).await;
}

fn work() -> WorkKey {
    WorkKey::FleetLease(afd_db::test_util::mint_uuid7())
}

fn ask(work: &WorkKey, payer: &Account, ceiling: Nanos) -> Reservation {
    Reservation {
        work: work.clone(),
        payer: payer.clone(),
        ceiling,
    }
}

fn line(seq: u32, charged: Nanos, runtime_ms: u32) -> RatedLine {
    RatedLine {
        usage: UsageLine {
            seq,
            at: Timestamp::UNIX_EPOCH,
            input_tokens: 0,
            cached_input_tokens: 0,
            cache_write_tokens: 0,
            cache_write_5m_tokens: 0,
            cache_write_1h_tokens: 0,
            output_tokens: 0,
            runtime_ms,
        },
        price: None,
        charged,
        unpriced_tokens: 0,
    }
}

fn settlement(work: &WorkKey, payer: &Account, lines: Vec<RatedLine>) -> Settlement {
    Settlement {
        work: work.clone(),
        payer: payer.clone(),
        attribution: Attribution::Fleet {
            fleet: afd_db::test_util::mint_uuid7(),
            fleet_name: NAMED.to_owned(),
            event_id: afd_db::test_util::mint_id(),
        },
        provider: NAMED.to_owned(),
        model: NAMED.to_owned(),
        posture: Posture::Platform,
        lines,
    }
}

#[expect(clippy::expect_used, reason = "a conformance check fails loudly")]
async fn reserving_twice_holds_once(biller: &dyn Biller, payer: Account) {
    let work = work();
    let first = biller
        .reserve(&ask(&work, &payer, CEILING))
        .await
        .expect(RESERVES);
    let again = biller
        .reserve(&ask(&work, &payer, CEILING))
        .await
        .expect(RESERVES);
    assert_eq!(
        first, again,
        "reserving the same work twice answers the same hold"
    );
    let balance = biller.balance(&payer).await.expect(BALANCES);
    assert_eq!(balance.held, CEILING, "the hold is counted once");
}

#[expect(clippy::expect_used, reason = "a conformance check fails loudly")]
async fn a_short_balance_answers_short(biller: &dyn Biller, payer: Account) {
    let asked = Nanos::from_i64(BALANCE.as_i64() + 1);
    let hold = biller
        .reserve(&ask(&work(), &payer, asked))
        .await
        .expect(RESERVES);
    assert_eq!(
        hold,
        Hold::Short {
            available: BALANCE,
            asked
        },
        "an ask past the balance is short"
    );
}

/// Two concurrent holds of 60% of [`BALANCE`] each: exactly one is granted.
///
/// # Panics
/// When the biller grants both, or neither.
#[expect(clippy::expect_used, reason = "a conformance check fails loudly")]
pub async fn racing_holds_never_exceed_the_balance(biller: &dyn Biller, payer: Account) {
    let (first, second) = (work(), work());
    let (left, right) = futures_util::future::join(
        biller.reserve(&ask(&first, &payer, RACING)),
        biller.reserve(&ask(&second, &payer, RACING)),
    )
    .await;
    let granted = [left.expect(RESERVES), right.expect(RESERVES)]
        .into_iter()
        .filter(|hold| matches!(hold, Hold::Held { .. }))
        .count();
    assert_eq!(
        granted, 1,
        "two holds that together pass the balance: exactly one is granted"
    );
}

#[expect(clippy::expect_used, reason = "a conformance check fails loudly")]
async fn settling_twice_charges_once(biller: &dyn Biller, payer: Account) {
    let work = work();
    biller
        .reserve(&ask(&work, &payer, CEILING))
        .await
        .expect(RESERVES);
    let lines = settlement(&work, &payer, vec![line(0, LINE, 0), line(1, LINE, 0)]);
    let first = biller.settle(&lines).await.expect(SETTLES);
    let again = biller.settle(&lines).await.expect(SETTLES);
    assert_eq!(
        first.charged.as_i64(),
        LINE.as_i64() * 2,
        "both lines are charged"
    );
    assert_eq!(
        again.charged,
        Nanos::ZERO,
        "a repeated settlement charges nothing"
    );
    assert_eq!(
        again.settled_lines, 0,
        "a repeated settlement settles no line"
    );
    let balance = biller.balance(&payer).await.expect(BALANCES);
    assert_eq!(
        balance.balance.as_i64(),
        BALANCE.as_i64() - LINE.as_i64() * 2
    );
    assert_eq!(
        balance.held.as_i64(),
        CEILING.as_i64() - LINE.as_i64() * 2,
        "settled lines leave the hold"
    );
}

#[expect(clippy::expect_used, reason = "a conformance check fails loudly")]
async fn a_released_work_reserves_no_more(biller: &dyn Biller, payer: Account) {
    let work = work();
    biller
        .reserve(&ask(&work, &payer, CEILING))
        .await
        .expect(RESERVES);
    biller
        .settle(&settlement(&work, &payer, vec![line(0, LINE, 0)]))
        .await
        .expect(SETTLES);
    biller
        .release(&work, Release::Completed)
        .await
        .expect(RELEASES);
    biller
        .release(&work, Release::Completed)
        .await
        .expect("a second release answers");
    let balance = biller.balance(&payer).await.expect(BALANCES);
    assert_eq!(balance.held, Nanos::ZERO, "a released hold holds nothing");
    let again = biller
        .reserve(&ask(&work, &payer, CEILING))
        .await
        .expect(RESERVES);
    assert_eq!(again, Hold::Closed, "a released work may not reserve again");
}

#[expect(clippy::expect_used, reason = "a conformance check fails loudly")]
async fn an_expired_lease_bills_its_last_runtime(biller: &dyn Biller, payer: Account) {
    let work = work();
    biller
        .reserve(&ask(&work, &payer, CEILING))
        .await
        .expect(RESERVES);
    let runtime_ms = u32::try_from(EXPIRED_RUNTIME.as_millis()).expect("three seconds fits");
    let runtime_cost = Nanos::from_i64(
        i64::from(runtime_ms) * crate::RUN_NANOS_PER_SEC / crate::nanos::MS_PER_SEC,
    );
    biller
        .settle(&settlement(
            &work,
            &payer,
            vec![line(0, runtime_cost, runtime_ms)],
        ))
        .await
        .expect(SETTLES);
    biller
        .release(
            &work,
            Release::Expired {
                runtime: EXPIRED_RUNTIME,
            },
        )
        .await
        .expect(RELEASES);
    let balance = biller.balance(&payer).await.expect(BALANCES);
    assert_eq!(
        balance.balance.as_i64(),
        BALANCE.as_i64() - runtime_cost.as_i64(),
        "the last runtime is billed"
    );
    assert_eq!(
        balance.held,
        Nanos::ZERO,
        "the expired lease's hold is released"
    );
}
