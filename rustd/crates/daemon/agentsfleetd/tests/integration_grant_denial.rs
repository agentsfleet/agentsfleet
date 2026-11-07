//! A connected workspace still needs fleet approval; denial must drain the queue.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test preconditions must fail loudly"
)]

use crate::e2e::{GOOD_KEK, Scenario, scenario};
use crate::reads::event_column;
use crate::wire::{assert_no_lease_for_fleet_under_test, capable_beat, fleet_poll, json, post};
use afd_approval::{Decision, Inbox};
use afd_core::id::Uuid7;
use afd_crypto::{entropy::Entropy, secret::Kek};
use afd_fleet::lease::Leases;
use afd_vault::{SecretBody, SecretName, Vault};
use afd_wire::event::EventType;
use agentsfleetd::supervisor::Supervisor;
use serde_json::json;
use std::sync::Arc;

const SERVICE: &str = "github";
const LEASES: &str = "/v1/runners/me/leases";
const LABEL: &str = "COALESCE(failure_reason, '')";

async fn connect_and_declare(run: &Scenario) {
    let vault = Vault::new(
        run.booted.database.clone(),
        Arc::new(Kek::from_hex(GOOD_KEK).expect("fixture key")),
        Entropy::new(),
    );
    let raw = serde_json::value::RawValue::from_string(
        json!({"integration": SERVICE, "app_id": "7", "installation_id": "42"}).to_string(),
    )
    .expect("handle JSON");
    vault
        .create(
            &Uuid7::parse(&run.workspace).expect("workspace"),
            &SecretName::parse(SERVICE).expect("secret name"),
            &SecretBody::parse(&raw).expect("handle body"),
            run.seeded_at,
        )
        .await
        .expect("existing connection");
    // Declared the way a person declares it: an Apply selecting a setup that
    // names the credential. The lease reads the selected revision, never a
    // copy on the fleet row.
    let mut connection = run.booted.database.acquire().await.expect("connection");
    let mut config: serde_json::Value = sqlx::query_scalar::<_, String>(
        "SELECT (r.definition -> 'config')::text FROM fleet.fleets f \
         JOIN fleet.revisions r ON r.id = f.head_revision_id WHERE f.id = $1::uuid",
    )
    .bind(&run.fleet)
    .fetch_one(&mut *connection)
    .await
    .map(|text| serde_json::from_str(&text).expect("stored config JSON"))
    .expect("the fleet's config reads");
    config
        .get_mut("x-agentsfleet")
        .and_then(serde_json::Value::as_object_mut)
        .expect("the stored config carries its runtime block")
        .insert("credentials".to_owned(), json!([SERVICE]));
    let definition =
        crate::e2e_native::definition(&config.to_string(), crate::e2e_seed::FIXTURE_INSTRUCTIONS);
    crate::e2e_native::reselect(&mut connection, &run.fleet, &definition).await;
    drop(connection);
    // Prefer this fixture's fleet over unrelated ready marks left by other suites.
    Leases::new(
        run.booted.database.clone(),
        run.booted.queue.clone(),
        Entropy::new(),
    )
    .claim(
        &Uuid7::parse(&run.fleet).expect("fleet"),
        &run.runner_id,
        std::time::Duration::ZERO,
    )
    .await
    .expect("initial affinity claim")
    .expect("new fleet is unclaimed");
}

/// How many approval cards this fixture's fleet has raised.
async fn gate_cards(run: &Scenario) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM fleet.approval_gates WHERE fleet_id = $1::uuid")
        .bind(&run.fleet)
        .fetch_one(&mut *run.booted.database.acquire().await.expect("connection"))
        .await
        .expect("the gate count must run")
}

/// Poll until the durable outcome is visible, checking that no run was issued.
async fn poll_until<F, Fut>(http: &reqwest::Client, run: &Scenario, awaited: &str, mut reached: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    const POLL_LIMIT: usize = 8;
    for _poll in 0..POLL_LIMIT {
        sqlx::query(
            "UPDATE runner.fleet_slots SET lease_expires_at = clock_timestamp() - interval '1 millisecond' \
             WHERE fleet_id = $1::uuid",
        )
        .bind(&run.fleet)
            .execute(&mut *run.booted.database.acquire().await.expect("connection"))
            .await
            .expect("expire the previous no-work claim");

        let response = post(http, run, LEASES, &fleet_poll()).await;
        // The body is in the message on purpose: a bare status says a poll
        // failed and nothing about why, and the daemon's own log is not
        // captured by this suite.
        let status = response.status().as_u16();
        let body = json(response).await;
        assert_eq!(status, 200, "the poll answers: {body}");
        assert_no_lease_for_fleet_under_test(run, &body);

        if reached().await {
            return;
        }
    }
    panic!("{awaited} did not happen in {POLL_LIMIT} polls; inspect AFD_TEST_LOG=1")
}

/// The fleet's monotonic claim counter.
///
/// `CLAIM_FLEET_SLOT` bumps it on every won claim, and this suite expires
/// the deadline before each poll, so a poll that reaches this fleet's partition
/// always wins the claim and always moves this number. That makes it the one
/// observable meaning "the daemon looked here" — as opposed to the rows the
/// callers assert on, which say what it decided once it did.
async fn fencing_token(run: &Scenario) -> i64 {
    sqlx::query_scalar("SELECT fencing_token FROM runner.fleet_slots WHERE fleet_id = $1::uuid")
        .bind(&run.fleet)
        .fetch_one(&mut *run.booted.database.acquire().await.expect("connection"))
        .await
        .expect("the affinity row exists: `connect_and_declare` claimed it")
}

/// Polls until the daemon has claimed this fleet once more.
///
/// Every call site asserts on what a poll DECIDED, and each of those outcomes
/// can already be true when the wait begins: a card is raised once and then
/// resolved rather than removed, and the pre-ended arm writes the terminal row
/// itself before polling at all. A wait keyed on any of them returns on its
/// first iteration — which usually sampled some other partition — and the
/// assertion then reads a row no poll revisited. The claim counter cannot be
/// true in advance, because the caller reads it first.
async fn poll_until_reached(http: &reqwest::Client, run: &Scenario, awaited: &str) {
    let before = fencing_token(run).await;
    poll_until(http, run, awaited, || async {
        fencing_token(run).await > before
    })
    .await;
}

async fn deny(run: &Scenario) {
    let actions: Vec<String> =
        sqlx::query_scalar("SELECT action_id FROM fleet.approval_gates WHERE fleet_id = $1::uuid")
            .bind(&run.fleet)
            .fetch_all(&mut *run.booted.database.acquire().await.expect("connection"))
            .await
            .expect("actionable cards");
    assert_eq!(
        actions.len(),
        1,
        "a connected workspace gets one fleet approval card"
    );
    Inbox::new(
        run.booted.database.clone(),
        run.booted.queue.clone(),
        afd_admission::Admissions::for_tests(run.booted.database.clone(), run.booted.queue.clone()),
    )
    .resolve(
        actions.first().expect("card"),
        Decision::Denied,
        "fixture",
        "",
        Some(&run.fleet),
    )
    .await
    .expect("denial");
}

async fn refusal_drains(preended: bool) {
    let mut supervisor = Supervisor::new();
    let run = scenario(&mut supervisor).await;
    connect_and_declare(&run).await;
    #[expect(
        clippy::disallowed_methods,
        reason = "a plain client calls the daemon this test booted; a proxy rule and a deadline are not what the test proves"
    )]
    let http = reqwest::Client::new();
    let beat = post(&http, &run, "/v1/runners/me/heartbeats", &capable_beat()).await;
    assert_eq!(beat.status().as_u16(), 200);
    poll_until_reached(&http, &run, "the gate never saw this fleet").await;
    // Asserted rather than waited on: the wait above proves a poll reached the
    // fleet, and THIS is what that poll had to do once it did.
    assert_eq!(
        gate_cards(&run).await,
        1,
        "the poll that reached the fleet raised its approval card"
    );
    deny(&run).await;
    if preended {
        // A pre-existing terminal narrative must settle the queue without executing.
        sqlx::query("UPDATE fleet.events SET status = $3, failure_reason = $4 WHERE fleet_id = $1::uuid AND event_id = $2")
            .bind(&run.fleet).bind(&run.event_id)
            .bind(afd_core::event::status::GATE_BLOCKED)
            .bind(afd_core::event::label::GRANT_DENIED)
            .execute(&mut *run.booted.database.acquire().await.expect("database"))
            .await.expect("terminal narrative");
    }
    poll_until_reached(&http, &run, "no poll revisited the denied event").await;
    let first_label = event_column(&run, &run.event_id, LABEL).await;
    let settled: bool = sqlx::query_scalar(
        "SELECT settled_at IS NOT NULL FROM fleet.admissions WHERE id = $1::uuid",
    )
    .bind(&run.event_id)
    .fetch_one(&mut *run.booted.database.acquire().await.expect("database"))
    .await
    .expect("queue state");
    let second = run.enqueue_event(EventType::Chat).await;
    poll_until_reached(&http, &run, "no poll reached the fleet's next event").await;
    let second_label = event_column(&run, &second, LABEL).await;
    supervisor.shutdown().await;
    run.cleanup().await;
    assert_eq!(
        first_label.as_deref(),
        Some(afd_core::event::label::GRANT_DENIED)
    );
    assert!(settled, "a terminal refusal must settle its admission");
    assert_eq!(
        second_label.as_deref(),
        Some(afd_core::event::label::GRANT_DENIED),
        "the next event must be reached after the denied one"
    );
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_denied_grant_does_not_block_later_events() {
    refusal_drains(false).await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn terminal_history_is_settled_without_blocking_later_events() {
    refusal_drains(true).await;
}
