//! Adoption requests reach native reviewed-revision custody and preserve paused work.
use super::*;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn goal_adoption_refuses_unknown_reviewed_revision_without_mutating_paused_work() {
    let lane = TestDatabase::shared();
    let fixture = Tasks::create(&lane).await;
    let router = Fleet::live(
        fixture.base.database.clone(),
        SUBJECT,
        ScopeSet::from_scopes(&[Scope::FleetRead, Scope::FleetWrite]),
    )
    .with_owned_workspace(fixture.base.workspace.clone())
    .router();
    let (_, tag) = read_task(&router, &fixture, 0).await;
    let response = patch(
        &router,
        &fixture,
        0,
        json!({"state":"paused","expected_generation":1}),
        &tag,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let (paused, tag) = read_task(&router, &fixture, 0).await;
    let request = json!({"goal_revision_id":afd_db::test_util::mint_id(),
        "expected_generation":2,"operation_id":afd_db::test_util::mint_id()});
    for _ in 0..2 {
        let response = patch(&router, &fixture, 0, request.clone(), &tag).await;
        assert_eq!(response.status(), StatusCode::CONFLICT);
        let body = json_body(response).await;
        assert_eq!(body.get("error_code"), Some(&json!("UZ-DT-004")));
        assert_eq!(body.get("current_state"), Some(&json!("paused")));
        let (after, after_tag) = read_task(&router, &fixture, 0).await;
        assert_eq!(after, paused);
        assert_eq!(after_tag, tag);
    }
    assert_eq!(queued(&fixture).await, 0);
    fixture.base.cleanup().await;
}
