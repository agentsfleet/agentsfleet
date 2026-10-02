//! Invites and members through the routes, over live Postgres: John invites
//! the stranger, the stranger accepts, John manages the account.
//!
//! The store's own rules are `afd_tenant`'s integration suite. What this adds
//! is the HTTP edge: each route is mounted, reads the caller's account from
//! the path, answers with the documented shape, and refuses with the
//! documented code. That no route reaches another account is
//! `integration_team_routes_scope.rs`, which shares the helpers here.
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

use crate::harness::{self, exchange, items, text};
use crate::integration_workspace_members::fixture::{Members, Person, owner_scopes};

pub(crate) const INVITES: &str = "/v1/tenants/me/invites";
pub(crate) const MEMBERS: &str = "/v1/tenants/me/members";
const MINE: &str = "/v1/users/me/invites";
/// The `current_state` a duplicate invite answers with, by what stands in its way.
const STATE_INVITED: &str = "invited";
const STATE_MEMBER: &str = "member";

/// One request as `who`, answered as its status and its JSON body.
pub(crate) async fn call(
    router: &Router,
    method: Method,
    path: &str,
    who: &Person,
    body: &str,
) -> (StatusCode, Value) {
    exchange(router, method, path, Some(&who.token), body).await
}

/// `who` sends `invite`'s email again.
pub(crate) async fn send_again(router: &Router, who: &Person, invite: &str) -> (StatusCode, Value) {
    let path = format!("{INVITES}/{invite}/send");
    call(router, Method::POST, &path, who, "").await
}

/// The item in a page whose `key` is `wanted`.
pub(crate) fn find<'page>(page: &'page Value, key: &str, wanted: &str) -> Option<&'page Value> {
    items(page)
        .iter()
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
    assert_eq!(
        harness::error_code(&twice),
        Some(error_code::INVITE_CONFLICT.as_str())
    );
    assert_eq!(text(&twice, "current_state"), Some(STATE_INVITED));

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
        harness::error_code(&elsewhere),
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

    // Now a member, the address is refused as one, not as a pending invite.
    let body = json!({ "email": stranger.email }).to_string();
    let (status, again) = call(&routers.john, Method::POST, INVITES, john, &body).await;
    assert_eq!(status, StatusCode::CONFLICT, "{again}");
    assert_eq!(
        harness::error_code(&again),
        Some(error_code::INVITE_CONFLICT.as_str())
    );
    assert_eq!(text(&again, "current_state"), Some(STATE_MEMBER));
    // And John revoking the invite she joined through hears the same, rather
    // than a 204 that would say she was kept out.
    let revoke = format!("{INVITES}/{invite}");
    let (status, joined) = call(&routers.john, Method::DELETE, &revoke, john, "").await;
    assert_eq!(status, StatusCode::CONFLICT, "{joined}");
    assert_eq!(
        harness::error_code(&joined),
        Some(error_code::INVITE_CONFLICT.as_str())
    );
    assert_eq!(text(&joined, "current_state"), Some(STATE_MEMBER));
}

/// John reads his members, cannot remove himself, removes the stranger twice.
async fn john_manages_the_account(routers: &Routers, members: &Members, invite: &str) {
    let (john, bob, stranger) = (&members.john, &members.bob, &members.stranger);
    let (_, roster) = call(&routers.john, Method::GET, MEMBERS, john, "").await;
    let role_of = |user: &str| find(&roster, "user_id", user).and_then(|m| text(m, "role"));
    assert_eq!(role_of(&john.user), Some(ROLE_OWNER));
    assert_eq!(role_of(&stranger.user), Some(ROLE_MEMBER));
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
    assert_eq!(
        harness::error_code(&last),
        Some(error_code::MEMBER_LAST_OWNER.as_str())
    );
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
        "revoking a spent invite whose invitee left is quiet"
    );
}

/// The routes' edges: malformed path ids and bodies are 400s, no credential
/// is a 401, a session without the admin grant is a 403, and an accept
/// replayed at the route answers exactly as the first.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_team_routes_refuse_malformed_and_unauthorized_calls() {
    let members = Members::create().await;
    let (john, stranger) = (&members.john, &members.stranger);
    let johns = members.router(john, owner_scopes());

    refuses_malformed_calls(&johns, john, stranger).await;

    let (status, anonymous) = exchange(&johns, Method::GET, INVITES, None, "").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        harness::error_code(&anonymous),
        Some(error_code::AUTH_UNAUTHORIZED.as_str())
    );
    let unscoped = members.router(john, ScopeSet::EMPTY);
    let (status, problem) = call(&unscoped, Method::GET, INVITES, john, "").await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{problem}");
    assert_eq!(
        harness::error_code(&problem),
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

/// Malformed path ids and malformed bodies are each a `400` with the
/// invalid-request code.
async fn refuses_malformed_calls(johns: &Router, john: &Person, stranger: &Person) {
    for path in [
        format!("{INVITES}/not-an-id"),
        format!("{MEMBERS}/not-an-id"),
    ] {
        let (status, problem) = call(johns, Method::DELETE, &path, john, "").await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{path}: {problem}");
        assert_eq!(
            harness::error_code(&problem),
            Some(error_code::INVALID_REQUEST.as_str())
        );
    }
    for body in [
        json!({ "email": "not an address" }).to_string(),
        // Shaped like an address, but the mail library refuses it, so every
        // send of the invite would fail.
        json!({ "email": "a<b@example.com" }).to_string(),
        json!({ "email": stranger.email, "role": "owner" }).to_string(),
        "{".to_owned(),
    ] {
        let (status, problem) = call(johns, Method::POST, INVITES, john, &body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}: {problem}");
        assert_eq!(
            harness::error_code(&problem),
            Some(error_code::INVALID_REQUEST.as_str()),
            "{body}"
        );
    }
}
