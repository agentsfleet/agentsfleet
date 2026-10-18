//! Creator privacy and membership revocation through the served draft routes.

use super::{fixture::Fixture, json_body, mint, send, send_with_headers};
use afd_auth::{
    Scope, credential::Presented, directory::Digest, principal::Subject, scope::ScopeSet,
};
use afd_core::{
    error_code::{AUTHORING_CURSOR, AUTHORING_NOT_FOUND},
    id::Uuid7,
};
use axum::Router;
use http::{Method, StatusCode, header};
use serde_json::{Value, json};

/// A joined member of the owner's account, in an account of their own.
const MEMBER: &str = "\
WITH tenant AS (
 INSERT INTO identity.tenants(id,name,created_at,updated_at) VALUES ($1::uuid,'authoring-member',now(),now())
), person AS (
 INSERT INTO identity.users(id,tenant_id,oidc_subject,email,created_at,updated_at)
 VALUES ($2::uuid,$1::uuid,$2,$2,now(),now())
), joined AS (
 INSERT INTO identity.memberships(id,tenant_id,user_id,role,created_at)
 SELECT $5::uuid,$6::uuid,$2::uuid,'member',now()
)
INSERT INTO identity.api_keys(id,tenant_id,name,description,key_hash,created_by_subject,created_at,updated_at)
VALUES ($3::uuid,$1::uuid,'member','',$4,$2,now(),now())";

struct Member {
    id: Uuid7,
    token: String,
}

async fn member(f: &mut Fixture) -> Member {
    let (tenant, id) = (mint(), mint());
    f.own_tenant(&tenant);
    let token = format!("agt_t{}{}", mint(), mint()).replace('-', "");
    let digest = Digest::of(&Presented::new(&token).expect("member token"));
    sqlx::query(MEMBER)
        .bind(tenant.as_str())
        .bind(id.as_str())
        .bind(mint().as_str())
        .bind(digest.as_str())
        .bind(mint().as_str())
        .bind(f.tenant.as_str())
        .execute(&mut *f.db.acquire().await.expect("seed connection"))
        .await
        .expect("seed joined member with a real key");
    Member { id, token }
}

/// One router serving both people, each with every scope.
fn router(f: &Fixture, member: &Member) -> Router {
    let services = f.services(&Scope::ALL);
    let _granted = services.capabilities().clone().with(
        &Subject::new(member.id.as_str()).expect("member subject"),
        ScopeSet::from_scopes(&Scope::ALL),
    );
    services.router()
}

async fn create(router: &Router, f: &Fixture, token: &str) -> (String, String) {
    let created = send(
        router,
        Method::POST,
        &f.collection(),
        Some(token),
        &f.request(&mint()).to_string(),
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let tag = created.headers()[header::ETAG]
        .to_str()
        .expect("strong tag")
        .to_owned();
    let item = created.headers()[header::LOCATION]
        .to_str()
        .expect("item location")
        .to_owned();
    (item, tag)
}

/// Every draft-member verb, as `(method, path suffix, body, If-Match)`.
fn verbs(tag: &str) -> Vec<(Method, &'static str, String, Option<String>)> {
    let message = json!({"message":"Foreign text","operation_id":mint()}).to_string();
    vec![
        (Method::GET, "", String::new(), None),
        (Method::GET, "/messages", String::new(), None),
        (Method::POST, "/messages", message, None),
        (
            Method::PATCH,
            "",
            json!({"title":"Taken"}).to_string(),
            Some(tag.to_owned()),
        ),
        (
            Method::POST,
            "/cancellations",
            json!({"through_sequence":"1"}).to_string(),
            None,
        ),
        (Method::GET, "/events/stream", String::new(), None),
        (Method::DELETE, "", String::new(), None),
    ]
}

async fn assert_hidden(router: &Router, item: &str, token: &str, tag: &str) {
    for (method, suffix, body, tag) in verbs(tag) {
        let path = format!("{item}{suffix}");
        let route = format!("{method} {suffix}");
        let headers: Vec<_> = tag
            .iter()
            .map(|tag| (header::IF_MATCH, tag.as_str()))
            .collect();
        let response = send_with_headers(router, method, &path, Some(token), &body, &headers).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{route}");
        let body: Value = json_body(response).await;
        assert_eq!(body["error_code"], AUTHORING_NOT_FOUND.as_str(), "{route}");
    }
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn foreign_creator_is_hidden_on_every_draft_route_and_changes_nothing() {
    let mut f = Fixture::create().await;
    let member = member(&mut f).await;
    let router = router(&f, &member);
    let (item, tag) = create(&router, &f, &f.token).await;
    assert_hidden(&router, &item, &member.token, &tag).await;
    let listed = send(
        &router,
        Method::GET,
        &f.collection(),
        Some(&member.token),
        "",
    )
    .await;
    assert_eq!(listed.status(), StatusCode::OK);
    assert_eq!(
        json_body(listed).await["items"],
        json!([]),
        "owner's draft is not listed"
    );
    let own = send(&router, Method::GET, &item, Some(&f.token), "").await;
    assert_eq!(own.status(), StatusCode::OK);
    assert_eq!(
        own.headers()[header::ETAG],
        tag.as_str(),
        "nothing was written"
    );
    let messages = send(
        &router,
        Method::GET,
        &format!("{item}/messages"),
        Some(&f.token),
        "",
    )
    .await;
    let items = json_body(messages).await["items"].clone();
    assert_eq!(
        items.as_array().map(Vec::len),
        Some(1),
        "no foreign message or Stop"
    );
    assert_eq!(items[0]["state"], "queued");
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn revoked_member_loses_their_own_draft_on_the_next_request() {
    let mut f = Fixture::create().await;
    let member = member(&mut f).await;
    let router = router(&f, &member);
    let (item, tag) = create(&router, &f, &member.token).await;
    let before = send(&router, Method::GET, &item, Some(&member.token), "").await;
    assert_eq!(before.status(), StatusCode::OK);
    sqlx::query("DELETE FROM core.memberships WHERE tenant_id=$1::uuid AND user_id=$2::uuid")
        .bind(f.tenant.as_str())
        .bind(member.id.as_str())
        .execute(&mut *f.db.acquire().await.expect("revocation connection"))
        .await
        .expect("commit membership revocation");
    assert_hidden(&router, &item, &member.token, &tag).await;
    let listed = send(
        &router,
        Method::GET,
        &f.collection(),
        Some(&member.token),
        "",
    )
    .await;
    assert_eq!(listed.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        json_body(listed).await["error_code"],
        AUTHORING_NOT_FOUND.as_str()
    );
    let created = send(
        &router,
        Method::POST,
        &f.collection(),
        Some(&member.token),
        &f.request(&mint()).to_string(),
    )
    .await;
    assert_eq!(
        created.status(),
        StatusCode::NOT_FOUND,
        "no new private state either"
    );
    let kept: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM core.authoring_drafts WHERE workspace_id=$1::uuid",
    )
    .bind(f.workspace.as_str())
    .fetch_one(&mut *f.db.acquire().await.expect("count connection"))
    .await
    .expect("owned drafts");
    assert_eq!(
        kept, 1,
        "revocation hides the draft; it neither deletes nor adds one"
    );
    f.cleanup().await;
}

/// A continuation is bound to the person, the walk and the page size it was
/// issued for. Presented by another member of the same workspace, at another
/// page size, or to another collection, the router refuses it as a cursor —
/// never as a page of someone else's drafts.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_cursor_presented_outside_its_walk_is_refused_through_the_router() {
    let mut f = Fixture::create().await;
    let member = member(&mut f).await;
    let router = router(&f, &member);
    let (item, _) = create(&router, &f, &f.token).await;
    create(&router, &f, &f.token).await;
    let first = send(
        &router,
        Method::GET,
        &format!("{}?limit=1", f.collection()),
        Some(&f.token),
        "",
    )
    .await;
    assert_eq!(first.status(), StatusCode::OK);
    let cursor = json_body(first).await["next_cursor"]
        .as_str()
        .expect("a second owner draft remains")
        .to_owned();
    let own = send(
        &router,
        Method::GET,
        &format!("{}?limit=1&starting_after={cursor}", f.collection()),
        Some(&f.token),
        "",
    )
    .await;
    assert_eq!(
        own.status(),
        StatusCode::OK,
        "the owner continues their walk"
    );
    for (label, path, token) in [
        (
            "another member",
            format!("{}?limit=1&starting_after={cursor}", f.collection()),
            member.token.as_str(),
        ),
        (
            "another page size",
            format!("{}?limit=2&starting_after={cursor}", f.collection()),
            f.token.as_str(),
        ),
        (
            "another collection",
            format!("{item}/messages?limit=1&starting_after={cursor}"),
            f.token.as_str(),
        ),
    ] {
        let refused = send(&router, Method::GET, &path, Some(token), "").await;
        assert_eq!(refused.status(), StatusCode::BAD_REQUEST, "{label}");
        let body: Value = json_body(refused).await;
        assert_eq!(body["error_code"], AUTHORING_CURSOR.as_str(), "{label}");
        assert!(body.get("items").is_none(), "{label} leaked a page: {body}");
    }
    f.cleanup().await;
}
