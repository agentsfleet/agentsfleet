//! The behaviour every [`Biller`] must show, as one suite any implementation
//! runs: `LedgerBiller` against a live database, `RecordingBiller` in memory,
//! and any later billing service or third-party biller.
//!
//! The caller supplies `fresh`, which answers a new payer funded with exactly
//! [`BALANCE`], so each check starts from a clean account.

use af_wire::usage::UsageLine;
use jiff::Timestamp;

use super::{Account, Ask, Attribution, Biller, Funds, Receipt, Settlement, WorkKey};
use crate::price::RatedLine;
use crate::{Nanos, Posture};

/// What an answering biller's `expect` says, per call.
const CHECKS: &str = "check answers";
const SETTLES: &str = "settle answers";
const BALANCES: &str = "balance answers";
/// The provider, model and fleet name a conformance line names.
const NAMED: &str = "conformance";
/// What every fresh payer is funded with.
pub const BALANCE: Nanos = Nanos::from_i64(1_000_000);
/// A line's charge in the settle checks.
const LINE: Nanos = Nanos::from_i64(100_000);

/// Runs every check against `biller`.
///
/// # Panics
/// When `biller` breaks a rule of the lifecycle; the message names the rule.
pub async fn every_check<F, Fut>(biller: &dyn Biller, mut fresh: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Account>,
{
    an_ask_within_the_balance_is_covered(biller, fresh().await).await;
    an_ask_past_the_balance_is_short(biller, fresh().await).await;
    settling_twice_charges_once(biller, fresh().await).await;
    a_short_settlement_leaves_a_debt(biller, fresh().await).await;
}

fn work() -> WorkKey {
    WorkKey::FleetLease(afd_db::test_util::mint_id())
}

fn ask(work: &WorkKey, payer: &Account, next: Nanos) -> Ask {
    Ask {
        work: work.clone(),
        payer: payer.clone(),
        next,
    }
}

const fn line(seq: u32, charged: Nanos, runtime_ms: u32) -> RatedLine {
    RatedLine {
        usage: UsageLine {
            kind: af_wire::usage::LineKind::Runtime,
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
            fleet: afd_db::test_util::mint_id(),
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
async fn an_ask_within_the_balance_is_covered(biller: &dyn Biller, payer: Account) {
    let funds = biller
        .check(&ask(&work(), &payer, BALANCE))
        .await
        .expect(CHECKS);
    assert_eq!(funds, Funds::Covered, "the whole balance is covered");
    let balance = biller.balance(&payer).await.expect(BALANCES);
    assert_eq!(balance.balance, BALANCE, "a check moves no money");
}

#[expect(clippy::expect_used, reason = "a conformance check fails loudly")]
async fn an_ask_past_the_balance_is_short(biller: &dyn Biller, payer: Account) {
    let asked = Nanos::from_i64(BALANCE.as_i64() + 1);
    let funds = biller
        .check(&ask(&work(), &payer, asked))
        .await
        .expect(CHECKS);
    assert_eq!(
        funds,
        Funds::Short {
            available: BALANCE,
            asked
        },
        "one nano past the balance is short, and says what is left"
    );
}

#[expect(clippy::expect_used, reason = "a conformance check fails loudly")]
async fn settling_twice_charges_once(biller: &dyn Biller, payer: Account) {
    let work = work();
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
    assert_eq!(balance.unsettled, Nanos::ZERO, "nothing is left to charge");
}

/// A settlement half again past the balance: the balance takes the whole
/// charge and goes below zero, records that it ran out, and refuses the next
/// check until a grant repays the debt.
///
/// # Panics
/// When the biller takes less than the charge, leaves the balance anywhere but
/// the debt, or covers any ask for a payer in debt.
#[expect(clippy::expect_used, reason = "a conformance check fails loudly")]
pub async fn a_short_settlement_leaves_a_debt(biller: &dyn Biller, payer: Account) -> Receipt {
    let settled = work();
    let past = Nanos::from_i64(BALANCE.as_i64() + BALANCE.as_i64() / 2);
    let receipt = biller
        .settle(&settlement(&settled, &payer, vec![line(0, past, 0)]))
        .await
        .expect(SETTLES);
    assert_eq!(receipt.charged, past, "the line's whole cost is charged");
    assert_eq!(receipt.taken, past, "the balance takes all of it");
    let balance = biller.balance(&payer).await.expect(BALANCES);
    assert_eq!(
        balance.balance.as_i64(),
        BALANCE.as_i64() - past.as_i64(),
        "the balance owes what it could not cover"
    );
    assert!(
        balance.exhausted_at.is_some(),
        "a balance in debt records when it ran out"
    );
    let asked = Nanos::from_i64(1);
    let funds = biller
        .check(&ask(&work(), &payer, asked))
        .await
        .expect(CHECKS);
    assert_eq!(
        funds,
        Funds::Short {
            available: Nanos::ZERO,
            asked
        },
        "a payer in debt is covered for nothing, not even one nano"
    );
    let free = biller
        .check(&ask(&work(), &payer, Nanos::ZERO))
        .await
        .expect(CHECKS);
    assert_eq!(
        free,
        Funds::Short {
            available: Nanos::ZERO,
            asked: Nanos::ZERO
        },
        "a payer in debt is short even of an ask for nothing, as a self-managed run's"
    );
    receipt
}
