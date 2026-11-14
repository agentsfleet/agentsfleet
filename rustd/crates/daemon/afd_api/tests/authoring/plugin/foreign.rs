//! Account ownership covers reads, source capture and every mutation.

use super::{
    Case, Method, StatusCode, Value, header, json, json_body, mint, send, send_with_headers,
};
use afd_auth::Scope;
use afd_db::test_util::WorkspaceSeed;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn plugins_remain_private_across_accounts() {
    let owner = Case::create().await;
    let foreign = Case::create().await;
    let published = owner.publish(mint().as_str()).await;
    assert_eq!(published.status(), StatusCode::CREATED);
    let tag = published.headers()[header::ETAG]
        .to_str()
        .expect("tag")
        .to_owned();
    let published = json_body(published).await;
    let id = published["plugin_id"].as_str().expect("Plugin id");
    let revision = published["revision_id"].as_str().expect("revision id");
    let path = format!("{}/{id}", foreign.collection());
    let listed = json_body(foreign.get(&foreign.collection()).await).await;
    assert_eq!(listed["items"], json!([]));
    for suffix in [
        String::new(),
        "/revisions".into(),
        format!("/revisions/{revision}"),
    ] {
        let response = foreign.get(&format!("{path}{suffix}")).await;
        assert_hidden(response).await;
    }
    assert_mutations_hidden(&foreign, &path, &tag, &published).await;
    assert_same_account_access(&owner, &published).await;
    let retained = format!("{}/{id}", owner.collection());
    assert_eq!(owner.get(&retained).await.status(), StatusCode::OK);
    assert_eq!(
        json_body(owner.get(&format!("{retained}/revisions")).await).await["items"]
            .as_array()
            .expect("history")
            .len(),
        1
    );
    foreign.base.cleanup().await;
    owner.base.cleanup().await;
}

async fn assert_hidden(response: axum::response::Response) {
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(json_body(response).await["error_code"], "UZ-AUTHOR-001");
}

async fn assert_mutations_hidden(case: &Case, path: &str, tag: &str, published: &Value) {
    assert_hidden(
        send(
            &case.router,
            Method::DELETE,
            path,
            Some(&case.base.token),
            "",
        )
        .await,
    )
    .await;
    let revise = send_with_headers(
        &case.router,
        Method::POST,
        &format!("{path}/revisions"),
        Some(&case.base.token),
        &json!({"draft_candidate_id":case.candidate}).to_string(),
        &[
            (header::IF_MATCH, tag),
            (
                header::HeaderName::from_static("idempotency-key"),
                mint().as_str(),
            ),
        ],
    )
    .await;
    assert_hidden(revise).await;
    let mut request = case.base.request(&mint());
    request["source"] = json!({"kind":"plugin", "plugin_id":published["plugin_id"], "revision_id":published["revision_id"]});
    request.as_object_mut().expect("request").remove("message");
    assert_hidden(
        send(
            &case.router,
            Method::POST,
            &case.base.collection(),
            Some(&case.base.token),
            &request.to_string(),
        )
        .await,
    )
    .await;
}

async fn assert_same_account_access(case: &Case, published: &Value) {
    let workspace = mint();
    WorkspaceSeed::new(&workspace, &case.base.tenant)
        .insert(&mut *case.base.db.acquire().await.expect("workspace connection"))
        .await
        .expect("another workspace in the owning account");
    let router = case
        .base
        .services(&Scope::ALL)
        .with_live_ownership()
        .router();
    let collection = format!("/v1/workspaces/{workspace}/plugins");
    let plugin = format!(
        "{collection}/{}",
        published["plugin_id"].as_str().expect("Plugin id")
    );
    for path in [
        collection.clone(),
        plugin.clone(),
        format!("{plugin}/revisions"),
    ] {
        assert_eq!(
            send(&router, Method::GET, &path, Some(&case.base.token), "")
                .await
                .status(),
            StatusCode::OK
        );
    }
    let mut request = case.base.request(&mint());
    request["source"] = json!({"kind":"plugin", "plugin_id":published["plugin_id"], "revision_id":published["revision_id"]});
    request.as_object_mut().expect("request").remove("message");
    let response = send(
        &router,
        Method::POST,
        &format!("/v1/workspaces/{workspace}/drafts"),
        Some(&case.base.token),
        &request.to_string(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
}
