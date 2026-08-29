//! What the fleets surface refuses about the REQUEST — its query, and its body.
//!
//! Sibling of `workspace_fleets.rs`, which pins who may act at all. Split along
//! the axis each proves: that one is the guard, the scope rung and the ownership
//! layer, and this is everything a caller can get wrong once past them.
//!
//! Every case here is answered without either datastore being reached, which is
//! the property that makes them worth pinning — a refusal that needed a row
//! would be one a client waits on a round trip for.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use crate::harness;

use http::{Method, StatusCode};
use serde_json::Value;

use self::harness::{Fleet, OWNED_WORKSPACE};

/// A tenant api-key, shaped as the authenticator classifies one.
const TENANT_KEY: &str = "agt_tdecafbaddecafbaddecafbaddecafbaddecafbaddecafbaddecafbaddecafbad";

/// The subject the fixture credential resolves to.
const SUBJECT: &str = "user_2fleets";

/// A well-formed fleet identifier.
const FLEET: &str = "01924f4e-0000-7000-8000-00000000f1ee";

/// Every scope this surface asks for, so nothing here is refused by the rung —
/// which axis is under test is the whole point of the split.
const EVERY_FLEET_SCOPE: afd_auth::scope::ScopeSet = afd_auth::scope::ScopeSet::from_scopes(&[
    afd_auth::scope::Scope::FleetRead,
    afd_auth::scope::Scope::FleetWrite,
    afd_auth::scope::Scope::FleetAdmin,
]);

/// The collection path, under the workspace the fixture owns.
fn collection() -> String {
    format!("/v1/workspaces/{OWNED_WORKSPACE}/fleets")
}

/// The item path, under the workspace the fixture owns.
fn item() -> String {
    format!("/v1/workspaces/{OWNED_WORKSPACE}/fleets/{FLEET}")
}

/// One fully authorised request, so what answers is the input rule under test.
async fn authorised(method: Method, path: &str, body: &str) -> axum::response::Response {
    let router = Fleet::new()
        .with_person(TENANT_KEY, SUBJECT, EVERY_FLEET_SCOPE)
        .router();
    harness::send(&router, method, path, Some(TENANT_KEY), body).await
}

/// Reads a problem document's `detail` back.
async fn detail_of(response: axum::response::Response) -> String {
    let document = harness::json_body(response).await;
    document
        .get("detail")
        .and_then(Value::as_str)
        .expect("every refusal carries a detail")
        .to_owned()
}

/// Asserts the request got PAST the edge and reached the store.
async fn assert_reached_the_verb(response: axum::response::Response, case: &str) {
    assert_eq!(
        response.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "{case}: only the verb answers with the datastore's refusal"
    );
    assert_eq!(
        detail_of(response).await,
        afd_fleet_lifecycle::error::detail::DATABASE_UNAVAILABLE,
        "{case}: the refusal is the store's, not a layer's"
    );
}

#[tokio::test]
async fn the_retired_cursor_parameter_is_reported_rather_than_ignored() {
    // Ignoring it would leave a caller reading page one forever with nothing
    // saying why, which is the whole reason it is refused.
    let path = format!("{}?cursor=1744000000000%3A019abc", collection());
    let response = authorised(Method::GET, &path, "").await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        detail_of(response).await,
        afd_api::handler::fleet::DETAIL_RETIRED_CURSOR
    );
}

#[tokio::test]
async fn a_cursor_this_daemon_never_issued_is_refused() {
    for token in ["garbage", "notanumber:019abc", "1744000000000:not-a-uuid"] {
        let path = format!("{}?starting_after={token}", collection());
        let response = authorised(Method::GET, &path, "").await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{token}");
        assert_eq!(
            detail_of(response).await,
            afd_api::handler::fleet::DETAIL_INVALID_CURSOR,
            "{token}"
        );
    }
}

#[tokio::test]
async fn an_unreadable_limit_reads_as_the_default_rather_than_a_refusal() {
    // `list.zig`'s leniency, kept: this list absorbs a bad limit where the
    // workspace directory answers a 400. Each is its own handler's vocabulary,
    // and a client sitting on either would change class if they were merged.
    let path = format!("{}?limit=not-a-number", collection());
    let response = authorised(Method::GET, &path, "").await;

    assert_reached_the_verb(response, "a lenient limit still reaches the walk").await;
}

#[tokio::test]
async fn an_install_must_name_exactly_one_library_tier() {
    let cases = [
        ("{}", afd_api::handler::fleet::DETAIL_LIBRARY_REQUIRED),
        (
            r#"{"platform_library_id":"a","tenant_library_id":"01924f4e-0000-7000-8000-00000000f1ee"}"#,
            afd_api::handler::fleet::DETAIL_LIBRARY_AMBIGUOUS,
        ),
        (
            r#"{"tenant_library_id":"not-a-uuid"}"#,
            afd_api::handler::fleet::DETAIL_TENANT_LIBRARY_ID,
        ),
    ];
    for (body, expected) in cases {
        let response = authorised(Method::POST, &collection(), body).await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(detail_of(response).await, expected, "{body}");
    }
}

#[tokio::test]
async fn an_install_refuses_a_name_it_could_not_store() {
    let body = r#"{"platform_library_id":"daily-digest","name":"Not A Slug"}"#;
    let response = authorised(Method::POST, &collection(), body).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        detail_of(response).await,
        afd_api::handler::fleet::DETAIL_NAME_INVALID
    );
}

#[tokio::test]
async fn an_empty_patch_answers_without_touching_a_row() {
    // The one success this suite can prove with no datastore in it, and it is a
    // real one: a dashboard saving an untouched form must not take a row lock.
    let response = authorised(Method::PATCH, &item(), "").await;

    assert_eq!(response.status(), StatusCode::OK);
    let document = harness::json_body(response).await;
    assert_eq!(
        document.get("fleet_id").and_then(Value::as_str),
        Some(FLEET)
    );
    assert!(
        document.get("config_revision").is_some_and(Value::is_null),
        "the no-op reports no revision, and the key stays on the wire"
    );
    assert!(
        document.get("etag").is_none(),
        "nothing was written, so there is no new version to name"
    );
}

#[tokio::test]
async fn a_conditional_patch_that_asks_for_nothing_is_refused() {
    // An `If-Match` with no field to write expects a compare that cannot
    // happen; answering 200 would tell the caller their edit landed.
    let router = Fleet::new()
        .with_person(TENANT_KEY, SUBJECT, EVERY_FLEET_SCOPE)
        .router();
    let response = harness::send_with_headers(
        &router,
        Method::PATCH,
        &item(),
        Some(TENANT_KEY),
        "",
        &[(http::header::IF_MATCH, "\"deadbeef\"")],
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        detail_of(response).await,
        afd_api::handler::fleet::detail::DETAIL_CONDITIONAL_EMPTY
    );
}

#[tokio::test]
async fn a_patch_naming_both_configuration_sources_is_refused_at_the_door() {
    // Both drive `core.fleets.config_json`, so there is no answer to which one
    // wins — and the type downstream cannot hold the ambiguity at all.
    let body = r#"{"config_json":"{}","trigger_markdown":"---\nname: probe\n---\n"}"#;
    let response = authorised(Method::PATCH, &item(), body).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        detail_of(response).await,
        afd_api::handler::fleet::detail::DETAIL_CONFIG_AMBIGUOUS
    );
}

#[tokio::test]
async fn a_patch_may_not_forge_a_status_the_platform_owns() {
    // `paused` belongs to the anomaly gate. Accepting it here would let a
    // caller manufacture a system-halt provenance on their own fleet.
    for spelling in ["paused", "installing", "deleted", ""] {
        let body = format!("{{\"status\":\"{spelling}\"}}");
        let response = authorised(Method::PATCH, &item(), &body).await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{spelling}");
        assert_eq!(
            detail_of(response).await,
            afd_api::handler::fleet::detail::DETAIL_STATUS_INVALID,
            "{spelling}"
        );
    }
}

#[tokio::test]
async fn a_body_that_is_not_json_is_refused_before_the_store() {
    for (method, path) in [(Method::POST, collection()), (Method::PATCH, item())] {
        let response = authorised(method.clone(), &path, "{not json").await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{method}");
    }
}
