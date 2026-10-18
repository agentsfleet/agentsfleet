//! A message finishes once on the runner path: a gate refused at claim, a
//! revoked creator, a model lost before the report, or the report itself —
//! and a replayed report finishes nothing again.

use super::super::funnel::{FINISHED, about, only, property};
use super::{claim, funded, queue};
use afd_core::id::Uuid7;

/// The one finished step reported so far, and its state.
fn finished_once(f: &super::Fixture, draft: &Uuid7) -> String {
    let finished = only(f.reported(FINISHED), "exactly one terminal transition");
    about(f, &finished, draft);
    property(&finished, "state").expect("a finished step names its state")
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_report_a_terminal_report_finished_once_and_not_on_replay() {
    let f = funded(1).await;
    let draft = queue(&f, "Answer me").await;
    let lease = claim(&f, &draft).await.expect("admitted");
    assert!(
        f.reported(FINISHED).is_empty(),
        "admission finishes nothing"
    );
    let id = Uuid7::parse(&lease.lease_id).expect("lease id");
    let report = super::stop::finished(&lease);
    for replayed in [false, true] {
        let ack = f
            .drafts
            .report(&f.runner, &id, report.clone())
            .await
            .expect("report settles");
        assert_eq!(ack.replayed, replayed);
    }
    assert_eq!(finished_once(&f, &draft.id), "completed");
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_report_a_claim_gate_failure_finished_once() {
    let f = funded(1).await;
    let draft = queue(&f, "Selected model later disappears").await;
    sqlx::query("DELETE FROM model.tenant_models WHERE id=$1::uuid")
        .bind(f.model.as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("disable owned entry");
    for _ in 0..2 {
        assert!(claim(&f, &draft).await.is_none(), "nothing to admit");
    }
    assert_eq!(finished_once(&f, &draft.id), "failed");
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_report_a_model_lost_before_the_report_finished_once() {
    let f = funded(1).await;
    let draft = queue(&f, "Model goes away mid-turn").await;
    let lease = claim(&f, &draft).await.expect("admitted");
    sqlx::query("DELETE FROM model.tenant_models WHERE id=$1::uuid")
        .bind(f.model.as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("disable owned entry");
    let id = Uuid7::parse(&lease.lease_id).expect("lease id");
    let report = super::stop::finished(&lease);
    for _ in 0..2 {
        f.drafts
            .report(&f.runner, &id, report.clone())
            .await
            .expect("accounting settles without publishing");
    }
    assert_eq!(finished_once(&f, &draft.id), "failed");
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_report_a_revoked_creators_queued_message_finished_once() {
    let f = funded(1).await;
    let created = f
        .drafts
        .create(
            f.member_access(),
            f.request(super::mint(), Some("Private member question")),
            super::now(),
        )
        .await
        .expect("member's private draft");
    sqlx::query("DELETE FROM identity.memberships WHERE tenant_id=$1::uuid AND user_id=$2::uuid")
        .bind(f.tenant.as_str())
        .bind(f.member.as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("committed membership revocation");
    for _ in 0..2 {
        assert!(claim(&f, &created.draft).await.is_none());
    }
    let finished = only(f.reported(FINISHED), "the second claim cancels nothing");
    assert_eq!(finished.actor(), Some(f.member.as_str()));
    let draft = property(&finished, "draft_id");
    assert_eq!(draft.as_deref(), Some(created.draft.id.as_str()));
    assert_eq!(property(&finished, "state").as_deref(), Some("cancelled"));
    f.cleanup().await;
}
