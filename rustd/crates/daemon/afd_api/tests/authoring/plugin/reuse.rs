//! Saving a draft started from a Plugin keeps the pinned definition and digest.

use super::fixture::{install, make_installable};
use super::{
    Case, Method, StatusCode, Value, header, json, json_body, mint, send, send_with_headers,
};

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn plugin_revision_saves_an_independent_fleet() {
    let mut case = Case::create().await;
    make_installable(&mut case).await;
    let published = case.publish(mint().as_str()).await;
    assert_eq!(published.status(), StatusCode::CREATED);
    let published = json_body(published).await;
    let plugin = format!(
        "{}/{}",
        case.collection(),
        published["plugin_id"].as_str().expect("Plugin id")
    );
    let source = json!({"kind":"plugin","plugin_id":published["plugin_id"],"revision_id":published["revision_id"]});
    let card = json_body(case.get(&plugin).await).await;
    let draft = start_from(&case, &source).await;
    let draft = json_body(case.get(&draft).await).await;
    let source = json!({"kind":"draft","draft_candidate_id":draft["candidate"]["id"]});
    let (fleet, saved) = install(&case, &source, &draft).await;
    assert_eq!(saved["definition"], card["head"]["definition"]);
    let digest: String = sqlx::query_scalar(
        "SELECT canonical_digest FROM fleet.revisions WHERE fleet_id=$1 AND revision=1",
    )
    .bind(crate::authoring::id_field(&saved["id"]))
    .fetch_one(
        &mut *case
            .base
            .db
            .acquire()
            .await
            .expect("saved revision connection"),
    )
    .await
    .expect("saved revision digest");
    assert_eq!(
        digest,
        card["head"]["canonical_digest"]
            .as_str()
            .expect("pinned digest")
    );
    assert_independent(&mut case, &plugin, &fleet, &saved).await;
    case.base.cleanup().await;
}

async fn assert_independent(case: &mut Case, plugin: &str, fleet: &str, saved: &Value) {
    let current = case.get(plugin).await;
    let tag = current.headers()[header::ETAG]
        .to_str()
        .expect("Plugin tag")
        .to_owned();
    case.edit().await;
    let appended = send_with_headers(
        &case.router,
        Method::POST,
        &format!("{plugin}/revisions"),
        Some(&case.base.token),
        &json!({"draft_candidate_id":case.candidate}).to_string(),
        &[
            (header::IF_MATCH, &tag),
            (
                header::HeaderName::from_static("idempotency-key"),
                mint().as_str(),
            ),
        ],
    )
    .await;
    assert_eq!(appended.status(), StatusCode::CREATED);
    assert_eq!(
        json_body(case.get(fleet).await).await["definition"],
        saved["definition"]
    );
    let removed = send(
        &case.router,
        Method::DELETE,
        plugin,
        Some(&case.base.token),
        "",
    )
    .await;
    assert_eq!(removed.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        json_body(case.get(fleet).await).await["definition"],
        saved["definition"]
    );
}

/// Starts a second draft from the card's revision; answers its location.
async fn start_from(case: &Case, source: &Value) -> String {
    let body = json!({"source":source,"authoring_model_ref":case.base.model,"operation_id":mint()});
    let response = send(
        &case.router,
        Method::POST,
        &case.base.collection(),
        Some(&case.base.token),
        &body.to_string(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    response.headers()[header::LOCATION]
        .to_str()
        .expect("draft location")
        .to_owned()
}
