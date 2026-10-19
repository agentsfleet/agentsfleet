//! The draft collection, served through a pool that holds only `api_runtime`.
//!
//! Every other case here connects as the lane's owner, which bypasses grants.
//! So when the identity tables went immutable (no UPDATE grant), the access
//! check's `FOR SHARE OF u, w` stayed green in the suite while Postgres
//! refused it on dev: a row lock needs UPDATE on the locked table, and the
//! refusal surfaced as a 500 for the workspace's own owner. The membership
//! read a member's request takes failed the same way.

use super::{fixture::Fixture, json_body, mint, privacy, send};
use crate::harness::Fleet;
use afd_auth::{
    principal::Subject,
    scope::{Scope, ScopeSet},
};
use afd_db::{Db, config::DbRole, test_util::TestDatabase};
use axum::Router;
use http::{Method, StatusCode};

/// A login role holding `api_runtime` and nothing else, the daemon's shape.
/// `api_runtime` itself is `NOLOGIN`, so a role of our own is the only way in.
const PROBE: &str = "authoring_probe";

/// An unmarked bearer, which the router hands to the session verifier.
const SESSION: &str = "dashboard-session";

/// The lane's URL with the probe's credentials in place of the owner's.
fn probe_url(lane_url: &str) -> String {
    let (scheme, rest) = lane_url
        .split_once("://")
        .expect("the lane's URL carries a scheme");
    let tail = rest.split_once('@').map_or(rest, |(_, tail)| tail);
    format!("{scheme}://{PROBE}:{PROBE}@{tail}")
}

/// A pool that authenticates as [`PROBE`]. The role is created through the
/// owner's pool because creating it is setup, not the thing under test; the
/// duplicate arms cover two suites racing on the shared lane.
async fn restricted(f: &Fixture) -> Db {
    let mut connection = f.db.acquire().await.expect("an owner connection");
    for statement in [
        "DO $$ BEGIN \
           EXECUTE format('CREATE ROLE %I LOGIN PASSWORD %L', 'authoring_probe', 'authoring_probe'); \
         EXCEPTION WHEN duplicate_object OR unique_violation THEN NULL; END $$",
        "GRANT api_runtime TO authoring_probe",
    ] {
        sqlx::query(statement)
            .execute(&mut *connection)
            .await
            .expect("the lane's owner may create and grant to a probe role");
    }
    drop(connection);
    TestDatabase::shared()
        .open(
            DbRole::Api,
            &[
                (DbRole::Api.url_knob(), &probe_url(&f.owner_url())),
                ("DATABASE_POOL_SIZE", "2"),
                ("DATABASE_MIN_POOL_SIZE", "1"),
            ],
        )
        .await
}

/// The production router over `runtime`: live ownership, the owner's session
/// accepted, and every scope for each of `people`.
fn router(f: &Fixture, runtime: &Db, people: &[&str]) -> Router {
    let all = ScopeSet::from_scopes(&Scope::ALL);
    let fleet = Fleet::live(runtime.clone(), f.owner.as_str(), all)
        .at(afd_core::clock::now())
        .with_owned_workspace(f.workspace.clone())
        .with_live_ownership()
        .with_dashboard_holding(f.owner.as_str(), all);
    for person in people {
        let subject = Subject::new(person).expect("a fixture subject");
        let _granted = fleet.capabilities().clone().with(&subject, all);
    }
    fleet.router()
}

/// The owner lists their drafts as the runtime role, by API key and by session.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn the_owner_lists_drafts_as_api_runtime_by_key_and_by_session() {
    let f = Fixture::create().await;
    let created = send(
        &f.router(),
        Method::POST,
        &f.collection(),
        Some(&f.token),
        &f.request(&mint()).to_string(),
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let created = json_body(created).await;

    let runtime = restricted(&f).await;
    let router = router(&f, &runtime, &[]);
    for (principal, bearer) in [("API key", f.token.as_str()), ("session", SESSION)] {
        let listed = send(&router, Method::GET, &f.collection(), Some(bearer), "").await;
        let status = listed.status();
        let body = json_body(listed).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "the owner's {principal} lists drafts as api_runtime: {body}"
        );
        assert_eq!(
            body.pointer("/items/0/id"),
            created.get("id"),
            "the owner's {principal} sees the draft they created"
        );
    }
    runtime.close().await;
    f.cleanup().await;
}

/// A member of the owner's account lists their own drafts as the runtime role,
/// which runs the membership read the owner's request skips.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_member_lists_drafts_as_api_runtime() {
    let mut f = Fixture::create().await;
    let member = privacy::member(&mut f).await;
    let runtime = restricted(&f).await;
    let router = router(&f, &runtime, &[member.id.as_str()]);
    let listed = send(
        &router,
        Method::GET,
        &f.collection(),
        Some(&member.token),
        "",
    )
    .await;
    let status = listed.status();
    let body = json_body(listed).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "a member lists drafts as api_runtime: {body}"
    );
    assert_eq!(body.pointer("/items"), Some(&serde_json::json!([])));
    runtime.close().await;
    f.cleanup().await;
}
