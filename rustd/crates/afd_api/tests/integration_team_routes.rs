//! Invites and members through the routes, over live Postgres: John invites
//! the stranger, the stranger accepts, John manages the account.
//!
//! The store's own rules are `afd_tenant`'s integration suite. What this adds
//! is the HTTP edge: each route is mounted, reads the caller's account from
//! the path, answers with the documented shape, and refuses with the
//! documented code.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_core::error_code;
use afd_tenant::workspace::access::{ROLE_MEMBER, ROLE_OWNER};
use axum::Router;
use http::{Method, StatusCode};
use serde_json::{Value, json};

use afd_auth::scope::ScopeSet;

use crate::harness::send;
use crate::integration_workspace_members::fixture::{Members, Person, owner_scopes};

const INVITES: &str = "/v1/tenants/me/invites";
const MEMBERS: &str = "/v1/tenants/me/members";
const MINE: &str = "/v1/me/invites";

async fn call(
    router: &Router,
    method: Method,
    path: &str,
    who: &Person,
    body: &str,
) -> (StatusCode, Value) {
    let response = send(router, method, path, Some(&who.token), body).await;
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("a test body is in memory");
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn items(page: &Value) -> Vec<Value> {
    page.get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn code(problem: &Value) -> Option<&str> {
    problem.get("error_code").and_then(Value::as_str)
}

/// A string field of a JSON object, when it holds one.
fn text<'v>(value: &'v Value, key: &str) -> Option<&'v str> {
    value.get(key).and_then(Value::as_str)
}

/// The item in a page whose `key` is `wanted`.
fn find(page: &Value, key: &str, wanted: &str) -> Option<Value> {
    items(page)
        .into_iter()
        .find(|item| text(item, key) == Some(wanted))
}

/// The three routers, one per person, over the same live rows.
struct Routers {
    john: Router,
    bob: Router,
    stranger: Router,
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn invites_and_members_travel_the_routes_end_to_end() {
    let members = Members::create().await;
    members.seed().await;
    let routers = Routers {
        john: members.router(&members.john, owner_scopes()),
        bob: members.router(&members.bob, owner_scopes()),
        stranger: members.router(&members.stranger, owner_scopes()),
    };

    let invite = john_invites_the_stranger(&routers, &members).await;
    the_stranger_accepts(&routers, &members, &invite).await;
    john_manages_the_account(&routers, &members, &invite).await;

    members.cleanup().await;
}

/// John invites the stranger's address, once; a second invite is a 409.
async fn john_invites_the_stranger(routers: &Routers, members: &Members) -> String {
    let (john, stranger) = (&members.john, &members.stranger);
    let body = json!({ "email": stranger.email.to_uppercase() }).to_string();

    let (status, invite) = call(&routers.john, Method::POST, INVITES, john, &body).await;
    assert_eq!(status, StatusCode::CREATED, "{invite}");
    let id = text(&invite, "id")
        .expect("the invite has an id")
        .to_owned();
    assert_eq!(
        text(&invite, "email"),
        Some(stranger.email.as_str()),
        "stored lowercased"
    );
    assert_eq!(text(&invite, "role"), Some(ROLE_MEMBER));
    let link = text(&invite, "link").unwrap_or_default();
    assert!(link.ends_with(&format!("/invites/{id}")), "{link}");

    let (status, twice) = call(&routers.john, Method::POST, INVITES, john, &body).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(code(&twice), Some(error_code::INVITE_CONFLICT.as_str()));
    assert_eq!(text(&twice, "current_state"), Some("invited_or_member"));

    let (_, listed) = call(&routers.john, Method::GET, INVITES, john, "").await;
    assert!(find(&listed, "id", &id).is_some(), "{listed}");
    id
}

/// The stranger sees the invite, Bob cannot accept it, the stranger can.
async fn the_stranger_accepts(routers: &Routers, members: &Members, invite: &str) {
    let (john, bob, stranger) = (&members.john, &members.bob, &members.stranger);
    let (_, waiting) = call(&routers.stranger, Method::GET, MINE, stranger, "").await;
    let mine = find(&waiting, "id", invite).expect("the invite waits for the stranger");
    assert_eq!(
        mine.pointer("/account/owner_name").and_then(Value::as_str),
        Some(john.display_name)
    );

    let accept = format!("{MINE}/{invite}/accept");
    let (status, elsewhere) = call(&routers.bob, Method::POST, &accept, bob, "").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        code(&elsewhere),
        Some(error_code::INVITE_EMAIL_MISMATCH.as_str())
    );
    let (status, accepted) = call(&routers.stranger, Method::POST, &accept, stranger, "").await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    let opened = accepted.get("workspace_ids").and_then(Value::as_array);
    assert!(
        opened.is_some_and(|ids| ids
            .iter()
            .any(|w| w.as_str() == Some(john.workspace.as_str()))),
        "{accepted}"
    );
}

/// John reads his members, cannot remove himself, removes the stranger twice.
async fn john_manages_the_account(routers: &Routers, members: &Members, invite: &str) {
    let (john, bob, stranger) = (&members.john, &members.bob, &members.stranger);
    let (_, roster) = call(&routers.john, Method::GET, MEMBERS, john, "").await;
    let role_of = |user: &str| {
        find(&roster, "user_id", user).and_then(|m| text(&m, "role").map(str::to_owned))
    };
    assert_eq!(role_of(&john.user).as_deref(), Some(ROLE_OWNER));
    assert_eq!(role_of(&stranger.user).as_deref(), Some(ROLE_MEMBER));
    let joined_of = |user: &str| {
        find(&roster, "user_id", user).and_then(|m| m.get("joined_at").and_then(Value::as_i64))
    };
    assert_eq!(joined_of(&john.user), Some(1), "signup's owner membership");
    assert_eq!(joined_of(&bob.user), Some(2), "Bob's seeded membership");

    let names_path = format!("/v1/workspaces/{}/members", john.workspace.as_str());
    let (status, names) = call(&routers.bob, Method::GET, &names_path, bob, "").await;
    assert_eq!(status, StatusCode::OK, "{names}");
    assert!(
        items(&names)
            .iter()
            .all(|member| member.get("email").is_none()),
        "{names}"
    );

    let own = format!("{MEMBERS}/{}", john.user);
    let (status, last) = call(&routers.john, Method::DELETE, &own, john, "").await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(code(&last), Some(error_code::MEMBER_LAST_OWNER.as_str()));
    let theirs = format!("{MEMBERS}/{}", stranger.user);
    for _twice in 0..2 {
        let (status, _) = call(&routers.john, Method::DELETE, &theirs, john, "").await;
        assert_eq!(
            status,
            StatusCode::NO_CONTENT,
            "removing, and removing again"
        );
    }
    let revoke = format!("{INVITES}/{invite}");
    let (status, _) = call(&routers.john, Method::DELETE, &revoke, john, "").await;
    assert_eq!(
        status,
        StatusCode::NO_CONTENT,
        "revoking an accepted invite is quiet"
    );
}

/// How many sends John's invite has counted, read from the row.
async fn attempts_of(members: &Members, invite: &str) -> i32 {
    let mut connection = members.database.acquire().await.expect("a connection");
    sqlx::query_scalar("SELECT email_attempts FROM core.invites WHERE id = $1::uuid")
        .bind(invite)
        .fetch_one(&mut *connection)
        .await
        .expect("the invite row reads")
}

/// An owner naming another account's invite or member through their own
/// `/tenants/me` routes reaches nothing: the revoke and the removal are quiet
/// no-ops, the send is not found and counts no attempt, and John's invite and
/// Bob's membership stand.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_owner_routes_never_reach_another_account() {
    let members = Members::create().await;
    members.seed().await;
    let (john, bob, stranger) = (&members.john, &members.bob, &members.stranger);
    let johns = members.router(john, owner_scopes());
    let strangers = members.router(stranger, owner_scopes());
    let body = json!({ "email": format!("dave+{}@example.test", bob.user) }).to_string();
    let (status, invite) = call(&johns, Method::POST, INVITES, john, &body).await;
    assert_eq!(status, StatusCode::CREATED, "{invite}");
    let invite = text(&invite, "id").expect("an id").to_owned();
    let counted = attempts_of(&members, &invite).await;

    let revoke = format!("{INVITES}/{invite}");
    let (status, _) = call(&strangers, Method::DELETE, &revoke, stranger, "").await;
    assert_eq!(
        status,
        StatusCode::NO_CONTENT,
        "nothing of theirs to revoke"
    );
    let resend = format!("{INVITES}/{invite}/send");
    let (status, problem) = call(&strangers, Method::POST, &resend, stranger, "").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{problem}");
    assert_eq!(code(&problem), Some(error_code::INVITE_NOT_FOUND.as_str()));
    assert_eq!(
        attempts_of(&members, &invite).await,
        counted,
        "no send counted"
    );
    let remove = format!("{MEMBERS}/{}", bob.user);
    let (status, _) = call(&strangers, Method::DELETE, &remove, stranger, "").await;
    assert_eq!(status, StatusCode::NO_CONTENT, "nobody of theirs to remove");

    let (_, listed) = call(&johns, Method::GET, INVITES, john, "").await;
    assert!(
        find(&listed, "id", &invite).is_some(),
        "John's invite stands: {listed}"
    );
    let (_, roster) = call(&johns, Method::GET, MEMBERS, john, "").await;
    assert!(
        find(&roster, "user_id", &bob.user).is_some(),
        "Bob stays: {roster}"
    );
    members.cleanup().await;
}

/// Bob is a member of John's account and owns his own: his `/tenants/me`
/// routes read his own account, never John's invites or roster.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_member_manages_only_own_account() {
    let members = Members::create().await;
    members.seed().await;
    let (john, bob, stranger) = (&members.john, &members.bob, &members.stranger);
    let johns = members.router(john, owner_scopes());
    let bobs = members.router(bob, owner_scopes());
    let body = json!({ "email": stranger.email }).to_string();
    let (status, invite) = call(&johns, Method::POST, INVITES, john, &body).await;
    assert_eq!(status, StatusCode::CREATED, "{invite}");
    let invite = text(&invite, "id").expect("an id").to_owned();

    let (status, listed) = call(&bobs, Method::GET, INVITES, bob, "").await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert!(
        find(&listed, "id", &invite).is_none(),
        "not Bob's invite: {listed}"
    );
    let (status, roster) = call(&bobs, Method::GET, MEMBERS, bob, "").await;
    assert_eq!(status, StatusCode::OK, "{roster}");
    assert!(
        find(&roster, "user_id", &john.user).is_none(),
        "not Bob's roster: {roster}"
    );
    assert!(
        find(&roster, "user_id", &bob.user).is_some(),
        "Bob owns his own: {roster}"
    );
    members.cleanup().await;
}

/// The routes' edges: malformed path ids and bodies are 400s, no credential
/// is a 401, a session without the admin grant is a 403, and an accept
/// replayed at the route answers exactly as the first.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_team_routes_refuse_malformed_and_unauthorized_calls() {
    let members = Members::create().await;
    members.seed().await;
    let (john, stranger) = (&members.john, &members.stranger);
    let johns = members.router(john, owner_scopes());

    for path in [
        format!("{INVITES}/not-an-id"),
        format!("{MEMBERS}/not-an-id"),
    ] {
        let (status, problem) = call(&johns, Method::DELETE, &path, john, "").await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{path}: {problem}");
    }
    for body in [
        json!({ "email": "not an address" }).to_string(),
        json!({ "email": stranger.email, "role": "owner" }).to_string(),
        "{".to_owned(),
    ] {
        let (status, problem) = call(&johns, Method::POST, INVITES, john, &body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}: {problem}");
    }

    let anonymous = send(&johns, Method::GET, INVITES, None, "").await;
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);
    let unscoped = members.router(john, ScopeSet::EMPTY);
    let (status, problem) = call(&unscoped, Method::GET, INVITES, john, "").await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
    assert_eq!(
        code(&problem),
        Some(error_code::AUTH_INSUFFICIENT_SCOPE.as_str())
    );

    let body = json!({ "email": stranger.email }).to_string();
    let (_, invite) = call(&johns, Method::POST, INVITES, john, &body).await;
    let invite = text(&invite, "id").expect("an id").to_owned();
    let strangers = members.router(stranger, owner_scopes());
    let accept = format!("{MINE}/{invite}/accept");
    let (first_status, first) = call(&strangers, Method::POST, &accept, stranger, "").await;
    let (again_status, again) = call(&strangers, Method::POST, &accept, stranger, "").await;
    assert_eq!(
        (first_status, again_status),
        (StatusCode::OK, StatusCode::OK)
    );
    assert_eq!(first, again, "a replayed accept answers as the first");
    members.cleanup().await;
}
