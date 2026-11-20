//! The host row decides a placement: an assigned choice another placement
//! already spent admits nothing, a host that is gone admits nothing, and a
//! choice that cannot be recorded rolls the whole assignment back.

use super::super::refusal::Refusal;
use super::fixture::{attempts, fund, funded, queue};
use super::{Fixture, mint};
use af_crypto::entropy::Entropy;
use af_wire::run::AcceptedKinds;
use afd_runner::Runners;

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn an_offered_choice_admits_queued_work_once_and_a_spent_choice_admits_nothing() {
    // Its own database: an assignment scans every queued chat, and the
    // shared lane's belong to other proofs.
    let f = fund(Fixture::isolated().await, 2).await;
    let draft = queue(&f, "Assigned through the shared poll").await;
    let ticket = Runners::new(f.db.clone(), Entropy::new())
        .assignment_ticket(&f.runner, AcceptedKinds::ALL)
        .await
        .expect("the host answers")
        .expect("an active host is assigned");
    let assigned = f
        .drafts
        .assign_next_draft(&f.runner, ticket)
        .await
        .expect("assignment answers")
        .expect("the queued chat is admitted");
    assert_eq!(assigned.draft_id, draft.id.to_string());
    let second = queue(&f, "A second chat for the spent choice").await;
    let spent = f
        .drafts
        .assign_next_draft(&f.runner, ticket)
        .await
        .expect("assignment answers");
    assert!(spent.is_none(), "the recorded placement spent the choice");
    sqlx::query("UPDATE setup.drafts SET expires_at=now()-interval '1 second' WHERE id=$1")
        .bind(&second.id)
        .execute(&mut *f.connection().await)
        .await
        .expect("the second chat's retention closed");
    let fresh = Runners::new(f.db.clone(), Entropy::new())
        .assignment_ticket(&f.runner, AcceptedKinds::ALL)
        .await
        .expect("the host answers")
        .expect("an active host is assigned");
    let skipped = f
        .drafts
        .assign_next_draft(&f.runner, fresh)
        .await
        .expect("assignment answers");
    assert!(
        skipped.is_none(),
        "a lapsed chat is passed over, and nothing else waits"
    );
    assert_eq!(attempts(&f).await, 1);
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn a_host_that_is_gone_admits_nothing() {
    let f = funded(1).await;
    let draft = queue(&f, "Assigned by a host that never registered").await;
    let placed = f
        .drafts
        .assign_queued(&mint(), &draft.id)
        .await
        .expect("assignment answers");
    assert!(placed.is_none());
    assert_eq!(attempts(&f).await, 0);
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn a_host_choice_that_cannot_be_recorded_rolls_the_assignment_back() {
    let f = funded(1).await;
    let draft = queue(&f, "Assigned while the host row refuses its choice").await;
    let runner = f.runner.clone();
    let refusal = Refusal::install(
        &f.owner_url(),
        "runner.runners",
        "choice",
        runner.as_str(),
        &format!("NEW.id='{runner}'::uuid AND NEW.run_version<>OLD.run_version"),
        "RETURN NULL",
    )
    .await
    .expect("the fixture refusal installs");
    let placed = f
        .drafts
        .assign_queued(&f.runner, &draft.id)
        .await
        .expect("assignment answers");
    assert!(placed.is_none(), "an unrecorded choice places nothing");
    assert_eq!(attempts(&f).await, 0, "the attempt rolled back with it");
    drop(refusal);
    let retried = f
        .drafts
        .assign_queued(&f.runner, &draft.id)
        .await
        .expect("assignment answers");
    assert!(
        retried.is_some(),
        "the message stayed queued for the next poll"
    );
    f.cleanup().await;
}
