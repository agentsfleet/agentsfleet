//! A failed prerequisite preserves identity; stale review reserves nothing.

use super::super::{fixture::now, mint};
use super::fixture::Case;
use afd_authoring::types::Start;

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn unfunded_save_replays_one_fleet_and_funding_resumes_it() {
    let c = Case::create().await;
    let (_, request) = c.request("retained-setup", Start::None).await;
    let key = mint();
    let first = c.save(&key, request.clone()).await;
    assert_eq!(first.status, "installing");
    assert!(
        first
            .setup
            .checks
            .iter()
            .any(|check| check.kind == "wallet" && check.state == "pending")
    );
    assert!(
        !first.needs_re_review,
        "readiness alone does not change review identity"
    );
    assert_eq!(c.save(&key, request.clone()).await.id, first.id);
    assert_eq!(c.fleet_count().await, 1);
    c.fund().await;
    let resumed = c.save(&key, request).await;
    assert_eq!(resumed.id, first.id);
    assert_eq!(resumed.status, "active");
    assert_eq!(c.fleet_count().await, 1);
    c.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn stale_review_allocates_neither_fleet_nor_replay_receipt() {
    let c = Case::create().await;
    let (_, mut request) = c.request("stale-review", Start::None).await;
    request.name = "changed-after-review".to_owned();
    let error = c
        .creations
        .save(c.base.access(), &mint(), request, now())
        .await
        .expect_err("fresh review required");
    assert_eq!(error.code().as_str(), "UZ-AUTHOR-006");
    assert!(error.review_digest().is_some());
    assert_eq!(c.fleet_count().await, 0);
    let receipts: i64 = sqlx::query_scalar("SELECT count(*) FROM authoring.replay_receipts WHERE workspace_id=$1::uuid AND scope LIKE '%:fleet:create:%'")
        .bind(c.base.workspace.as_str()).fetch_one(&mut *c.base.connection().await).await.expect("creation receipts");
    assert_eq!(receipts, 0);
    c.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn accepted_key_refuses_changed_body_and_foreign_creator() {
    let c = Case::create().await;
    let (_, request) = c.request("bound-save", Start::None).await;
    let key = mint();
    let first = c.save(&key, request.clone()).await;
    let mut changed = request.clone();
    changed.start = Start::RunNow {
        message: "different choice".to_owned(),
    };
    let error = c
        .creations
        .save(c.base.access(), &key, changed, now())
        .await
        .expect_err("body is bound");
    assert_eq!(error.code().as_str(), "UZ-AUTHOR-005");
    let error = c
        .creations
        .save(c.base.member_access(), &key, request, now())
        .await
        .expect_err("creator is bound");
    assert_eq!(error.code().as_str(), "UZ-AUTHOR-001");
    assert_eq!(c.fleet_count().await, 1);
    assert_eq!(
        c.creations
            .read(c.base.access(), &first.id)
            .await
            .expect("retained fleet")
            .status,
        "installing"
    );
    c.cleanup().await;
}

/// A queue no socket reaches: the stream stage of Save fails, nothing else.
fn unreachable(c: &Case) -> afd_authoring::Creations {
    let config = afd_dragonfly::config::DragonflyConfig::from_url(
        afd_dragonfly::config::DragonflyRole::Default,
        "redis://127.0.0.1:1".to_owned(),
    )
    .with_request_timeout(std::time::Duration::from_millis(250));
    let queue = afd_dragonfly::Dragonfly::unreachable(&config).expect("lazy manager");
    let admissions = afd_admission::Admissions::new(
        c.base.db.clone(),
        queue.clone(),
        afd_crypto::entropy::Entropy::new(),
    );
    afd_authoring::Creations::new(c.setup.clone(), admissions)
}

fn stream_state(saved: &afd_authoring::SavedFleet) -> &str {
    saved
        .setup
        .checks
        .iter()
        .find(|check| check.kind == "stream")
        .expect("stream check")
        .state
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn save_with_unavailable_notifications_activates_and_still_binds_body_and_actor() {
    let c = Case::create().await;
    c.fund().await;
    let (_, request) = c.request("stream-fault", Start::None).await;
    let key = mint();
    let broken = unreachable(&c);
    let first = broken
        .save(c.base.access(), &key, request.clone(), now())
        .await
        .expect("a failed external stage is progress, not an error");
    assert_eq!(first.status, "active");
    assert_eq!(stream_state(&first), "ready");
    let again = broken
        .save(c.base.access(), &key, request.clone(), now())
        .await
        .expect("retry against the same failure");
    assert_eq!(
        (again.id.clone(), again.status.as_str()),
        (first.id.clone(), "active")
    );
    assert_bound(&c, &broken, &key, &request).await;
    assert_eq!(c.fleet_count().await, 1, "no stage failure allocates twice");
    let resumed = c.save(&key, request.clone()).await;
    assert_eq!(resumed.id, first.id, "recovery resumes the retained fleet");
    assert_eq!(resumed.status, "active");
    assert_eq!(stream_state(&resumed), "ready");
    assert_bound(&c, &c.creations, &key, &request).await;
    assert_eq!(c.save(&key, request).await.id, first.id);
    assert_eq!(c.fleet_count().await, 1);
    c.cleanup().await;
}

/// The accepted key refuses a changed body and a different person, whatever
/// stage the setup it names has reached.
async fn assert_bound(
    c: &Case,
    creations: &afd_authoring::Creations,
    key: &afd_core::id::Uuid7,
    request: &afd_authoring::types::SaveFleet,
) {
    let mut changed = request.clone();
    changed.start = Start::RunNow {
        message: "different choice".to_owned(),
    };
    let error = creations
        .save(c.base.access(), key, changed, now())
        .await
        .expect_err("body is bound");
    assert_eq!(error.code(), afd_core::error_code::AUTHORING_REPLAY);
    let error = creations
        .save(c.base.member_access(), key, request.clone(), now())
        .await
        .expect_err("creator is bound");
    assert_eq!(error.code(), afd_core::error_code::AUTHORING_NOT_FOUND);
}
