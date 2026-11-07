//! Saved runner policy gates new placement without revoking issued task authority.

use afd_fleet::{lease::Acquired, task::TaskLifecycle as _};
use afd_wire::task::DecisionAssessment;

use super::{
    super::fixture::{Case, exercise},
    fixture::{Decisions, known, request},
};

mod race;
pub(super) mod support;
use support::{claimable, expire, issue, selected, set};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn configured_off_blocks_initial_source_discovery_and_claim_until_enabled() {
    exercise(async |case| {
        let fixture = Decisions::create(case).await;
        expire(case).await;
        set(case, false).await;
        assert!(
            !claimable(case).await,
            "direct claim also checks saved policy"
        );
        assert!(selected(case).await.is_none());
        set(case, true).await;
        assert!(claimable(case).await);
        assert!(selected(case).await.is_some());
        fixture.model.finish().await.expect("model cleanup");
    })
    .await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn configured_off_preserves_ordinary_chat_placement() {
    exercise(async |case| {
        expire(case).await;
        set(case, false).await;
        assert!(claimable(case).await);
        let acquired = selected(case)
            .await
            .expect("ordinary chat remains eligible");
        assert!(issue(case, &acquired).await.is_some());
    })
    .await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn configured_edit_after_claim_is_rechecked_before_issuing_a_source_lease() {
    exercise(async |case| {
        let fixture = Decisions::create(case).await;
        expire(case).await;
        let acquired = selected(case).await.expect("enabled source claimed");
        case.held
            .leases
            .record_received(&acquired)
            .await
            .expect("source history");
        refuse_after_edit(case, &acquired).await;
        set(case, true).await;
        assert!(
            issue(case, &acquired).await.is_some(),
            "refused issue preserves claim"
        );
        fixture.model.finish().await.expect("model cleanup");
    })
    .await;
}

async fn refuse_after_edit(case: &Case, acquired: &Acquired) {
    let before: i64 =
        sqlx::query_scalar("SELECT count(*) FROM runner.leases WHERE fleet_id=$1::uuid")
            .bind(&case.held.fleet)
            .fetch_one(
                &mut *case
                    .held
                    .fixtures
                    .database
                    .acquire()
                    .await
                    .expect("observer"),
            )
            .await
            .expect("lease count");
    set(case, false).await;
    assert!(issue(case, acquired).await.is_none());
    let after: i64 =
        sqlx::query_scalar("SELECT count(*) FROM runner.leases WHERE fleet_id=$1::uuid")
            .bind(&case.held.fleet)
            .fetch_one(
                &mut *case
                    .held
                    .fixtures
                    .database
                    .acquire()
                    .await
                    .expect("observer"),
            )
            .await
            .expect("lease count");
    assert_eq!(
        after, before,
        "no lease survives a rejected final eligibility proof"
    );
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn configured_off_blocks_queued_continuations_without_consuming_them() {
    exercise(continuation).await;
}

async fn continuation(case: &Case) {
    let binding = case
        .tasks
        .bind(case.lease(), &case.plan("queued-policy", 2))
        .await
        .expect("real task continuation");
    let queued = binding
        .items
        .get(1)
        .expect("queued goal")
        .task_id
        .as_ref()
        .expect("task");
    sqlx::query("UPDATE fleet.admissions SET settled_at=clock_timestamp() WHERE id=$1::uuid")
        .bind(&case.held.event_id)
        .execute(
            &mut *case
                .held
                .fixtures
                .database
                .acquire()
                .await
                .expect("source lifecycle"),
        )
        .await
        .expect("source retired before continuation");
    expire(case).await;
    let before = case
        .tasks
        .read(&case.scope, queued)
        .await
        .expect("queued task");
    set(case, false).await;
    assert!(!claimable(case).await);
    assert!(selected(case).await.is_none());
    assert_eq!(
        case.tasks
            .read(&case.scope, queued)
            .await
            .expect("unchanged task"),
        before
    );
    set(case, true).await;
    assert!(claimable(case).await);
    assert!(selected(case).await.is_some());
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn configured_off_preserves_inflight_decision_issuance_and_receipts() {
    exercise(async |case| {
        let fixture = Decisions::create(case).await;
        set(case, false).await;
        let (_, body) = fixture
            .reserve(&fixture.reservation(DecisionAssessment::Message, None))
            .await;
        let request = request(&body);
        let recorded = fixture.record(&request.call_id, known(&request)).await;
        assert_eq!(recorded.charged_nanos.as_deref(), Some("3"));
        assert_eq!(fixture.counts().await, (1, 1, 1));
        assert!(
            fixture
                .record(&request.call_id, known(&request))
                .await
                .replayed
        );
        fixture.model.finish().await.expect("model cleanup");
    })
    .await;
}
