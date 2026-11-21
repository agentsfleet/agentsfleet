//! The recording biller's own bookkeeping, and the counting wrapper passing
//! its answers through.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_billing::Nanos;
use afd_billing::biller::{Account, Biller, Observed, RecordingBiller, conformance};
use afd_db::test_util::mint_id;

/// The first funding, replaced by the second.
const FIRST_FUNDING: Nanos = Nanos::from_i64(10);
/// What the second funding sets the balance to.
const SECOND_FUNDING: Nanos = Nanos::from_i64(70_000);

fn payer() -> Account {
    Account {
        tenant: mint_id(),
        workspace: mint_id(),
    }
}

/// Funding a payer the biller already knows sets its balance in place rather
/// than opening a second wallet the reads would never find.
#[tokio::test]
async fn funding_a_known_payer_replaces_its_balance() {
    let biller = RecordingBiller::new();
    let payer = payer();
    biller.fund(&payer.tenant, FIRST_FUNDING);
    biller.fund(&payer.tenant, SECOND_FUNDING);
    let balance = biller
        .balance(&payer)
        .await
        .expect("a funded payer answers");
    assert_eq!(balance.balance, SECOND_FUNDING);
    assert_eq!(balance.exhausted_at, None);
}

/// A payer funded below zero owes a debt, and the counting wrapper answers
/// every check for it as the biller it wraps does.
#[tokio::test]
async fn an_observed_payer_in_debt_is_short() {
    let inner = RecordingBiller::new();
    let payer = payer();
    inner.fund(&payer.tenant, Nanos::from_i64(-SECOND_FUNDING.as_i64()));
    conformance::a_payer_in_debt_is_short(&Observed::new(inner), payer).await;
}
