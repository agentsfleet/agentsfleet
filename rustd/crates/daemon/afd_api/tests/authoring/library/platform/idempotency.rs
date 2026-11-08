//! Retrying an identical curation does not invent a new metadata version.

use super::*;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn identical_metadata_preserves_the_resource_tag_and_timestamp() {
    let case = Case::create().await;
    let patch = json!({"published":true,"description":"Reviewed setup",
        "required_credentials_reasons":{"github":"Read workflow failures"}});
    let mut tag = case.tag.clone();
    let mut replies = Vec::new();
    for _ in 0..2 {
        let response = case
            .patch_admin(&patch.to_string(), &[(header::IF_MATCH, &tag)])
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        tag = response.headers()[header::ETAG]
            .to_str()
            .expect("curated tag")
            .to_owned();
        replies.push((tag.clone(), json_body(response).await));
    }
    assert_ne!(tag, case.tag, "the first publication must change metadata");
    assert_eq!(replies[0], replies[1], "a repeated edit must be a no-op");
    let stale = case
        .patch_admin(&patch.to_string(), &[(header::IF_MATCH, &case.tag)])
        .await;
    assert_eq!(stale.status(), StatusCode::PRECONDITION_FAILED);
    assert_eq!(json_body(stale).await["etag"], tag);
    case.inner.base.cleanup().await;
}
