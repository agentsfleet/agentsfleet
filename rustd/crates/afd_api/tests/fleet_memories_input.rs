//! What the memories surface accepts, and what it refuses before a statement.
//!
//! The sibling of `fleet_memories.rs`, split the way `workspace_fleets_input`
//! splits from `workspace_fleets`: that suite proves the guard, the rungs and
//! the ownership layer, and this one proves the VALUES — a query string, a page
//! size, a continuation token, a memory key.
//!
//! # Every case here ends in one of two answers
//!
//! A 400, because the request could never name a row; or a 503, because it
//! could and the harness's Postgres is not there. The second is the useful
//! half: it is what proves a well-formed request got past every gate in front
//! of the store rather than being refused by one of them silently.
//!
//! The parsing itself is unit-tested beside the code, in
//! `handler/fleet/memory_request/tests.rs`. What this adds is that the refusal
//! survives the whole layer stack and arrives as the envelope and status a
//! client branches on.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use crate::harness;

use afd_auth::scope::{Scope, ScopeSet};
use afd_core::error_code;
use http::{Method, StatusCode};
use serde_json::Value;

use self::harness::{Fleet, OWNED_WORKSPACE};

/// A tenant api-key, shaped as the authenticator classifies one.
const TENANT_KEY: &str = "agt_tdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef";

/// The subject the fixture credential resolves to.
const SUBJECT: &str = "user_2memories";

/// A well-formed fleet identifier the fixture addresses.
const FLEET: &str = "01924f4e-0000-7000-8000-00000000fee7";

/// A memory key the fixture addresses.
const KEY: &str = "wrong-lesson";

/// The rung the read takes.
const FLEET_READ: ScopeSet = ScopeSet::from_scopes(&[Scope::FleetRead]);

/// The rung the forget takes.
const FLEET_WRITE: ScopeSet = ScopeSet::from_scopes(&[Scope::FleetWrite]);

/// The longest key that could ever have been stored.
const MAX_KEY_LEN: usize = 255;

/// A page size far past what any page will serve.
///
/// Named rather than spelled inline so the assertion says what it is asking
/// for: not "one hundred thousand", but "more than this surface will ever
/// give".
const OVER_THE_CEILING: i64 = 100_000;

/// One request at a fresh router holding one scoped person.
async fn send(scopes: ScopeSet, method: Method, path: &str) -> axum::response::Response {
    let router = Fleet::new()
        .with_person(TENANT_KEY, SUBJECT, scopes)
        .router();
    harness::send(&router, method, path, Some(TENANT_KEY), "").await
}

/// The collection, under the workspace the fixture owns.
fn collection() -> String {
    format!("/v1/workspaces/{OWNED_WORKSPACE}/fleets/{FLEET}/memories")
}

/// The collection with a query string appended.
fn listing(query: &str) -> String {
    format!("{}?{query}", collection())
}

/// One entry, under the same workspace and fleet.
fn item(key: &str) -> String {
    format!("/v1/workspaces/{OWNED_WORKSPACE}/fleets/{FLEET}/memories/{key}")
}

/// One fully authorised read.
async fn reading(path: &str) -> axum::response::Response {
    send(FLEET_READ, Method::GET, path).await
}

/// One fully authorised forget.
async fn forgetting(path: &str) -> axum::response::Response {
    send(FLEET_WRITE, Method::DELETE, path).await
}

/// Reads a problem document's `error_code` back.
async fn code_of(response: axum::response::Response) -> String {
    let document = harness::json_body(response).await;
    document
        .get("error_code")
        .and_then(Value::as_str)
        .expect("every refusal carries a registry code")
        .to_owned()
}

/// A fleet id that is not an identifier never reaches a statement.
///
/// Refused rather than passed to the `::uuid` cast, which keeps every error
/// from below a genuine datastore fault.
#[tokio::test]
async fn a_fleet_id_that_is_not_an_identifier_is_refused() {
    let path = format!("/v1/workspaces/{OWNED_WORKSPACE}/fleets/not-a-uuid/memories");
    let response = reading(&path).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        code_of(response).await,
        error_code::INVALID_REQUEST.as_str()
    );
}

/// A limit that is not a positive integer is refused, never coerced.
#[tokio::test]
async fn a_limit_that_is_not_a_positive_integer_is_refused() {
    for query in ["limit=0", "limit=-5", "limit=abc", "limit=1.5"] {
        let response = reading(&listing(query)).await;
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "{query} is not a page size"
        );
    }
}

/// The limit is judged before the cursor.
///
/// A request wrong in both halves answers byte for byte what a request wrong
/// in the limit ALONE answers — which is the whole claim, and it is made here
/// rather than in the parser's unit tests because only a rendered response
/// carries the sentence that distinguishes the two refusals. Without this, a
/// reordering of the two checks would change what an operator is told and no
/// test would notice.
#[tokio::test]
async fn the_limit_is_judged_before_the_cursor() {
    let both_wrong = reading(&listing("limit=0&starting_after=not-a-cursor")).await;
    let only_limit = reading(&listing("limit=0")).await;

    assert_eq!(both_wrong.status(), StatusCode::BAD_REQUEST);
    assert_eq!(only_limit.status(), StatusCode::BAD_REQUEST);

    // `request_id` is minted per response, so the documents are compared on
    // everything a caller acts on rather than on the whole body.
    let (wrong, limit) = (
        harness::json_body(both_wrong).await,
        harness::json_body(only_limit).await,
    );
    for field in ["error_code", "title", "detail"] {
        assert_eq!(
            wrong.get(field),
            limit.get(field),
            "a doubly-wrong request is told about the cursor, not the limit ({field})"
        );
    }
}

/// A limit over the ceiling is CLAMPED, not refused.
///
/// This surface's own vocabulary, kept from `parseLimitQs`: the workspace
/// directory answers a 400 for the same ask, and a client sitting on either
/// would change class if the two were made to agree.
#[tokio::test]
async fn a_limit_over_the_ceiling_is_clamped_rather_than_refused() {
    let response = reading(&listing(&format!("limit={OVER_THE_CEILING}"))).await;
    assert_ne!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "an over-large page is served small, not refused"
    );
}

/// A continuation this walk did not issue is refused, never read as page one.
///
/// Silently serving page one is how a client keeps a paging bug for months.
#[tokio::test]
async fn a_continuation_this_walk_did_not_issue_is_refused() {
    for token in [
        "not-a-cursor",
        "abc:key",
        "1700000000000:",
        "s:cHJvZA:019abc",
    ] {
        let response = reading(&listing(&format!("starting_after={token}"))).await;
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "{token} is not this walk's cursor"
        );
    }
}

/// A query string this daemon cannot decode fails the whole request.
///
/// The whole string, not only the parameter this read wanted: `req.query()`
/// parses in one pass and fails the request on a bad escape anywhere in it.
#[tokio::test]
async fn a_query_string_with_a_malformed_escape_is_refused() {
    for query in ["query=100%", "query=a%2", "limit=10&junk=%2"] {
        let response = reading(&listing(query)).await;
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "{query} does not decode"
        );
    }
}

/// A well-formed listing reaches the store, and the outage is a 503.
///
/// RULE ECL: a datastore that will not answer is a TRANSPORT-class refusal
/// wherever it is raised, so a client backs off rather than treating it as its
/// own request being wrong. Every list shape is here, because each of the three
/// runs a different statement and a typo in one would otherwise only surface
/// against live Postgres.
#[tokio::test]
async fn a_well_formed_listing_reaches_the_store_and_reports_the_outage() {
    for query in [
        "",
        "limit=5",
        "category=core",
        "query=monday",
        "query=hello+world",
        "starting_after=1700000000000:goal:current",
    ] {
        let response = reading(&listing(query)).await;
        assert_eq!(
            response.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "{query} must reach the store"
        );
        assert_eq!(
            code_of(response).await,
            error_code::INTERNAL_DB_UNAVAILABLE.as_str(),
            "{query}: the refusal is the datastore's"
        );
    }
}

/// A memory key with a malformed escape never reaches the database lookup.
///
/// The one place axum's own decoding would have absorbed the refusal: it leaves
/// `%2` as two literal characters and would have gone looking for a key spelled
/// that way, answering 404 where this answers 400.
#[tokio::test]
async fn a_memory_key_with_a_malformed_escape_is_refused() {
    for key in ["bad%2", "bad%", "bad%zz"] {
        let response = forgetting(&item(key)).await;
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "{key} does not decode"
        );
        assert_eq!(
            code_of(response).await,
            error_code::INVALID_REQUEST.as_str(),
            "{key}: a malformed key is a malformed request"
        );
    }
}

/// A memory key longer than one that could be stored is refused.
#[tokio::test]
async fn a_memory_key_over_its_bound_is_refused() {
    let over = "k".repeat(MAX_KEY_LEN + 1);
    let response = forgetting(&item(&over)).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// A well-formed forget reaches the store, and the outage is a 503.
///
/// Including an ENCODED separator, which is the shape that proves the key is
/// read from the raw path: `style%2Fkey` is ONE segment naming `style/key`, and
/// a reader that had split on the decoded slash would never have found the row.
/// A raw `+` is here for the mirror reason — it is a literal plus in a path and
/// a space in a query string, and only one of the two decoders may see it.
#[tokio::test]
async fn a_well_formed_forget_reaches_the_store_and_reports_the_outage() {
    for key in [KEY, "style%2Fkey", "path+plus", "path%20space", "100%25"] {
        let response = forgetting(&item(key)).await;
        assert_eq!(
            response.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "{key} must reach the store"
        );
        assert_eq!(
            code_of(response).await,
            error_code::INTERNAL_DB_UNAVAILABLE.as_str(),
            "{key}: the refusal is the datastore's"
        );
    }
}
