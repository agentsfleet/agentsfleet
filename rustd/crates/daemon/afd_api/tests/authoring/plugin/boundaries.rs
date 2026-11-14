//! Retired URLs and source spellings cannot enter the renamed authoring path.

use super::{Case, Method, StatusCode, json, mint, send};

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn removed_workspace_library_routes_return_not_found() {
    let case = Case::create().await;
    let old = format!("/v1/workspaces/{}/fleet-libraries", case.base.workspace);
    let id = mint();
    for (suffix, methods) in [
        (String::new(), vec![Method::GET, Method::POST]),
        ("/owned".into(), vec![Method::GET]),
        (format!("/{id}"), vec![Method::GET, Method::DELETE]),
        (format!("/{id}/revisions"), vec![Method::GET, Method::POST]),
        (format!("/{id}/revisions/{}", mint()), vec![Method::GET]),
    ] {
        for method in methods {
            let path = format!("{old}{suffix}");
            let response = send(&case.router, method, &path, Some(&case.base.token), "{}").await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
        }
    }
    case.base.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn plugin_sources_reject_retired_spellings_and_non_uuid_ids() {
    let case = Case::create().await;
    for source in [
        json!({"kind":"library","library_id":mint(),"revision_id":mint()}),
        json!({"kind":"plugin","library_id":mint(),"revision_id":mint()}),
        json!({"kind":"plugin","plugin_id":"example","revision_id":mint()}),
        json!({"kind":"plugin","plugin_id":mint(),"revision_id":"example"}),
    ] {
        let mut body = case.base.request(&mint());
        body["source"] = source;
        let response = send(
            &case.router,
            Method::POST,
            &case.base.collection(),
            Some(&case.base.token),
            &body.to_string(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
    case.base.cleanup().await;
}
