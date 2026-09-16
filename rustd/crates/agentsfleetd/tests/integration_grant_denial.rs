//! A connected workspace still needs fleet approval; denial must drain the queue.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test preconditions must fail loudly"
)]

use crate::e2e::{GOOD_KEK, Scenario, scenario};
use crate::reads::event_column;
use crate::wire::{assert_no_lease_for_fleet_under_test, capable_beat, json, post};
use afd_approval::{Decision, Inbox};
use afd_core::id::Uuid7;
use afd_crypto::{entropy::Entropy, secret::Kek};
use afd_datastore::FleetStreams;
use afd_fleet::lease::{Leases, runner_consumer};
use afd_vault::{SecretBody, SecretName, Vault};
use afd_wire::event::EventType;
use agentsfleetd::supervisor::Supervisor;
use serde_json::json;
use std::sync::Arc;

const SERVICE: &str = "github";
const LEASES: &str = "/v1/runners/me/leases";
const LABEL: &str = "COALESCE(failure_label, '')";

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
    sqlx::query("UPDATE core.fleets SET config_json = jsonb_set(config_json, '{x-agentsfleet,credentials}', $2::jsonb) WHERE id = $1::uuid")
        .bind(&run.fleet).bind(json!([SERVICE]).to_string())
        .execute(&mut *run.booted.database.acquire().await.expect("connection"))
        .await.expect("declare credential");
    // Prefer this fixture's fleet over unrelated ready marks left by other suites.
    Leases::new(
        run.booted.database.clone(),
        run.booted.queue.clone(),
        Entropy::new(),
    )
    .claim(
        &Uuid7::parse(&run.fleet).expect("fleet"),
        &run.runner_id,
        run.seeded_at,
        0,
    )
    .await
    .expect("initial affinity claim")
    .expect("new fleet is unclaimed");
}

/// How many approval cards this fixture's fleet has raised.
async fn gate_cards(run: &Scenario) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM core.fleet_approval_gates WHERE fleet_id = $1::uuid")
        .bind(&run.fleet)
        .fetch_one(&mut *run.booted.database.acquire().await.expect("connection"))
        .await
        .expect("the gate count must run")
}

/// Polls a full rotation budget, ending when `reached` says the daemon has
/// done the thing under test to this fleet.
///
/// # Why one request is not enough
///
/// Readiness is sixteen partitions and a poll reads one of them, so a single
/// request reaches a given fleet about one time in sixteen. The trap is that
/// the assertion still passes when it does not: a poll that never looked at
/// this fleet answers `lease: null`, which is exactly what a gate-blocked
/// fleet answers too. The test then went on to assert a card that nothing had
/// raised, and failed several steps later with `0 != 1`, pointing at the gate
/// rather than at the poll.
///
/// # Why the condition is a parameter
///
/// It was `gate_cards(run) > 0` for every call, and a card is raised ONCE and
/// then resolved rather than removed — so every poll after the first returned
/// on its first iteration, having usually sampled some other partition. The
/// denial was then asserted against an event no poll had revisited. Each call
/// site now names the row IT is waiting for: the card for the gate, the
/// event's own label for the refusals that follow it.
///
/// Every response on the way is still checked, because "no work" is the answer
/// under test and a 200 carrying a lease would mean the gate let it through.
async fn poll_until<F, Fut>(http: &reqwest::Client, run: &Scenario, awaited: &str, mut reached: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    const ROTATIONS: u16 = 8;
    for _poll in 0..(afd_datastore::ready::READY_PARTITIONS * ROTATIONS) {
        // A no-work poll retains its affinity claim until expiry. Move that
        // deadline into the past so this test reaches redelivery without a
        // wall-clock sleep. Redis remains untouched: a missing acknowledgment
        // must still be visible.
        sqlx::query("UPDATE fleet.runner_affinity SET leased_until = $2 WHERE fleet_id = $1::uuid")
            .bind(&run.fleet)
            .bind(afd_core::clock::now().as_millis() - 1)
            .execute(&mut *run.booted.database.acquire().await.expect("connection"))
            .await
            .expect("expire the previous no-work claim");

        let response = post(http, run, LEASES, &json!({})).await;
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
    panic!(
        "{awaited} never happened in {ROTATIONS} rotations of the readiness index — \
         every poll answered no-work, so either no poll reached this fleet's partition \
         or the daemon declined it for a reason only its own log carries \
         (`AFD_TEST_LOG=1`)."
    )
}

/// The fleet's monotonic claim counter.
///
/// `CLAIM_AFFINITY_SLOT` bumps it on every won claim, and this suite expires
/// the deadline before each poll, so a poll that reaches this fleet's partition
/// always wins the claim and always moves this number. That makes it the one
/// observable meaning "the daemon looked here" — as opposed to the rows the
/// callers assert on, which say what it decided once it did.
async fn fencing_seq(run: &Scenario) -> i64 {
    sqlx::query_scalar("SELECT fencing_seq FROM fleet.runner_affinity WHERE fleet_id = $1::uuid")
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
    let before = fencing_seq(run).await;
    poll_until(http, run, awaited, || async {
        fencing_seq(run).await > before
    })
    .await;
}

async fn deny(run: &Scenario) {
    let actions: Vec<String> = sqlx::query_scalar(
        "SELECT action_id FROM core.fleet_approval_gates WHERE fleet_id = $1::uuid",
    )
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
        afd_core::clock::now(),
    )
    .await
    .expect("denial");
}

async fn refusal_drains(preended: bool) {
    let mut supervisor = Supervisor::new();
    let run = scenario(&mut supervisor).await;
    connect_and_declare(&run).await;
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
        // The durable half succeeded before a crash or a failed Redis acknowledgment.
        Leases::new(
            run.booted.database.clone(),
            run.booted.queue.clone(),
            Entropy::new(),
        )
        .block(
            &Uuid7::parse(&run.fleet).expect("fleet"),
            &run.event_id,
            afd_fleet::lease::admit::Refusal {
                label: afd_core::event::label::GRANT_DENIED,
                detail: "",
            },
            afd_core::clock::now(),
        )
        .await
        .expect("terminal write before retry");
    }
    poll_until_reached(&http, &run, "no poll revisited the denied event").await;
    let first_label = event_column(&run, &run.event_id, LABEL).await;
    let pending = FleetStreams::new(run.booted.queue.clone())
        .read_pending(&run.fleet, &runner_consumer())
        .await
        .expect("pending read");
    let second = run.enqueue_event(EventType::Chat).await;
    poll_until_reached(&http, &run, "no poll reached the fleet's next event").await;
    let second_label = event_column(&run, &second, LABEL).await;
    supervisor.shutdown().await;
    run.cleanup().await;
    assert_eq!(
        first_label.as_deref(),
        Some(afd_core::event::label::GRANT_DENIED)
    );
    assert!(
        pending.is_none(),
        "a terminal refusal must acknowledge its pending entry"
    );
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
async fn a_terminal_refusal_redelivery_retries_acknowledgment() {
    refusal_drains(true).await;
}
