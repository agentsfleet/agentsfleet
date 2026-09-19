//! The gate chain over a claimed event, entered where a suite can steer it.
//!
//! Every gate below the claim is proven on its own elsewhere — `installed()`
//! against a paused fleet, `money_gates` against a drained ledger. What those
//! suites never reach is the ORDER above them: which of `Plane::lease`'s
//! answers each verdict becomes, and whether the terminal row is written on the
//! way. That chain runs only inside the verb, and the verb begins by asking the
//! readiness index for work — one partition per call, against a cursor the
//! whole process shares.
//!
//! So the claim is made here, naming this suite's own fleet, and the verb is
//! entered at `Plane::lease_claimed`. It runs the identical chain in the
//! identical order and returns the identical bytes; the only step it does not
//! take is choosing which event to run over.
//!
//! Marked `#[ignore]` so the unit lane compiles and lints these without
//! datastores; `make test-integration-rustd` is the only lane that runs them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::clock::UnixMillis;

use afd_crypto::entropy::Entropy;

use crate::requests::ENROLLED_AT;
use crate::seed::{MODEL, Seeded, seeded};
use crate::support::Fixtures;

/// The answer every stop on this path renders.
const NO_LEASE: &str = "\"lease\":null";

/// A stored document the runtime parser accepts, with a one-dollar ceiling.
const BUDGETED_CONFIG: &str = r#"{"name":"probe","x-agentsfleet":{"triggers":[{"type":"api"}],"tools":[],"budget":{"daily_dollars":1.0}}}"#;

/// A settled spend past [`BUDGETED_CONFIG`]'s ceiling, in nanodollars.
const OVERSPENT_NANOS: i64 = 2_000_000_000;

/// The status an operator's pause leaves on the row.
const FLEET_STATUS_STOPPED: &str = "stopped";

/// The gate kind a fixture raises over a whole event.
const KIND_EVENT: &str = "tool_call";

/// How long a fixture gate stays unexpired.
const GATE_WINDOW_MS: i64 = 600_000;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_fleet_paused_after_its_event_was_claimed_issues_no_lease() {
    // The window `installed()` documents and no suite had entered: the
    // selection pass filters on status, so a fleet reaching the claim and then
    // stopping is an operator pausing it in between. The claim must lapse on
    // its own rather than run under a fleet nobody wants running.
    crate::support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let Seeded {
        runners: [runner],
        fleet,
        ..
    } = seeded::<1>(&fixtures).await;
    let now = UnixMillis::from_millis(ENROLLED_AT);

    let claimed =
        crate::seed::select_fleet_within_rotations(&fixtures.leases(), &runner, now, &fleet)
            .await
            .expect("the fleet is leasable");
    set_status(&fixtures, &fleet, FLEET_STATUS_STOPPED).await;

    let answer = fixtures
        .plane()
        .lease_claimed(claimed, &runner, now)
        .await
        .expect("a paused fleet is a decision, not a fault");
    assert!(
        answer.contains(NO_LEASE),
        "a fleet paused mid-claim issued a lease: {answer}"
    );

    fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_fleet_past_its_ceiling_is_refused_through_the_whole_verb() {
    // `money_gates` is proven against a drained ledger next door. What runs
    // only here is what the verb DOES with that refusal: end the event, write
    // the terminal row, acknowledge the entry, and answer no-work — rather
    // than leave a delivery leasable that every poll would re-read forever.
    crate::support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let Seeded {
        runners: [runner],
        fleet,
        tenant,
        ..
    } = seeded::<1>(&fixtures).await;
    let workspace = workspace_of(&fixtures, &fleet).await;
    set_config(&fixtures, &fleet, BUDGETED_CONFIG).await;
    seed_spend(&fixtures, &tenant, &workspace, &fleet, OVERSPENT_NANOS).await;
    let now = UnixMillis::from_millis(ENROLLED_AT);

    let claimed =
        crate::seed::select_fleet_within_rotations(&fixtures.leases(), &runner, now, &fleet)
            .await
            .expect("the fleet is leasable");

    let answer = fixtures
        .plane()
        .lease_claimed(claimed, &runner, now)
        .await
        .expect("a drained budget is a decision, not a fault");
    assert!(
        answer.contains(NO_LEASE),
        "a fleet past its ceiling was issued a lease: {answer}"
    );

    fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_denied_gate_ends_the_event_rather_than_parking_it() {
    // `of_gate` maps a denial to `Admission::Refuse`, and the verb must then
    // END the event: a human said no, so waiting would offer the same event
    // back on every poll forever. The pairing with the test below is the
    // claim — denial and pending are one `Verdict` enum at the gate and two
    // different endings here.
    crate::support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let Seeded {
        runners: [runner],
        fleet,
        event_id,
        ..
    } = seeded::<1>(&fixtures).await;
    set_config(&fixtures, &fleet, BUDGETED_CONFIG).await;
    seed_gate(&fixtures, &fleet, &event_id, "denied").await;
    let now = UnixMillis::from_millis(ENROLLED_AT);

    let claimed =
        crate::seed::select_fleet_within_rotations(&fixtures.leases(), &runner, now, &fleet)
            .await
            .expect("the fleet is leasable");

    let answer = fixtures
        .plane()
        .lease_claimed(claimed, &runner, now)
        .await
        .expect("a denied gate is a decision, not a fault");
    assert!(
        answer.contains(NO_LEASE),
        "a denied gate issued a lease: {answer}"
    );

    fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_an_unanswered_gate_parks_the_event_without_ending_it() {
    // The mirror of the denial above: nobody has answered yet, so the event
    // must be left alone. Ending it here would throw away work a human is
    // still deciding about, and the runner is told to wait exactly as it is
    // told to wait for an empty queue.
    crate::support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let Seeded {
        runners: [runner],
        fleet,
        event_id,
        ..
    } = seeded::<1>(&fixtures).await;
    set_config(&fixtures, &fleet, BUDGETED_CONFIG).await;
    seed_gate(&fixtures, &fleet, &event_id, "pending").await;
    let now = UnixMillis::from_millis(ENROLLED_AT);

    let claimed =
        crate::seed::select_fleet_within_rotations(&fixtures.leases(), &runner, now, &fleet)
            .await
            .expect("the fleet is leasable");

    let answer = fixtures
        .plane()
        .lease_claimed(claimed, &runner, now)
        .await
        .expect("an unanswered gate is a decision, not a fault");
    assert!(
        answer.contains(NO_LEASE),
        "an unanswered gate issued a lease: {answer}"
    );

    fixtures.cleanup().await;
}

/// Writes one gate over `event`, in whatever state `status` names.
///
/// The column list mirrors `integration_credential_mint.rs`'s write-gate seed,
/// because `core.fleet_approval_gates` carries several NOT NULL columns a
/// shorter insert discovers one round trip at a time — `resolved_by` among
/// them, which is required even on a row nobody has resolved.
async fn seed_gate(fixtures: &Fixtures, fleet: &str, event: &str, status: &str) {
    let workspace = workspace_of(fixtures, fleet).await;
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO core.fleet_approval_gates \
           (id, fleet_id, workspace_id, action_id, tool_name, action_name, \
            gate_kind, proposed_action, evidence, blast_radius, timeout_at, \
            resolved_by, status, detail, created_at, updated_at, event_id) \
         VALUES ($1::uuid, $2::uuid, $3::uuid, $4, 'chat', 'run', \
                 $5, 'run the event', '{}'::jsonb, 'one fleet', \
                 $6, 'fixture:human', $7, '', $8, $8, $9)",
    )
    .bind(ledger_id())
    .bind(fleet)
    .bind(&workspace)
    .bind(ledger_id())
    .bind(KIND_EVENT)
    .bind(ENROLLED_AT + GATE_WINDOW_MS)
    .bind(status)
    .bind(ENROLLED_AT)
    .bind(event)
    .execute(&mut *connection)
    .await
    .expect("the gate row must insert");
}

/// Replaces a seeded fleet's status.
async fn set_status(fixtures: &Fixtures, fleet: &str, status: &str) {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query("UPDATE core.fleets SET status = $2 WHERE id = $1::uuid")
        .bind(fleet)
        .bind(status)
        .execute(&mut *connection)
        .await
        .expect("the status must update");
}

/// Replaces a seeded fleet's stored configuration.
async fn set_config(fixtures: &Fixtures, fleet: &str, config_json: &str) {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query("UPDATE core.fleets SET config_json = $2::jsonb WHERE id = $1::uuid")
        .bind(fleet)
        .bind(config_json)
        .execute(&mut *connection)
        .await
        .expect("the config must update");
}

/// The workspace a seeded fleet belongs to.
async fn workspace_of(fixtures: &Fixtures, fleet: &str) -> String {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query_scalar::<_, String>(
        "SELECT workspace_id::text FROM core.fleets WHERE id = $1::uuid",
    )
    .bind(fleet)
    .fetch_one(&mut *connection)
    .await
    .expect("a seeded fleet has a workspace")
}

/// Seeds one settled ledger row draining `nanos` for `fleet`.
async fn seed_spend(fixtures: &Fixtures, tenant: &str, workspace: &str, fleet: &str, nanos: i64) {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO billing.usage_ledger \
           (id, tenant_id, workspace_id, fleet_id, event_id, charge_type, posture, model, \
            credit_deducted_nanos, event_created_at, created_at, last_charged_at) \
         VALUES ($8::uuid, $1::uuid, $2::uuid, $3::uuid, $4, 'receive', 'platform', $5, \
                 $6, $7, $7, $7)",
    )
    .bind(tenant)
    .bind(workspace)
    .bind(fleet)
    .bind(format!("event-lease-gates-spent-{fleet}"))
    .bind(MODEL)
    .bind(nanos)
    .bind(ENROLLED_AT)
    .bind(ledger_id())
    .execute(&mut *connection)
    .await
    .expect("the spend seeds");
}

/// A fresh version-7 identifier for a ledger row.
///
/// `billing.usage_ledger` constrains its primary key to the v7 spelling
/// (`ck_usage_ledger_id_uuidv7`), so a `gen_random_uuid()` default is refused.
/// Drawn through the workspace's own entropy surface, as the sibling suites do,
/// rather than through a random crate.
fn ledger_id() -> String {
    let mut bytes = [0u8; afd_core::id::ENTROPY_LEN];
    Entropy::new()
        .fill(&mut bytes)
        .expect("the host provides entropy");
    afd_core::id::Uuid7::encode(UnixMillis::from_millis(ENROLLED_AT), bytes)
        .expect("a v7 identifier encodes")
        .as_str()
        .to_owned()
}
