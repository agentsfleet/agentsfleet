//! Approving a setup grant through the approvals route restarts setup, so an
//! installing fleet whose last grant was approved becomes active, and its
//! trigger schedules sync, with no further request; a denial leaves it
//! installing. Approving is the person's whole answer, so the fleet must not
//! wait on a second request.

use super::{
    super::{json_body, mint, send},
    fixture::Case,
};
use af_core::{clock, id::Uuid7};
use afd_api::Services as _;
use afd_auth::Scope;
use afd_vault::{SecretBody, SecretName};
use http::{Method, StatusCode, header};
use serde_json::json;

/// The refresh token a fixture Jira credential carries; never a real one.
const FIXTURE_REFRESH_TOKEN: &str = "private-grant-resume-token";

/// Seals a Jira credential, so setup's only missing check is the grant a
/// person must approve.
async fn jira_credential(case: &Case) {
    let body = json!({"integration":"jira","refresh_token":FIXTURE_REFRESH_TOKEN});
    let raw = serde_json::value::RawValue::from_string(body.to_string()).expect("credential JSON");
    case.base
        .services(&Scope::ALL)
        .secrets()
        .create(
            &case.base.workspace,
            &SecretName::parse("jira").expect("credential name"),
            &SecretBody::parse(&raw).expect("credential body"),
            clock::now(),
        )
        .await
        .expect("seal the credential through the router's vault");
}

/// Saves a fleet that needs the Jira grant; answers its item and its one
/// pending setup card.
async fn installing(case: &Case) -> (String, Uuid7) {
    case.fund().await;
    jira_credential(case).await;
    let (_, request) = case
        .request_with(json!({"credentials":["jira"],
            "triggers":[{"type":"cron","schedule":"0 9 * * *","timezone":"UTC","message":"Daily check"}]}))
        .await;
    let saved = case.save(mint().as_str(), &request).await;
    assert_eq!(saved.status(), StatusCode::CREATED);
    let item = saved.headers()[header::LOCATION]
        .to_str()
        .expect("fleet location")
        .to_owned();
    let body = json_body(saved).await;
    assert_eq!(body["status"], "installing");
    let fleet = super::fixture::fleet_of(&item);
    let cards: Vec<(Uuid7, String, String)> = sqlx::query_as(
        "SELECT id,gate_type,status FROM fleet.approval_gates WHERE fleet_id=$1 ORDER BY id",
    )
    .bind(&fleet)
    .fetch_all(&mut *case.base.db.acquire().await.expect("card connection"))
    .await
    .expect("setup cards");
    assert_eq!(
        cards.len(),
        1,
        "one setup card: {cards:?} {}",
        body["setup"]
    );
    assert_eq!(
        (cards[0].1.as_str(), cards[0].2.as_str()),
        ("integration_grant", "pending")
    );
    (item, cards[0].0.clone())
}

/// The fleet's one schedule row version; every sync claim bumps it.
async fn schedule_version(case: &Case, item: &str) -> i64 {
    let fleet = super::fixture::fleet_of(item);
    sqlx::query_scalar("SELECT resource_version FROM fleet.schedules WHERE fleet_id=$1")
        .bind(&fleet)
        .fetch_one(&mut *case.base.db.acquire().await.expect("schedule connection"))
        .await
        .expect("the fleet's one schedule")
}

/// Answers the card through the HTTP approvals route; answers the fleet's
/// status and whether its schedule was synced since Save.
async fn decided(case: &Case, decision: &str) -> (String, bool) {
    let (item, gate) = installing(case).await;
    let saved = schedule_version(case, &item).await;
    let route = format!(
        "/v1/workspaces/{}/approvals/{}/{decision}",
        case.base.workspace,
        gate.as_str()
    );
    let answered = send(
        &case.router,
        Method::POST,
        &route,
        Some(&case.base.token),
        "",
    )
    .await;
    assert_eq!(answered.status(), StatusCode::OK, "{decision}");
    let read = send(&case.router, Method::GET, &item, Some(&case.base.token), "").await;
    assert_eq!(read.status(), StatusCode::OK);
    let status = json_body(read).await["status"]
        .as_str()
        .expect("fleet status")
        .to_owned();
    (status, schedule_version(case, &item).await > saved)
}

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn approving_a_setup_grant_through_the_route_activates_the_fleet() {
    let approved = Case::create().await;
    assert_eq!(
        decided(&approved, "approve").await,
        ("active".to_owned(), true),
        "the approval alone activates and syncs: no PATCH, no second Save"
    );
    approved.cleanup().await;

    let denied = Case::create().await;
    assert_eq!(
        decided(&denied, "deny").await,
        ("installing".to_owned(), false),
        "a denial neither activates nor syncs"
    );
    denied.cleanup().await;
}
