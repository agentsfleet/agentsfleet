//! A running fleet asks a person for one secret or integration through the
//! run it runs under: `POST L/access`.
//!
//! Booted as the schedules suites are (`crate::schedules::Assigned`): the
//! daemon's own router and run plane over the lane's Postgres and NATS, the
//! seeded event's run taken. A request raises one card and answers 201; a
//! repeat while the card is open, and a request the approved grant already
//! covers, answer 200 and raise none.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test target: an unmet precondition should fail the test loudly, and a step \
              indexes the JSON it was answered"
)]

use af_crypto::entropy::Entropy;
use afd_approval::{GrantKind, KIND_INTEGRATION_GRANT};
use afd_connector::Provider;
use afd_db::test_util::InstallSeed;
use agentsfleetd::supervisor::Supervisor;
use reqwest::Method;
use serde_json::{Value, json};

use crate::definition_install::seal;
use crate::schedules::Assigned;
use crate::verbs::FakeQStash;

/// The workspace secret the cases ask for.
const SENTRY: &str = "sentry";

/// An identifier no run carries.
const UNKNOWN: &str = "0195b4ba-8d3a-7fff-8abc-ffffffffffff";

/// The held run's access verb.
fn access(held: &Assigned) -> String {
    format!("/v1/runners/me/runs/{}/access", held.run_id)
}

/// A request for `name` under the held run's fence.
fn asking(held: &Assigned, name: &str) -> Value {
    json!({"fencing_token": held.fence, "name": name})
}

/// Seals `body` under `name` in the held fleet's workspace.
async fn store(held: &Assigned, name: &str, body: &Value) {
    let scene = &held.scene;
    let body = body.to_string();
    seal(
        &scene.booted,
        &scene.workspace,
        name,
        &body,
        af_core::clock::now(),
    )
    .await;
}

/// Seals the [`SENTRY`] secret, sent only to its own host.
async fn store_sentry(held: &Assigned) {
    store(
        held,
        SENTRY,
        &json!({"host": "sentry.io", "token": "fixture"}),
    )
    .await;
}

/// Connects GitHub to the held fleet's workspace: the App installation's
/// handle in the vault and its routing row, as the connect flow writes both.
async fn connect_github(held: &Assigned) {
    let mut bytes = [0_u8; 4];
    Entropy::new()
        .fill(&mut bytes)
        .expect("the host provides entropy");
    let installation = u32::from_be_bytes(bytes).to_string();
    let handle = json!({"integration": Provider::GitHub.id(), "installation_id": installation});
    store(held, Provider::GitHub.id(), &handle).await;
    let seed = InstallSeed::new(Provider::GitHub.id(), &installation, &held.scene.workspace);
    seed.insert(&mut *held.connection().await)
        .await
        .expect("the installation maps to the workspace");
}

/// The held fleet's grants and its access cards.
async fn rows(held: &Assigned) -> (i64, i64) {
    sqlx::query_as(
        "SELECT (SELECT count(*) FROM connector.integration_grants WHERE fleet_id = $1), \
         (SELECT count(*) FROM fleet.approval_gates WHERE fleet_id = $1 AND gate_type = $2)",
    )
    .bind(&held.scene.fleet)
    .bind(KIND_INTEGRATION_GRANT)
    .fetch_one(&mut *held.connection().await)
    .await
    .expect("grant and card counts")
}

/// The stored `(grant_type, status)` of the held fleet's grant of `name`.
async fn grant(held: &Assigned, name: &str) -> (String, String) {
    sqlx::query_as(
        "SELECT grant_type, status FROM connector.integration_grants \
         WHERE fleet_id = $1 AND service = $2",
    )
    .bind(&held.scene.fleet)
    .bind(name)
    .fetch_one(&mut *held.connection().await)
    .await
    .expect("the fleet's grant")
}

/// Answers `(status, error_code)` for one request as the held run.
async fn refused(held: &Assigned, path: &str, body: &Value) -> (u16, Value) {
    let (status, problem) = held.call(Method::POST, path, Some(body)).await;
    (status, problem["error_code"].clone())
}

/// A request for a stored secret sets its grant pending and raises one card;
/// a repeat while that card is open answers it and raises no second.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_a_request_raises_one_card_and_a_repeat_finds_it_open() {
    let mut supervisor = Supervisor::new();
    let qstash = FakeQStash::start().await;
    let held = Box::pin(Assigned::boot(&mut supervisor, &qstash)).await;
    store_sentry(&held).await;

    let (status, raised) = held
        .call(Method::POST, &access(&held), Some(&asking(&held, SENTRY)))
        .await;
    assert_eq!(status, 201, "{raised}");
    assert!(raised["gate_id"].is_string(), "{raised}");
    let (status, open) = held
        .call(Method::POST, &access(&held), Some(&asking(&held, SENTRY)))
        .await;

    assert_eq!(status, 200, "{open}");
    assert_eq!(raised["state"], "raised");
    assert_eq!(open["state"], "pending");
    assert_eq!(open["grant_id"], raised["grant_id"]);
    assert_eq!(open["gate_id"], raised["gate_id"]);
    let expected = (GrantKind::Secret.as_str().to_owned(), "pending".to_owned());
    assert_eq!(grant(&held, SENTRY).await, expected);
    assert_eq!(rows(&held).await, (1, 1));
    held.finish(supervisor).await;
}

/// A request the fleet's approved grant already covers is answered at once,
/// with the grant and no card.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_a_request_the_approved_grant_covers_raises_no_card() {
    let mut supervisor = Supervisor::new();
    let qstash = FakeQStash::start().await;
    let held = Box::pin(Assigned::boot(&mut supervisor, &qstash)).await;
    store_sentry(&held).await;
    let now = af_core::clock::now();
    crate::e2e_seed::approve_secret(&held.scene.booted, &held.scene.fleet, SENTRY, now).await;

    let (status, covered) = held
        .call(Method::POST, &access(&held), Some(&asking(&held, SENTRY)))
        .await;

    assert_eq!(status, 200, "{covered}");
    assert!(covered["grant_id"].is_string(), "{covered}");
    assert_eq!(covered["state"], "covered");
    assert_eq!(
        covered["gate_id"],
        Value::Null,
        "a covered request raises no card"
    );
    assert_eq!(rows(&held).await, (1, 0));
    held.finish(supervisor).await;
}

/// A name the workspace lacks, an integration it never connected, and a
/// GitHub request with no scope are each refused, and nothing is written.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_access_refuses_what_the_workspace_cannot_give() {
    let mut supervisor = Supervisor::new();
    let qstash = FakeQStash::start().await;
    let held = Box::pin(Assigned::boot(&mut supervisor, &qstash)).await;
    let path = access(&held);
    let mut scoped = asking(&held, Provider::GitHub.id());
    scoped["scope"] = json!({"repositories": ["org/app"], "access": "read"});

    let missing = refused(&held, &path, &asking(&held, SENTRY)).await;
    let unconnected = refused(&held, &path, &scoped).await;
    connect_github(&held).await;
    let unscoped = refused(&held, &path, &asking(&held, Provider::GitHub.id())).await;

    assert_eq!(missing, (404, json!("UZ-AGT-022")));
    assert_eq!(unconnected, (409, json!("UZ-AGT-023")));
    assert_eq!(unscoped, (400, json!("UZ-REQ-001")));
    assert_eq!(rows(&held).await, (0, 0));
    held.finish(supervisor).await;
}

/// A fence the run does not hold, a run that is not this runner's, and a body
/// the verb cannot read are refused before any grant is touched.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_access_refuses_a_run_that_is_not_held() {
    let mut supervisor = Supervisor::new();
    let qstash = FakeQStash::start().await;
    let held = Box::pin(Assigned::boot(&mut supervisor, &qstash)).await;
    store_sentry(&held).await;
    let path = access(&held);
    let elsewhere = format!("/v1/runners/me/runs/{UNKNOWN}/access");
    let mut late = asking(&held, SENTRY);
    late["fencing_token"] = json!(held.fence + 1);
    let mut foreign = asking(&held, SENTRY);
    foreign["fleet_id"] = json!(held.scene.fleet);
    let cases = [
        (path.clone(), late, 409, "UZ-RUN-005"),
        (elsewhere, asking(&held, SENTRY), 404, "UZ-RUN-006"),
        (path.clone(), foreign, 400, "UZ-REQ-001"),
        (path.clone(), asking(&held, ""), 400, "UZ-REQ-001"),
        (
            "/v1/runners/me/runs/not-a-run/access".to_owned(),
            asking(&held, SENTRY),
            400,
            "UZ-REQ-001",
        ),
    ];

    for (route, body, status, code) in cases {
        let answered = refused(&held, &route, &body).await;
        assert_eq!(answered, (status, json!(code)), "{route}: {body}");
    }
    assert_eq!(rows(&held).await, (0, 0));
    held.finish(supervisor).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_host_request_for_an_address_is_refused() {
    let mut supervisor = Supervisor::new();
    let qstash = FakeQStash::start().await;
    let held = Box::pin(Assigned::boot(&mut supervisor, &qstash)).await;
    let path = access(&held);
    for host in [
        "10.0.0.5",
        "localhost",
        "169.254.169.254",
        "https://docs.example/private",
    ] {
        let body = json!({"fencing_token":held.fence,"host":host});
        assert_eq!(
            refused(&held, &path, &body).await,
            (400, json!("UZ-REQ-001"))
        );
    }
    let mismatch = json!({"fencing_token":held.fence,"host":"docs.example",
        "evidence":{"tool":"web_fetch","from_host":"app.example","to_host":"elsewhere.example"}});
    assert_eq!(
        refused(&held, &path, &mismatch).await,
        (400, json!("UZ-REQ-001"))
    );
    assert_eq!(rows(&held).await, (0, 0));
    held.finish(supervisor).await;
}
