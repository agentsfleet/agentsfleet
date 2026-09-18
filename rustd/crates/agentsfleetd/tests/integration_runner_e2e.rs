//! §7 — the parity harness: a runner's whole loop against the daemon that ships.
//!
//! Dimensions 7.1 and 7.2, and the only suite in this workspace where the
//! request crosses a real socket into a real `agentsfleetd-rs` process graph.
//! Everything else proves a store verb or a router in isolation; this proves the
//! two were wired to each other.
//!
//! # What this catches that the other suites cannot
//!
//! `afd_fleet`'s integration suites call `Leases::select`/`issue`/`report`
//! directly, so they prove the SQL. `afd_api`'s router suites drive the real
//! router over a store with no datastore behind it, so they prove the guard
//! order. Neither notices when a verb is served on a path no runner sends to,
//! when the daemon boots without mounting a route, when the identity the guard
//! resolves is not the identity the store scopes by, or when a lease payload the
//! plane assembles cannot round-trip through the wire types a runner parses.
//! Every one of those is a wiring defect that only appears end to end.
//!
//! # Why there is no live Zig daemon on the other side
//!
//! §7 records the trade: M175 §6 deleted the Zig integration lanes, so no second
//! daemon exists to diff rows against. What replaces the differ is
//! [`test_seeded_row_shapes`] — the ported statements' own output, pinned as a
//! recorded shape — and it is named as WEAKER than a cross-implementation diff
//! rather than presented as equivalent.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "integration_runner_e2e/money_gate.rs"]
mod money_gate;
#[path = "integration_runner_e2e/refusals.rs"]
mod refusals;

use agentsfleetd::supervisor::Supervisor;
use serde_json::{Value, json};

use crate::e2e::{Scenario, scenario};
use crate::reads::{balance, counter_column, event_column, lease_column, lease_rows, ledger_rows};
use crate::wire::{
    MEMORY_CATEGORY, MEMORY_CONTENT, MEMORY_KEY, UNKNOWN_TOKEN,
    assert_no_lease_for_fleet_under_test, capable_beat, field, get, json, poll_for_seeded_lease,
    poll_until, post, report_body,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_runner_suite_vs_rust_daemon() {
    let mut supervisor = Supervisor::new();
    let run = scenario(&mut supervisor).await;
    let http = reqwest::Client::new();

    refusals::assert_unknown_credential_is_refused(&http, &run).await;
    prove_runner_ready(&http, &run).await;
    let (lease_id, fence) = claim_seeded_lease(&http, &run).await;
    prove_live_lease_satellites(&http, &run, &lease_id).await;
    capture_memory(&http, &run, &lease_id, fence).await;
    settle_report(&http, &run, &lease_id, fence).await;
    refusals::assert_unsupported_event_ends(&http, &run).await;

    supervisor.shutdown().await;
    run.cleanup().await;
}

async fn prove_runner_ready(http: &reqwest::Client, run: &Scenario) {
    let self_record = get(http, run, "/v1/runners/me").await;
    assert_eq!(
        self_record.status().as_u16(),
        200,
        "an enrolled runner reads its own row"
    );
    assert_eq!(
        field(&json(self_record).await, "id"),
        &json!(run.runner_id.as_str()),
        "the row it reads is the one enrolment minted the token for — the guard \
         and the store scope by the same identity"
    );

    let beat = post(http, run, "/v1/runners/me/heartbeats", &capable_beat()).await;
    assert_eq!(beat.status().as_u16(), 200, "a beat is accepted");
    assert_eq!(
        field(&json(beat).await, "degraded"),
        &json!(false),
        "and the proven capabilities clear the degraded verdict, which is what \
         makes the poll below leasable rather than fail-closed"
    );
}

async fn claim_seeded_lease(http: &reqwest::Client, run: &Scenario) -> (String, u64) {
    // One caller, one poller. There used to be a bare POST here to prove the
    // STATUS before the loop proved the EVENT, on the reading that a poll which
    // misses this fleet's partition costs nothing — and about fifteen times in
    // sixteen that is true. The sixteenth is the bug: that throwaway request
    // WINS the lease, its body is dropped on the floor, and the lease is now
    // held. `fleet.runner_affinity` then refuses to re-issue it while
    // `leased_until` is in the future, so every one of the 128 polls below
    // correctly answers `lease: null` and the test panics saying the event was
    // never offered. Measured at 2 failures in 15 local runs, and it is the red
    // `test-integration-rustd` lane on Pull Request #693.
    //
    // `poll_for_lease` already asserts the status on EVERY turn and its doc
    // comment names this exact caller as the hazard, so the separate request
    // proved nothing the loop does not prove and cost a lease to do it.
    poll_for_seeded_lease(http, run).await
}

async fn capture_memory(http: &reqwest::Client, run: &Scenario, lease_id: &str, fence: u64) {
    let captured = post(
        http,
        run,
        &format!("/v1/runners/me/memory/{}", run.fleet),
        &json!({
            "lease_id": lease_id,
            "fencing_token": fence,
            "memory": [{
                "key": MEMORY_KEY,
                "content": MEMORY_CONTENT,
                "category": MEMORY_CATEGORY,
            }],
        }),
    )
    .await;
    assert_eq!(
        captured.status().as_u16(),
        200,
        "the holder of the current fence may write the fleet's memory"
    );
}

async fn settle_report(http: &reqwest::Client, run: &Scenario, lease_id: &str, fence: u64) {
    let before = balance(run).await;
    let report = report_body(lease_id, &run.event_id, fence);
    let settled = post(http, run, "/v1/runners/me/reports", &report).await;
    assert_eq!(settled.status().as_u16(), 200, "the report is accepted");
    assert_eq!(
        json(settled).await,
        json!({"ok": true}),
        "and says so in the shape a runner parses"
    );

    assert_settled(run, lease_id, before).await;
    assert_replay_returns_the_stored_outcome(http, run, lease_id, &report).await;
}

/// The side-channel verbs operate on the same live lease and identity.
async fn prove_live_lease_satellites(http: &reqwest::Client, run: &Scenario, lease_id: &str) {
    assert_memory_hydrates(http, run).await;
    assert_lease_renews(http, run, lease_id).await;
    assert_activity_degrades_gracefully(http, run, lease_id).await;
    refusals::assert_credential_and_duplicate_refusals(http, run, lease_id).await;
}

async fn assert_memory_hydrates(http: &reqwest::Client, run: &Scenario) {
    let hydrated = get(http, run, &format!("/v1/runners/me/memory/{}", run.fleet)).await;
    assert_eq!(hydrated.status().as_u16(), 200);
    assert_eq!(json(hydrated).await, json!({"memory": []}));
}

async fn assert_lease_renews(http: &reqwest::Client, run: &Scenario, lease_id: &str) {
    let renewed = post(
        http,
        run,
        &format!("/v1/runners/me/leases/{lease_id}/renew"),
        &json!({"input_tokens": 10, "cached_input_tokens": 2, "output_tokens": 3}),
    )
    .await;
    assert_eq!(renewed.status().as_u16(), 200);
    assert!(
        field(&json(renewed).await, "lease_expires_at")
            .as_i64()
            .is_some_and(|expires| expires > run.seeded_at.as_millis()),
        "renewal returns the live deadline the datastore advanced"
    );
}

async fn assert_activity_degrades_gracefully(
    http: &reqwest::Client,
    run: &Scenario,
    lease_id: &str,
) {
    let activity = post(
        http,
        run,
        &format!("/v1/runners/me/leases/{lease_id}/activity"),
        &json!({
            "frames": [
                {"tool_call_started": {"name": "search", "args_redacted": "not-json"}},
                {"tool_call_progress": {"name": "search", "elapsed_ms": 10}},
                {"fleet_response_chunk": {"text": "working"}},
                {"tool_call_completed": {"name": "search", "ms": 20}}
            ]
        }),
    )
    .await;
    assert_eq!(
        activity.status().as_u16(),
        202,
        "an invalid cosmetic frame is dropped without failing the live run"
    );
}

/// Dimension 7.2 — the ported statements fill the columns they are supposed to.
///
/// The differ M175 §6 deleted, replaced by the weaker claim §7 says it is: not
/// "these rows equal the Zig's", which nothing can check any more, but "these
/// rows carry every column the ported statement was written to fill". A port
/// that drops a column from an `INSERT` list still compiles, still returns
/// `Ok`, and still passes every behavioural assertion that reads some OTHER
/// column — this is the test that does not.
///
/// Four tables, because those are the four Dimension 7.2 names and they are
/// written by
/// report, because a draw is only meaningful against what preceded it.
async fn assert_settled(run: &Scenario, lease_id: &str, before: Option<i64>) {
    assert_eq!(
        lease_column(run, lease_id, "status").await.as_deref(),
        Some("reported"),
        "the lease is flipped, which is what stops the reclaim sweep re-issuing it"
    );
    let after = balance(run).await;
    assert!(
        after < before,
        "a priced run draws the tenant's wallet down: {before:?} → {after:?}"
    );
    assert_eq!(
        ledger_rows(run).await,
        2,
        "one receive row and one stage row — the two-rows-per-event invariant, \
         proven here across the two REQUESTS that write them rather than within one store call"
    );
    assert_eq!(
        counter_column(run, "succeeded").await.as_deref(),
        Some("1"),
        "the lifetime tally counts the completed run on the succeeded arm"
    );
}

/// A second delivery of the same report is answered, and changes nothing.
///
/// The guard is still the lease's `status = active` predicate and a report that
/// claims no row still writes none: no ledger row, no wallet draw, no tally.
/// What CHANGED is the answer. A replay used to be a 409, on the reading that a
/// result nobody is waiting for is not worth retrying — and that reading is
/// wrong in the one case a replay actually happens. A runner replays because it
/// never received the first response, so the run whose answer the platform is
/// refusing to acknowledge is a run the platform already charged for and
/// already stored. Told 409, the runner discards a finished result; told 200,
/// it stops retrying with its work safely landed.
///
/// The route to both is the lease id, which is this endpoint's idempotency key.
/// A fleet the runner has genuinely been superseded on still answers 409,
/// because the lease is not `reported` there — that refusal is proven against
/// live rows in the fleet plane's own suites, not here.
async fn assert_replay_returns_the_stored_outcome(
    http: &reqwest::Client,
    run: &Scenario,
    lease_id: &str,
    report: &Value,
) {
    let drawn = balance(run).await;
    let replay = post(http, run, "/v1/runners/me/reports", report).await;
    assert_eq!(
        replay.status().as_u16(),
        200,
        "the second delivery is acknowledged, so the runner stops retrying a result \
         the platform has already stored and already charged for"
    );
    assert_eq!(
        json(replay).await,
        json!({"ok": true}),
        "and in the same shape the first one answered — a runner parses one reply, not two"
    );
    assert_eq!(
        ledger_rows(run).await,
        2,
        "and it stays at two rows: nothing claimed, so nothing charged"
    );
    assert_eq!(
        balance(run).await,
        drawn,
        "a repeated report charges nothing at all"
    );
    assert_eq!(
        lease_column(run, lease_id, "status").await.as_deref(),
        Some("reported"),
        "and mutates nothing — the lease reads exactly as the first report left it"
    );
    assert_eq!(
        counter_column(run, "succeeded").await.as_deref(),
        Some("1"),
        "the lifetime tally counts the run once, because it is gated on the claim \
         that the repeat did not win. The handler's product funnel and cost meters \
         are skipped on the same arm and export nowhere a test can read, so this \
         row is the durable half of that claim and the arm itself is the reviewed half"
    );
}
