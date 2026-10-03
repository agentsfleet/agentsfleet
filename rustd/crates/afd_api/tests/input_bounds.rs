//! Path segments and filters held to a bound declared on a type.
//!
//! Each was a hand-written length check, or no check at all; each is now a
//! bound garde proves before the store is asked anything. This suite pins the
//! edge of every one through the production router: one past the bound is a
//! `400` naming it, and the bound itself gets past the parameter check. The
//! sentences are spelled out rather than imported, so the test and the code
//! under test cannot agree with each other by construction.
#![cfg(feature = "test-util")]

use crate::harness;

use afd_auth::scope::{Scope, ScopeSet};
use http::{Method, StatusCode};
use serde_json::Value;

use self::harness::{Fleet, OWNED_WORKSPACE};

/// A tenant api-key, shaped as the authenticator classifies one.
const TENANT_KEY: &str = "agt_tb0deb0deb0deb0deb0deb0deb0deb0deb0deb0deb0deb0deb0deb0deb0deb0de";

/// The subject the fixture credential resolves to.
const SUBJECT: &str = "user_2input_bounds";

/// A well-formed fleet identifier the fixture addresses.
const FLEET: &str = "01924f4e-0000-7000-8000-00000000b0de";

/// Every rung a route below declares, so nothing is refused by a rung.
const EVERY_SCOPE: ScopeSet = ScopeSet::from_scopes(&[
    Scope::FleetRead,
    Scope::FleetWrite,
    Scope::ApprovalRead,
    Scope::PlatformKeyAdmin,
]);

/// The malformed-request code.
const INVALID_REQUEST: &str = "UZ-REQ-001";

/// The longest memory key, in decoded bytes.
const MAX_KEY_LEN: usize = 255;

/// The longest event identifier a path may name.
const EVENT_ID_MAX_LEN: usize = 256;

/// The longest provider a platform-key path may name.
const KEY_PROVIDER_MAX_BYTES: usize = 32;

/// The longest actor glob or prefix.
const MAX_ACTOR_FILTER_BYTES: usize = 256;

/// The longest gate-kind filter.
const MAX_GATE_KIND_BYTES: usize = 64;

/// The catalogue's provider column, which the filter shares.
const CATALOGUE_PROVIDER_BYTES: usize = 64;

/// One request through a router holding one fully scoped person.
async fn send(method: Method, path: &str) -> (StatusCode, Option<String>, Option<String>) {
    let router = Fleet::new()
        .with_person(TENANT_KEY, SUBJECT, EVERY_SCOPE)
        .router();
    let response = harness::send(&router, method, path, Some(TENANT_KEY), "").await;
    let status = response.status();
    let document = harness::json_body(response).await;
    let field = |key: &str| document.get(key).and_then(Value::as_str).map(str::to_owned);
    (status, field("error_code"), field("detail"))
}

/// Asserts `path` is refused with `code` and `sentence`.
async fn assert_refused(method: Method, path: &str, code: &str, sentence: &str) {
    let expected = (
        StatusCode::BAD_REQUEST,
        Some(code.to_owned()),
        Some(sentence.to_owned()),
    );
    assert_eq!(send(method, path).await, expected, "{path}");
}

/// Asserts `path` got past every parameter check.
async fn assert_accepted(method: Method, path: &str) {
    let (status, _code, detail) = send(method, path).await;
    assert_ne!(status, StatusCode::BAD_REQUEST, "{path}: {detail:?}");
}

fn fleet_path(rest: &str) -> String {
    format!("/v1/workspaces/{OWNED_WORKSPACE}/fleets/{FLEET}/{rest}")
}

#[tokio::test]
async fn test_path_segments_are_bounded_on_their_path_type() {
    // The memory key is measured DECODED: 256 bytes of key, whatever the URL.
    let key = "k".repeat(MAX_KEY_LEN + 1);
    assert_refused(
        Method::DELETE,
        &fleet_path(&format!("memories/{key}")),
        INVALID_REQUEST,
        "memory key must be 1..255 chars",
    )
    .await;
    let encoded = "%6B".repeat(MAX_KEY_LEN);
    assert_accepted(Method::DELETE, &fleet_path(&format!("memories/{encoded}"))).await;

    // An over-long event id is told its bound, on both paths that name one.
    let event = "e".repeat(EVENT_ID_MAX_LEN + 1);
    let event_sentence = "event_id must be 1-256 bytes";
    for path in [
        fleet_path(&format!("events/{event}")),
        fleet_path(&format!("events/{event}/tool-calls/fence:1")),
    ] {
        assert_refused(Method::GET, &path, INVALID_REQUEST, event_sentence).await;
    }
    let at_bound = "e".repeat(EVENT_ID_MAX_LEN);
    assert_accepted(Method::GET, &fleet_path(&format!("events/{at_bound}"))).await;

    // The platform-key path is held to the body's provider bound.
    let provider = "p".repeat(KEY_PROVIDER_MAX_BYTES + 1);
    assert_refused(
        Method::DELETE,
        &format!("/v1/admin/platform-keys/{provider}"),
        INVALID_REQUEST,
        "provider must be 1–32 chars",
    )
    .await;
}

#[tokio::test]
async fn test_unbounded_filters_now_refuse_oversize() {
    let events = format!("/v1/workspaces/{OWNED_WORKSPACE}/events");
    let over = "a".repeat(MAX_ACTOR_FILTER_BYTES + 1);
    let cases = [
        ("actor", "actor must be at most 256 bytes"),
        ("actor_prefix", "actor_prefix must be at most 256 bytes"),
    ];
    for (filter, sentence) in cases {
        let path = format!("{events}?{filter}={over}");
        assert_refused(Method::GET, &path, INVALID_REQUEST, sentence).await;
        let at_bound = "a".repeat(MAX_ACTOR_FILTER_BYTES);
        assert_accepted(Method::GET, &format!("{events}?{filter}={at_bound}")).await;
    }

    let approvals = format!("/v1/workspaces/{OWNED_WORKSPACE}/approvals");
    let kind = "g".repeat(MAX_GATE_KIND_BYTES + 1);
    assert_refused(
        Method::GET,
        &format!("{approvals}?gate_kind={kind}"),
        INVALID_REQUEST,
        "gate_kind must be at most 64 bytes",
    )
    .await;
    let at_bound = "g".repeat(MAX_GATE_KIND_BYTES);
    assert_accepted(Method::GET, &format!("{approvals}?gate_kind={at_bound}")).await;
}

#[tokio::test]
async fn test_provider_filter_shares_the_catalogue_bound() {
    let over = "p".repeat(CATALOGUE_PROVIDER_BYTES + 1);
    assert_refused(
        Method::GET,
        &format!("/v1/models?provider={over}"),
        "UZ-LIBRARY-003",
        "provider must be at most 64 bytes once normalized, and valid UTF-8",
    )
    .await;
    let at_bound = "p".repeat(CATALOGUE_PROVIDER_BYTES);
    assert_accepted(Method::GET, &format!("/v1/models?provider={at_bound}")).await;
}
