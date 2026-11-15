//! A secret's "available to" list is exactly the credential list a lease resolves.
#![expect(clippy::expect_used, reason = "test preconditions fail loudly")]

use afd_auth::scope::{Scope, ScopeSet};
use afd_db::test_util::{
    FIXTURE_SETUP_DEFINITION, FIXTURE_SETUP_SEMANTICS, FleetSeed, SetupRevision, TestDatabase,
    mint_id,
};
use afd_fleet_lifecycle::FleetStatus;
use axum::Router;
use http::{Method, StatusCode};
use serde_json::{Value, json};

use super::fixture::{SUBJECT, Tasks};
use crate::harness::{Fleet, json_body, send};

/// Declared by a revision only unfinished tasks still run.
const ALPHA: &str = "alpha_key";
/// Declared by the fleet's current head.
const BETA: &str = "beta_key";
/// Only mentioned in instructions, never declared.
const GAMMA: &str = "gamma_key";

/// A valid native definition declaring `credentials` and mentioning `GAMMA` in prose.
fn definition(credentials: &[&str]) -> (Value, String) {
    let mut value: Value =
        serde_json::from_str(FIXTURE_SETUP_DEFINITION).expect("native fixture definition");
    value
        .pointer_mut("/config/x-agentsfleet")
        .and_then(Value::as_object_mut)
        .expect("native runtime extension")
        .insert("credentials".to_owned(), json!(credentials));
    value.as_object_mut().expect("definition object").insert(
        "instructions_md".to_owned(),
        json!(format!("Use {GAMMA} only if the operator asks.")),
    );
    let parsed = af_fleet_runtime::definition::Definition::parse(
        &serde_json::to_string(&value).expect("fixture document"),
    )
    .expect("a definition declaring credentials stays valid");
    let digest =
        af_fleet_runtime::definition::canonical_digest(&parsed).expect("native content identity");
    (
        serde_json::to_value(parsed).expect("typed definition"),
        digest,
    )
}

async fn run(
    fixture: &Tasks<'_>,
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

/// Appends a head revision declaring `credentials`, as Apply would.
async fn append(fixture: &Tasks<'_>, credentials: &[&str]) {
    let (definition, digest) = definition(credentials);
    let mut connection = fixture
        .base
        .database
        .acquire()
        .await
        .expect("history connection");
    let _revision: i64 = sqlx::query_scalar(include_str!("fixtures/advance_revision.sql"))
        .bind(mint_id())
        .bind(&fixture.base.fleet)
        .bind(definition)
        .bind(digest)
        .fetch_one(&mut *connection)
        .await
        .expect("history and head commit together");
}

/// A fleet in another workspace of the same account that declares `ALPHA`.
async fn foreign_fleet(fixture: &Tasks<'_>) {
    let workspace = fixture.base.other_workspace().await;
    let (definition, digest) = definition(&[ALPHA]);
    let stored = definition.to_string();
    let mut connection = fixture
        .base
        .database
        .acquire()
        .await
        .expect("foreign connection");
    FleetSeed::new(
        &mint_id(),
        &workspace,
        fixture.base.tenant(),
        FleetStatus::Active.as_str(),
    )
    .name("Foreign reader")
    .setup(SetupRevision {
        definition: &stored,
        digest: &digest,
        semantics: FIXTURE_SETUP_SEMANTICS,
        model_ref: None,
    })
    .insert(&mut *connection)
    .await
    .expect("foreign fleet seeds");
}

fn router(fixture: &Tasks<'_>, scopes: &[Scope]) -> Router {
    Fleet::live(
        fixture.base.database.clone(),
        SUBJECT,
        ScopeSet::from_scopes(scopes),
    )
    .with_owned_workspace(fixture.base.workspace.clone())
    .router()
}

/// Secret name → the fleet names its `available_to` lists, or `None` when absent.
async fn access(router: &Router, fixture: &Tasks<'_>) -> Vec<(String, Option<Vec<String>>)> {
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
    let fixture = Tasks::create(&lane).await;
    // Unfinished tasks move onto a revision declaring ALPHA; the head then moves to BETA.
    append(&fixture, &[ALPHA]).await;
    run(
        &fixture,
        include_str!("fixtures/accept_head.sql"),
        (&fixture.base.fleet, None),
    )
    .await;
    append(&fixture, &[BETA]).await;
    foreign_fleet(&fixture).await;
    let owner = router(
        &fixture,
        &[Scope::SecretRead, Scope::SecretWrite, Scope::FleetRead],
    );
    for name in [ALPHA, BETA, GAMMA] {
        let path = format!("/v1/workspaces/{}/secrets", fixture.base.workspace);
        let body = json!({"name": name, "data": {"token": "fixture-value"}}).to_string();
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
            (ALPHA.into(), fleet()),
            (BETA.into(), fleet()),
            (GAMMA.into(), Some(vec![]))
        ],
        "head and unfinished-task revisions grant; prose and other workspaces do not"
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
        include_str!("fixtures/archive_fleet.sql"),
        (&fixture.base.fleet, Some(FleetStatus::Archived.as_str())),
    )
    .await;
    assert!(
        access(&owner, &fixture)
            .await
            .iter()
            .all(|(_, fleets)| fleets.as_deref() == Some(&[])),
        "an archived fleet takes no work, so it can read nothing"
    );
    fixture.base.cleanup().await;
}
