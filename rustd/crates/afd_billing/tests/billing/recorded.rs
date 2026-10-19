//! The recording biller's own bookkeeping, and the counting wrapper's answer
//! to a settlement that charged nothing new.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_billing::Nanos;
use afd_billing::biller::{Account, Biller, Observed, RecordingBiller, Release, WorkKey};
use afd_db::test_util::mint_uuid7;

use crate::biller::{LINE, rated};

/// The first funding, replaced by the second.
const FIRST_FUNDING: Nanos = Nanos::from_i64(10);
/// What the second funding sets the balance to.
const SECOND_FUNDING: Nanos = Nanos::from_i64(70_000);

fn payer() -> Account {
    Account {
        tenant: mint_uuid7(),
        workspace: mint_uuid7(),
    }
}

/// A settlement against no lane: the recording biller keeps no fleet rows.
fn settlement(payer: &Account, work: &WorkKey) -> afd_billing::biller::Settlement {
    afd_billing::biller::Settlement {
        work: work.clone(),
        payer: payer.clone(),
        attribution: afd_billing::biller::Attribution::Fleet {
            fleet: mint_uuid7(),
            fleet_name: "recorded".to_owned(),
            event_id: afd_db::test_util::mint_id(),
        },
        provider: "recorded".to_owned(),
        model: "recorded".to_owned(),
        posture: afd_billing::Posture::Platform,
        lines: vec![rated(1)],
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
    assert_eq!(balance.held, Nanos::from_i64(0));
    assert_eq!(balance.exhausted_at, None);
}

/// Every settlement and release is kept, in order, for a domain test to read.
#[tokio::test]
async fn settlements_and_releases_are_kept_in_order() {
    let biller = RecordingBiller::new();
    let payer = payer();
    biller.fund(&payer.tenant, SECOND_FUNDING);
    let work = WorkKey::FleetLease(mint_uuid7());
    let charged = settlement(&payer, &work);
    biller
        .settle(&charged)
        .await
        .expect("the settlement answers");
    biller
        .release(&work, Release::Failed)
        .await
        .expect("the release answers");
    assert_eq!(biller.settlements(), vec![charged]);
    assert_eq!(biller.releases(), vec![(work, Release::Failed)]);
}

/// A retried settlement charges nothing, and the counting wrapper passes the
/// empty receipt through with the balance untouched by the retry.
#[tokio::test]
async fn an_observed_retry_charges_nothing_new() {
    let payer = payer();
    let work = WorkKey::FleetLease(mint_uuid7());
    let charged = settlement(&payer, &work);
    let inner = Observed::new({
        let inner = RecordingBiller::new();
        inner.fund(&payer.tenant, SECOND_FUNDING);
        inner
    });
    let first = inner
        .settle(&charged)
        .await
        .expect("the first settle answers");
    assert_eq!(first.settled_lines, 1);
    assert_eq!(first.taken, LINE);
    let again = inner.settle(&charged).await.expect("the retry answers");
    assert_eq!(again.settled_lines, 0);
    assert_eq!(again.charged, Nanos::from_i64(0));
    assert_eq!(again.taken, Nanos::from_i64(0));
    let balance = inner.balance(&payer).await.expect("the balance answers");
    assert_eq!(
        balance.balance,
        Nanos::from_i64(SECOND_FUNDING.as_i64() - LINE.as_i64())
    );
}
