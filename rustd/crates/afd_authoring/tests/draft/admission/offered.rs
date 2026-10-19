//! The host row decides a placement: an offered choice another placement
//! already spent admits nothing, a host that is gone admits nothing, and a
//! choice that cannot be recorded rolls the whole admission back.

use super::super::refusal::Refusal;
use super::fixture::{attempts, fund, funded, queue};
use super::{Fixture, mint};
use afd_crypto::entropy::Entropy;
use afd_runner::Runners;
use afd_wire::lease::AcceptedKinds;

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn an_offered_choice_admits_queued_work_once_and_a_spent_choice_admits_nothing() {
    // Its own database: an offered claim scans every queued chat, and the
    // shared lane's belong to other proofs.
    let f = fund(Fixture::isolated().await, 2).await;
    let draft = queue(&f, "Claimed through the shared poll").await;
    let ticket = Runners::new(f.db.clone(), Entropy::new())
        .claim_offer(&f.runner, AcceptedKinds::ALL)
        .await
        .expect("the host answers")
        .expect("an active host is offered");
    let claimed = f
        .drafts
        .claim_offered(&f.runner, ticket)
        .await
        .expect("admission answers")
        .expect("the queued chat is admitted");
    assert_eq!(claimed.draft_id, draft.id.to_string());
    queue(&f, "A second chat for the spent choice").await;
    let spent = f
        .drafts
        .claim_offered(&f.runner, ticket)
        .await
        .expect("admission answers");
    assert!(spent.is_none(), "the recorded placement spent the choice");
    assert_eq!(attempts(&f).await, 1);
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn a_host_that_is_gone_admits_nothing() {
    let f = funded(1).await;
    let draft = queue(&f, "Claimed by a host that never registered").await;
    let placed = f
        .drafts
        .claim_queued(&mint(), &draft.id)
        .await
        .expect("admission answers");
    assert!(placed.is_none());
    assert_eq!(attempts(&f).await, 0);
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn a_host_choice_that_cannot_be_recorded_rolls_the_admission_back() {
    let f = funded(1).await;
    let draft = queue(&f, "Claimed while the host row refuses its choice").await;
    let runner = f.runner.clone();
    let refusal = Refusal::install(
        &f.owner_url(),
        "runner.runners",
        "choice",
        runner.as_str(),
        &format!("NEW.id='{runner}'::uuid AND NEW.lease_version<>OLD.lease_version"),
        "RETURN NULL",
    )
    .await;
    let placed = f
        .drafts
        .claim_queued(&f.runner, &draft.id)
        .await
        .expect("admission answers");
    assert!(placed.is_none(), "an unrecorded choice places nothing");
    assert_eq!(attempts(&f).await, 0, "the attempt rolled back with it");
    drop(refusal);
    let retried = f
        .drafts
        .claim_queued(&f.runner, &draft.id)
        .await
        .expect("admission answers");
    assert!(
        retried.is_some(),
        "the message stayed queued for the next poll"
    );
    f.cleanup().await;
}
