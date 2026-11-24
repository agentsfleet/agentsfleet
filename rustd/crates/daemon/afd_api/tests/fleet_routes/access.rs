//! What a fleet holds is a person's to give and to take back: an access card
//! is approved only by a signed-in person, and a revoke on the route ends the
//! grant.

use super::fixture::{Fixture, SESSION};
use super::{json_body, send};
use af_core::error_code;
use af_core::id::Uuid7;
use afd_api_wire::grant::status;
use afd_approval::{GrantKind, IntegrationGrants, Requested, Wanted};
use axum::Router;
use http::{Method, StatusCode};
use serde_json::Value;

/// The workspace secret the fleet asks for.
const SENTRY: &str = "sentry";

/// A fleet created through `router` as `bearer`, its card for [`SENTRY`]
/// raised on its first turn, answering the fleet, the grant and the card.
async fn asked(f: &Fixture, router: &Router, bearer: &str) -> (Uuid7, Uuid7, Uuid7) {
    let created = f.create_fleet(router, bearer).await;
    let id = |pointer: &str| {
        Uuid7::parse(
            created
                .pointer(pointer)
                .and_then(Value::as_str)
                .expect(pointer),
        )
        .expect("a UUIDv7")
    };
    let (fleet, turn) = (id("/fleet/id"), id("/accepted/turn_id"));
    let requested = IntegrationGrants::new(f.db.clone(), af_crypto::entropy::Entropy::new())
        .request(
            &f.workspace,
            &fleet,
            Wanted {
                name: SENTRY,
                kind: GrantKind::Secret,
                scope: None,
                turn: &turn,
            },
            af_core::clock::now(),
        )
        .await
        .expect("the request raises a card");
    let Requested::Raised { grant, gate } = requested else {
        panic!("a first request raises a card: {requested:?}");
    };
    (fleet, grant, gate)
}

/// The grant's stored status.
async fn grant_status(f: &Fixture, grant: &Uuid7) -> String {
    sqlx::query_scalar("SELECT status FROM connector.integration_grants WHERE id = $1")
        .bind(grant)
        .fetch_one(&mut *f.db.acquire().await.expect("a connection"))
        .await
        .expect("the grant reads")
}

/// An API key on the approve route is refused with 403, and the grant stays
/// pending; a signed-in person's approval then lands.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_api_key_cannot_approve_a_grant() {
    let f = Fixture::create().await;
    let key = f.router();
    let (_fleet, grant, gate) = asked(&f, &key, &f.token).await;
    let approve = format!("/v1/workspaces/{}/approvals/{gate}/approve", f.workspace);

    for decision in ["approve", "deny"] {
        let path = format!("/v1/workspaces/{}/approvals/{gate}/{decision}", f.workspace);
        let refused = send(&key, Method::POST, &path, Some(&f.token), "").await;
        assert_eq!(refused.status(), StatusCode::FORBIDDEN, "{decision}");
        assert_eq!(
            json_body(refused).await["error_code"],
            error_code::AUTH_FORBIDDEN.as_str()
        );
    }
    assert_eq!(grant_status(&f, &grant).await, status::PENDING);

    let person = f.signed_in(f.owner.as_str());
    let approved = send(&person, Method::POST, &approve, Some(SESSION), "").await;
    assert_eq!(approved.status(), StatusCode::OK);
    assert_eq!(grant_status(&f, &grant).await, status::APPROVED);
    f.cleanup().await;
}

/// A signed-in person revokes an approved grant on the route: 204, and the
/// grant is revoked; an API key is refused with 403 and changes nothing.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_person_revokes_a_grant_on_the_route() {
    let f = Fixture::create().await;
    let person = f.signed_in(f.owner.as_str());
    let (fleet, grant, gate) = asked(&f, &person, SESSION).await;
    let approve = format!("/v1/workspaces/{}/approvals/{gate}/approve", f.workspace);
    let approved = send(&person, Method::POST, &approve, Some(SESSION), "").await;
    assert_eq!(approved.status(), StatusCode::OK);
    let revoke = format!("{}/integration-grants/{grant}", f.fleet(fleet.as_str()));

    let refused = send(&f.router(), Method::DELETE, &revoke, Some(&f.token), "").await;
    assert_eq!(refused.status(), StatusCode::FORBIDDEN);
    assert_eq!(grant_status(&f, &grant).await, status::APPROVED);

    let revoked = send(&person, Method::DELETE, &revoke, Some(SESSION), "").await;
    assert_eq!(revoked.status(), StatusCode::NO_CONTENT);
    assert_eq!(grant_status(&f, &grant).await, status::REVOKED);
    let listed = send(
        &person,
        Method::GET,
        &format!("{}/integration-grants", f.fleet(fleet.as_str())),
        Some(SESSION),
        "",
    )
    .await;
    assert_eq!(listed.status(), StatusCode::OK);
    let listed = json_body(listed).await;
    assert_eq!(listed["items"][0]["name"], SENTRY);
    assert_eq!(listed["items"][0]["status"], status::REVOKED);
    f.cleanup().await;
}
