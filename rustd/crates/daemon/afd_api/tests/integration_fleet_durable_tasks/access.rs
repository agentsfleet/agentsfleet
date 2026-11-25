//! A secret's "available to" list is exactly the credential list a run
//! resolves: the names a person approved for each runnable fleet.
#![expect(clippy::expect_used, reason = "test preconditions fail loudly")]

use af_wire::durable_task::DurableTaskState;
use afd_api_wire::grant::{grant_type, status};
use afd_auth::scope::{Scope, ScopeSet};
use afd_db::test_util::{FleetSeed, GrantSeed, TestDatabase, mint_id};
use afd_fleet_lifecycle::FleetStatus;
use axum::Router;
use http::{Method, StatusCode};
use serde_json::{Value, json};

use super::fixture::{DurableTasks, SUBJECT};
use crate::harness::{Fleet, json_body, send};

/// Asked for by the fleet and waiting on a person: it grants nothing yet.
const ALPHA: &str = "alpha_key";
/// Approved for the fleet.
const BETA: &str = "beta_key";
/// Never asked for.
const GAMMA: &str = "gamma_key";
/// A provider key. It reaches a run through the fleet's model, so a grant
/// list cannot say who uses it.
const DELTA: &str = "delta_key";

async fn run(
    fixture: &DurableTasks<'_>,
    statement: &'static str,
    binds: (&af_core::id::Uuid7, Option<&str>),
) {
    let mut connection = fixture
        .base
        .database
        .acquire()
        .await
        .expect("fixture connection");
    let mut query = sqlx::query(statement).bind(binds.0);
    if let Some(value) = binds.1 {
        query = query.bind(value);
    }
    query
        .execute(&mut *connection)
        .await
        .expect("fixture statement");
}

/// The fleet's grant of `name` in `status`, as a person's decision leaves it.
async fn grant(fixture: &DurableTasks<'_>, fleet: &af_core::id::Uuid7, name: &str, at: &str) {
    let mut connection = fixture
        .base
        .database
        .acquire()
        .await
        .expect("grant connection");
    let seed = GrantSeed::new(fleet, name, grant_type::SECRET, at);
    let seed = if at == status::APPROVED {
        seed.approved(afd_db::test_util::SEEDED_AT)
    } else {
        seed
    };
    seed.insert(&mut *connection)
        .await
        .expect("the grant seeds");
}

/// A fleet in another workspace of the same account approved for `ALPHA`.
async fn foreign_fleet(fixture: &DurableTasks<'_>) {
    let workspace = fixture.base.other_workspace().await;
    let fleet = mint_id();
    let mut connection = fixture
        .base
        .database
        .acquire()
        .await
        .expect("foreign connection");
    FleetSeed::new(
        &fleet,
        &workspace,
        fixture.base.tenant(),
        FleetStatus::Active.as_str(),
    )
    .name("Foreign reader")
    .insert(&mut *connection)
    .await
    .expect("foreign fleet seeds");
    drop(connection);
    grant(fixture, &fleet, ALPHA, status::APPROVED).await;
}

fn router(fixture: &DurableTasks<'_>, scopes: &[Scope]) -> Router {
    Fleet::live(
        fixture.base.database.clone(),
        SUBJECT,
        ScopeSet::from_scopes(scopes),
    )
    .with_owned_workspace(fixture.base.workspace.clone())
    .router()
}

/// Secret name → the fleet names its `available_to` lists, or `None` when absent.
async fn access(router: &Router, fixture: &DurableTasks<'_>) -> Vec<(String, Option<Vec<String>>)> {
    let path = format!("/v1/workspaces/{}/secrets", fixture.base.workspace);
    let response = send(router, Method::GET, &path, Some(&fixture.base.token), "").await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    body.get("secrets")
        .and_then(Value::as_array)
        .expect("secret list")
        .iter()
        .map(|secret| {
            let fleets = secret.get("available_to").map(|fleets| {
                fleets
                    .as_array()
                    .expect("fleet list")
                    .iter()
                    .map(|fleet| {
                        assert_eq!(fleet.get("id"), Some(&json!(fixture.base.fleet.as_str())));
                        fleet
                            .get("name")
                            .and_then(Value::as_str)
                            .expect("fleet name")
                            .to_owned()
                    })
                    .collect()
            });
            (
                secret
                    .get("name")
                    .and_then(Value::as_str)
                    .expect("name")
                    .to_owned(),
                fleets,
            )
        })
        .collect()
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn secret_access_projection_matches_execution_policy() {
    let lane = TestDatabase::shared();
    let fixture = DurableTasks::create(&lane).await;
    // The fleet asked for ALPHA and a person has not answered; a person
    // approved BETA. Another workspace's fleet holds ALPHA, which this
    // workspace's list never shows.
    let fleet = fixture.base.fleet.clone();
    grant(&fixture, &fleet, ALPHA, status::PENDING).await;
    grant(&fixture, &fleet, BETA, status::APPROVED).await;
    foreign_fleet(&fixture).await;
    let owner = router(
        &fixture,
        &[Scope::SecretRead, Scope::SecretWrite, Scope::FleetRead],
    );
    let token = json!({"token": "fixture-value"});
    let provider_key = json!({"provider": "anthropic", "api_key": "fixture-value"});
    for (name, data) in [
        (ALPHA, &token),
        (BETA, &token),
        (GAMMA, &token),
        (DELTA, &provider_key),
    ] {
        let path = format!("/v1/workspaces/{}/secrets", fixture.base.workspace);
        let body = json!({"name": name, "data": data}).to_string();
        let stored = send(
            &owner,
            Method::POST,
            &path,
            Some(&fixture.base.token),
            &body,
        )
        .await;
        assert_eq!(stored.status(), StatusCode::CREATED, "{name} stores");
    }
    let fleet = || Some(vec!["Repair progress".to_owned()]);
    assert_eq!(
        access(&owner, &fixture).await,
        [
            (ALPHA.into(), Some(vec![])),
            (BETA.into(), fleet()),
            (DELTA.into(), None),
            (GAMMA.into(), Some(vec![]))
        ],
        "only an approved grant lists access: a pending request and another \
         workspace's grant list nothing, and a provider key claims no access either way"
    );
    let secrets_only = router(&fixture, &[Scope::SecretRead]);
    assert!(
        access(&secrets_only, &fixture)
            .await
            .iter()
            .all(|(_, fleets)| fleets.is_none()),
        "a caller who may not read fleets learns no fleet names"
    );
    run(
        &fixture,
        include_str!("fixtures/finish_durable_tasks.sql"),
        (&fixture.base.fleet, Some(DurableTaskState::Done.as_str())),
    )
    .await;
    assert_eq!(
        access(&owner, &fixture).await,
        [
            (ALPHA.into(), Some(vec![])),
            (BETA.into(), fleet()),
            (DELTA.into(), None),
            (GAMMA.into(), Some(vec![]))
        ],
        "finishing the tasks changes nothing; the approved grant still lists"
    );
    run(
        &fixture,
        include_str!("fixtures/archive_fleet.sql"),
        (&fixture.base.fleet, Some(FleetStatus::Archived.as_str())),
    )
    .await;
    assert!(
        access(&owner, &fixture)
            .await
            .iter()
            .filter(|(name, _)| name != DELTA)
            .all(|(_, fleets)| fleets.as_deref() == Some(&[])),
        "an archived fleet takes no work, so it can read nothing"
    );
    fixture.base.cleanup().await;
}
