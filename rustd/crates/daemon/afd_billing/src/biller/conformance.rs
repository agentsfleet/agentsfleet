//! The behaviour every [`Biller`] must show, as one suite any implementation
//! runs: `LedgerBiller` against a live database, `RecordingBiller` in memory,
//! and any later billing service or third-party biller.
//!
//! The caller supplies `fresh`, which answers a new payer funded with exactly
//! [`BALANCE`], so each check starts from a clean account. How a payer comes
//! to owe a debt is the biller's own business, so [`a_payer_in_debt_is_short`]
//! takes one the caller drove there.

use super::{Account, Ask, Biller, Funds, WorkKey};
use crate::Nanos;

/// What an answering biller's `expect` says, per call.
const CHECKS: &str = "check answers";
const BALANCES: &str = "balance answers";
/// What every fresh payer is funded with.
pub const BALANCE: Nanos = Nanos::from_i64(1_000_000);

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
}

fn work() -> WorkKey {
    WorkKey::FleetRun(afd_db::test_util::mint_id())
}

fn ask(payer: &Account, next: Nanos) -> Ask {
    Ask {
        work: work(),
        payer: payer.clone(),
        next,
    }
}

#[expect(clippy::expect_used, reason = "a conformance check fails loudly")]
async fn an_ask_within_the_balance_is_covered(biller: &dyn Biller, payer: Account) {
    let funds = biller.check(&ask(&payer, BALANCE)).await.expect(CHECKS);
    assert_eq!(funds, Funds::Covered, "the whole balance is covered");
    let balance = biller.balance(&payer).await.expect(BALANCES);
    assert_eq!(balance.balance, BALANCE, "a check moves no money");
}

#[expect(clippy::expect_used, reason = "a conformance check fails loudly")]
async fn an_ask_past_the_balance_is_short(biller: &dyn Biller, payer: Account) {
    let asked = Nanos::from_i64(BALANCE.as_i64() + 1);
    let funds = biller.check(&ask(&payer, asked)).await.expect(CHECKS);
    assert_eq!(
        funds,
        Funds::Short {
            available: BALANCE,
            asked
        },
        "one nano past the balance is short, and says what is left"
    );
}

/// A payer whose balance is below zero: it records that it ran out, and is
/// short of every ask, even one for nothing, until a grant repays the debt.
///
/// # Panics
/// When the biller covers any ask for `payer`, or reports no exhaustion.
#[expect(clippy::expect_used, reason = "a conformance check fails loudly")]
pub async fn a_payer_in_debt_is_short(biller: &dyn Biller, payer: Account) {
    let balance = biller.balance(&payer).await.expect(BALANCES);
    assert!(
        balance.balance.as_i64() < 0,
        "the caller drove it into debt"
    );
    assert!(
        balance.exhausted_at.is_some(),
        "a balance in debt records when it ran out"
    );
    for asked in [Nanos::from_i64(1), Nanos::ZERO] {
        let funds = biller.check(&ask(&payer, asked)).await.expect(CHECKS);
        assert_eq!(
            funds,
            Funds::Short {
                available: Nanos::ZERO,
                asked
            },
            "a payer in debt is short even of an ask for nothing, as a self-managed run's"
        );
    }
}
