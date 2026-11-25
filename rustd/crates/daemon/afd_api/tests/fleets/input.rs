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
    clippy::indexing_slicing,
    reason = "test target: a step indexes the problem body it just received"
)]

use crate::harness;
use crate::harness::{assert_reached_the_verb, detail_of};

use http::{Method, StatusCode};

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

#[tokio::test]
async fn a_cursor_this_daemon_never_issued_is_refused() {
    // The last is a well-formed text-sort token: a cursor some OTHER list
    // issued, naming a sort this walk on `(created_at, id)` does not have.
    for token in [
        "garbage",
        "notanumber:019abc",
        "1744000000000:not-a-uuid",
        "s:YWJj:01924f4e-0000-7000-8000-00000000f1ee",
    ] {
        let path = format!("{}?starting_after={token}", collection());
        let response = authorised(Method::GET, &path, "").await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{token}");
        assert_eq!(
            detail_of(response).await,
            afd_api_tenant::fleet::DETAIL_INVALID_CURSOR,
            "{token}"
        );
    }
}

#[tokio::test]
async fn an_unreadable_limit_reads_as_the_default_rather_than_a_refusal() {
    // This list is lenient: it absorbs a bad limit where the workspace
    // directory answers a 400. Each is its own handler's vocabulary,
    // and a client sitting on either would change class if they were merged.
    let path = format!("{}?limit=not-a-number", collection());
    let response = authorised(Method::GET, &path, "").await;

    assert_reached_the_verb(response, "a lenient limit still reaches the walk").await;
}

#[tokio::test]
async fn removed_file_configuration_inputs_are_refused() {
    for body in [
        r#"{"config_json":"{}"}"#,
        r#"{"source_markdown":"prose"}"#,
        r#"{"trigger_markdown":"schedule"}"#,
        r#"{"name":null}"#,
        r#"{"status":null}"#,
    ] {
        let response = authorised(Method::PATCH, &item(), body).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{body}");
        let problem = harness::json_body(response).await;
        assert!(problem["error_code"].is_string());
    }
}

#[tokio::test]
async fn a_patch_may_not_forge_a_status_the_platform_owns() {
    for spelling in ["paused", "stopped", "killed", "installing", "deleted", ""] {
        let body = serde_json::json!({"status":spelling}).to_string();
        let response = authorised(Method::PATCH, &item(), &body).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{spelling}");
    }
}

/// A rename is bounded to 1..=64 bytes, checked on the body before the
/// resource tag is asked for: outside the bound the patch is refused with a
/// sentence naming `name`, and at either edge it passes on to the tag.
#[tokio::test]
async fn a_rename_outside_its_bounds_is_refused_before_the_resource_tag() {
    for name in [String::new(), "x".repeat(65), "界".repeat(22)] {
        let body = serde_json::json!({ "name": name }).to_string();
        let response = authorised(Method::PATCH, &item(), &body).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{name}");
        assert_eq!(
            detail_of(response).await,
            "name must be 1 to 64 lower-case letters, digits or hyphens",
            "{name}"
        );
    }
    for name in ["x".to_owned(), "x".repeat(64), "界".repeat(21)] {
        let body = serde_json::json!({ "name": name }).to_string();
        let response = authorised(Method::PATCH, &item(), &body).await;
        assert_eq!(
            response.status(),
            StatusCode::PRECONDITION_REQUIRED,
            "{name}"
        );
    }
}

/// A memory-access change is read strictly: a body that does not parse, a
/// grant that is not a boolean and a field the route does not know are each
/// refused as malformed before the store; a readable change reaches it.
#[tokio::test]
async fn an_unreadable_memory_access_change_is_refused_before_the_store() {
    let path = format!("{}/memory-access", item());
    for body in ["{not json", r#"{"read":"yes"}"#, r#"{"write":true}"#, ""] {
        let response = authorised(Method::PATCH, &path, body).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(
            detail_of(response).await,
            "Malformed memory access body",
            "{body}"
        );
    }
    let response = authorised(Method::PATCH, &path, r#"{"read":true}"#).await;
    assert_reached_the_verb(response, "a readable change reaches the store").await;
}

#[tokio::test]
async fn a_status_patch_requires_the_resource_tag_before_reaching_the_store() {
    let response = authorised(Method::PATCH, &item(), r#"{"status":"archived"}"#).await;
    assert_eq!(response.status(), StatusCode::PRECONDITION_REQUIRED);
    assert_eq!(
        harness::json_body(response).await["error_code"],
        af_core::error_code::SETUP_PRECONDITION.as_str()
    );
}

/// A create names its first message and its retry key in the body: one
/// missing either, or carrying a field a fleet no longer has, is refused
/// before the store.
#[tokio::test]
async fn a_create_without_a_message_or_key_is_refused_before_the_store() {
    let router = Fleet::new()
        .with_person(TENANT_KEY, SUBJECT, EVERY_FLEET_SCOPE)
        .router();
    for invalid in [
        format!(r#"{{"operation_id":"{FLEET}"}}"#),
        r#"{"message":"Triage the failing deploy"}"#.to_owned(),
        format!(r#"{{"message":"","operation_id":"{FLEET}"}}"#),
        format!(r#"{{"message":"hi","operation_id":"{FLEET}","review_digest":"review"}}"#),
        format!(
            r#"{{"message":"hi","operation_id":"{FLEET}","source":{{"kind":"file","path":"setup.json"}}}}"#
        ),
    ] {
        let response = harness::send(
            &router,
            Method::POST,
            &collection(),
            Some(TENANT_KEY),
            &invalid,
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{invalid}");
    }
}

#[tokio::test]
async fn a_body_that_is_not_json_is_refused_before_the_store() {
    let response = authorised(Method::PATCH, &item(), "{not json").await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// A name outside the saved-name bound is refused with a
/// sentence naming `name`, before the store is reached.
#[tokio::test]
async fn fleet_list_refuses_a_name_outside_its_bound() {
    let too_long = "a".repeat(65);
    for name in ["Agent%20Bob", "", too_long.as_str()] {
        let path = format!("{}?name={name}", collection());
        let response = authorised(Method::GET, &path, "").await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{name:?}");
        let detail = detail_of(response).await;
        assert!(detail.starts_with("name "), "{name:?}: {detail}");
    }
    let response = authorised(Method::GET, &format!("{}?name=agentbob", collection()), "").await;
    assert_reached_the_verb(response, "a name inside the bound reaches the store").await;
}
