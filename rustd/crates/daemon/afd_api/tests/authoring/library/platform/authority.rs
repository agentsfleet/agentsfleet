//! Platform capability never substitutes for private source ownership.

use super::*;
use crate::authoring::fixture::Fixture;
use afd_auth::Scope;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn platform_staging_refuses_foreign_candidates_retired_inputs_and_missing_scope() {
    let case = Case::create().await;
    let foreign = Fixture::create().await;
    let body = json!({"workspace_id":case.inner.base.workspace,"draft_candidate_id":case.inner.candidate,"label":"Foreign"});
    let response = stage_through(&foreign.router(), &foreign.token, &body).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let router = case.inner.base.services(&[Scope::FleetRead]).router();
    let response = stage_through(&router, &case.inner.base.token, &case.body()).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    for field in [
        "source_kind",
        "source_ref",
        "skill_markdown",
        "trigger_markdown",
        "replace",
    ] {
        let mut body = case.body();
        body[field] = json!("retired input");
        assert_eq!(
            case.stage(&body).await.status(),
            StatusCode::BAD_REQUEST,
            "{field}"
        );
    }
    let mut changed = case.body();
    changed["label"] = json!("Changed retry");
    assert_eq!(case.stage(&changed).await.status(), StatusCode::CONFLICT);
    foreign.cleanup().await;
    case.inner.base.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn conditional_curation_rejects_missing_or_stale_tags_and_published_deletion() {
    let mut case = Case::create().await;
    for (headers, status) in [
        (vec![], StatusCode::PRECONDITION_REQUIRED),
        (
            vec![(header::IF_MATCH, "\"stale\"")],
            StatusCode::PRECONDITION_FAILED,
        ),
    ] {
        let response = case.patch_admin(r#"{"published":true}"#, &headers).await;
        assert_eq!(response.status(), status);
        assert_eq!(
            case.inner.get(&case.tenant_path()).await.status(),
            StatusCode::NOT_FOUND
        );
    }
    for body in [
        json!({}),
        json!({"published":null}),
        json!({"source_repo":"owner/repo"}),
        json!({"required_credentials_reasons":{"secret":true}}),
    ] {
        let response = case
            .patch_admin(&body.to_string(), &[(header::IF_MATCH, &case.tag)])
            .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
    case.curate(&json!({"published":true})).await;
    assert_eq!(case.withdraw().await.status(), StatusCode::CONFLICT);
    assert_eq!(
        case.inner.get(&case.tenant_path()).await.status(),
        StatusCode::OK
    );
    case.inner.base.cleanup().await;
}
