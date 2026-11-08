//! Only explicit publication enables tenant discovery and pinned source reads.

use super::*;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_staged_slug_stays_hidden_until_publication_and_withdrawal_hides_it_again() {
    let mut case = Case::create().await;
    assert_staged_slug_is_hidden_and_replays(&case).await;
    case.curate(&json!({"published":true,"description":"Curated setup","required_credentials_reasons":{"github":"Read workflow failures"}})).await;
    let library = published_library_with_history(&case).await;
    let source =
        json!({"kind":"library","library_id":case.slug,"revision_id":case.staged["revision_id"]});
    let created = create_from(&case, &source).await;
    assert_eq!(created.status(), StatusCode::CREATED);
    assert_eq!(
        json_body(created).await["candidate"]["definition"],
        library["head"]["definition"]
    );
    let removed = send(
        &case.inner.router,
        Method::DELETE,
        &case.tenant_path(),
        Some(&case.inner.base.token),
        "",
    )
    .await;
    assert_eq!(removed.status(), StatusCode::NOT_FOUND);
    case.curate(&json!({"published":false})).await;
    assert_eq!(
        case.inner.get(&case.tenant_path()).await.status(),
        StatusCode::NOT_FOUND
    );
    let hidden_source = create_from(&case, &source).await;
    assert_eq!(hidden_source.status(), StatusCode::NOT_FOUND);
    assert_eq!(case.withdraw().await.status(), StatusCode::NO_CONTENT);
    assert_eq!(case.stage(&case.body()).await.status(), StatusCode::GONE);
    case.inner.base.cleanup().await;
}

/// The staged slug is the library id, a tenant cannot read it yet, and a
/// repeated staging replays the first answer.
async fn assert_staged_slug_is_hidden_and_replays(case: &Case) {
    assert_eq!(case.staged["library_id"], case.slug);
    assert_eq!(
        case.inner.get(&case.tenant_path()).await.status(),
        StatusCode::NOT_FOUND
    );
    let replay = case.stage(&case.body()).await;
    assert_eq!(replay.status(), StatusCode::CREATED);
    assert_eq!(json_body(replay).await, case.staged);
}

/// Reads the published library through the tenant route, proves its curated
/// card and its one-revision history, and answers the library body.
async fn published_library_with_history(case: &Case) -> Value {
    let response = case.inner.get(&case.tenant_path()).await;
    assert_eq!(response.status(), StatusCode::OK);
    let library = json_body(response).await;
    assert_eq!(library["card"]["id"], case.slug);
    assert_eq!(library["head"]["library_id"], case.slug);
    assert_eq!(library["card"]["description"], "Curated setup");
    assert_eq!(
        library["card"]["required_credentials_reasons"]["github"],
        "Read workflow failures"
    );
    let history = case
        .inner
        .get(&format!("{}/revisions", case.tenant_path()))
        .await;
    assert_eq!(history.status(), StatusCode::OK);
    assert_eq!(
        json_body(history).await["items"][0]["id"],
        case.staged["revision_id"]
    );
    library
}

/// Opens a draft from `source` in the tenant's own collection.
async fn create_from(case: &Case, source: &Value) -> Response {
    send(
        &case.inner.router,
        Method::POST,
        &case.inner.base.collection(),
        Some(&case.inner.base.token),
        &json!({"source":source,"authoring_model_ref":case.inner.base.model,"operation_id":mint()})
            .to_string(),
    )
    .await
}
