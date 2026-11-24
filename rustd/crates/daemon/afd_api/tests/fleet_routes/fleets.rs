//! A fleet is shared from its first message, and only a signed-in person
//! opens its network.

use super::fixture::{FIRST_MESSAGE, Fixture, SESSION};
use super::{json_body, send, send_with_headers};
use af_core::error_code;
use axum::Router;
use http::{Method, StatusCode, header};
use serde_json::{Value, json};

/// The fleet's id and strong tag, as the create answered them.
fn created(body: &Value) -> (String, String) {
    let fleet = body["fleet"]["id"]
        .as_str()
        .expect("the fleet's id")
        .to_owned();
    let etag = body["fleet"]["etag"]
        .as_str()
        .expect("the fleet's tag")
        .to_owned();
    (fleet, etag)
}

/// `PATCH F` with `body` under `etag`, as `bearer`.
async fn patch(
    f: &Fixture,
    router: &Router,
    bearer: &str,
    fleet: &str,
    etag: &str,
    body: &Value,
) -> axum::response::Response {
    send_with_headers(
        router,
        Method::PATCH,
        &f.fleet(fleet),
        Some(bearer),
        &body.to_string(),
        &[(header::IF_MATCH, etag)],
    )
    .await
}

/// The stored network toggles of `fleet`.
async fn network(f: &Fixture, fleet: &str) -> (String, bool) {
    sqlx::query_as("SELECT http_egress, package_domains FROM fleet.fleets WHERE id = $1::uuid")
        .bind(fleet)
        .fetch_one(&mut *f.db.acquire().await.expect("a connection"))
        .await
        .expect("the fleet row reads")
}

/// Nothing is private: the moment a person's first message makes a fleet,
/// another member of the account reads it and its thread, before any run.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_second_member_reads_a_new_fleet_at_once() {
    let f = Fixture::create().await;
    let owner = f.signed_in(f.owner.as_str());
    let body = f.create_fleet(&owner, SESSION).await;
    let (fleet, _) = created(&body);
    assert_eq!(body["fleet"]["status"], "active");
    assert_eq!(body["replayed"], false);

    let member = f.signed_in(f.member.as_str());
    let read = send(&member, Method::GET, &f.fleet(&fleet), Some(SESSION), "").await;
    assert_eq!(read.status(), StatusCode::OK);
    assert_eq!(json_body(read).await["id"], json!(fleet));
    let thread = send(
        &member,
        Method::GET,
        &format!("{}/messages", f.fleet(&fleet)),
        Some(SESSION),
        "",
    )
    .await;
    assert_eq!(thread.status(), StatusCode::OK);
    let thread = json_body(thread).await;
    assert!(
        thread.to_string().contains(FIRST_MESSAGE),
        "the member reads the first message: {thread}"
    );
    f.cleanup().await;
}

/// A signed-in person opens the fleet's network in one change, and the row
/// the next run's policy reads holds it.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_person_opens_a_fleets_network() {
    let f = Fixture::create().await;
    let person = f.signed_in(f.owner.as_str());
    let (fleet, etag) = created(&f.create_fleet(&person, SESSION).await);
    assert_eq!(
        network(&f, &fleet).await,
        ("restricted".to_owned(), false),
        "a new fleet is private"
    );

    let body = json!({"http_egress": "public", "package_domains": true});
    let opened = patch(&f, &person, SESSION, &fleet, &etag, &body).await;
    assert_eq!(opened.status(), StatusCode::OK);
    let opened = json_body(opened).await;
    assert_eq!(opened["http_egress"], "public");
    assert_eq!(opened["package_domains"], true);
    assert_eq!(network(&f, &fleet).await, ("public".to_owned(), true));
    f.cleanup().await;
}

/// An API key changes a fleet's name, but never its network: 403, and the
/// fleet is unchanged.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_api_key_cannot_open_a_fleets_network() {
    let f = Fixture::create().await;
    let key = f.router();
    let (fleet, etag) = created(&f.create_fleet(&key, &f.token).await);

    for body in [
        json!({"http_egress": "public"}),
        json!({"package_domains": true}),
        json!({"name": "renamed-by-key", "http_egress": "public"}),
    ] {
        let refused = patch(&f, &key, &f.token, &fleet, &etag, &body).await;
        assert_eq!(refused.status(), StatusCode::FORBIDDEN, "{body}");
        assert_eq!(
            json_body(refused).await["error_code"],
            error_code::AUTH_FORBIDDEN.as_str()
        );
    }
    assert_eq!(network(&f, &fleet).await, ("restricted".to_owned(), false));

    let renamed = patch(
        &f,
        &key,
        &f.token,
        &fleet,
        &etag,
        &json!({"name": "renamed"}),
    )
    .await;
    assert_eq!(
        renamed.status(),
        StatusCode::OK,
        "a name is not the network"
    );
    f.cleanup().await;
}
