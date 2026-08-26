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

mod support;

#[path = "support/e2e_db.rs"]
mod e2e_db;

#[path = "support/e2e_seed.rs"]
mod e2e_seed;

#[path = "support/e2e.rs"]
mod e2e;

#[path = "support/e2e_wire.rs"]
mod wire;

#[path = "support/e2e_reads.rs"]
mod reads;

use agentsfleetd::supervisor::Supervisor;
use serde_json::{Value, json};

use self::e2e::{Scenario, scenario};
use self::reads::{balance, counter_column, lease_column, lease_rows, ledger_rows};
use self::wire::{
    MEMORY_CATEGORY, MEMORY_CONTENT, MEMORY_KEY, UNKNOWN_TOKEN, capable_beat, claim, field, get,
    json, post, report_body,
};

/// for nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn test_runner_suite_vs_rust_daemon() {
    let mut supervisor = Supervisor::new();
    let run = scenario(&mut supervisor).await;
    let http = reqwest::Client::new();

    // ── The credential is refused before anything else is proven ────────────
    // First, so a suite that somehow authenticated nothing cannot report green
    // on the verbs below.
    let unknown = http
        .get(format!("{}/v1/runners/me", run.base))
        .bearer_auth(UNKNOWN_TOKEN)
        .send()
        .await
        .expect("the daemon answers an unknown credential");
    assert_eq!(
        unknown.status().as_u16(),
        401,
        "a well-formed token belonging to no row is refused by the directory"
    );

    // ── Self-read and beat ──────────────────────────────────────────────────
    let self_record = get(&http, &run, "/v1/runners/me").await;
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

    let beat = post(&http, &run, "/v1/runners/me/heartbeats", &capable_beat()).await;
    assert_eq!(beat.status().as_u16(), 200, "a beat is accepted");
    assert_eq!(
        field(&json(beat).await, "degraded"),
        &json!(false),
        "and the proven capabilities clear the degraded verdict, which is what \
         makes the poll below leasable rather than fail-closed"
    );

    // ── The lease ───────────────────────────────────────────────────────────
    let leased = post(&http, &run, "/v1/runners/me/leases", &json!({})).await;
    assert_eq!(
        leased.status().as_u16(),
        200,
        "work and no-work are the same status on this verb"
    );
    let body = json(leased).await;
    let lease = body
        .get("lease")
        .filter(|value| !value.is_null())
        .expect("the seeded fleet is leasable, so the poll carries work");
    assert_eq!(
        field(field(lease, "event"), "event_id"),
        &json!(run.event_id),
        "the daemon handed back the event this scenario put on the stream"
    );
    let (lease_id, fence) = claim(lease);

    // ── Memory, under the lease's fence ─────────────────────────────────────
    let captured = post(
        &http,
        &run,
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

    // ── The report ──────────────────────────────────────────────────────────
    let before = balance(&run).await;
    let report = report_body(&lease_id, &run.event_id, fence);
    let settled = post(&http, &run, "/v1/runners/me/reports", &report).await;
    assert_eq!(settled.status().as_u16(), 200, "the report is accepted");
    assert_eq!(
        json(settled).await,
        json!({"ok": true}),
        "and says so in the shape a runner parses"
    );

    assert_settled(&run, &lease_id, before).await;

    assert_replay_is_fenced(&http, &run, &lease_id, &report).await;

    supervisor.shutdown().await;
    run.cleanup().await;
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

/// A second delivery of the same report claims nothing and changes nothing.
///
/// The guard is the lease's `status = active` predicate, and a report that
/// claims no row writes none: no ledger row, no wallet draw, no tally. What the
/// runner gets back is a refusal rather than an acknowledgement, because a
/// result nobody is waiting for is not something to retry.
async fn assert_replay_is_fenced(
    http: &reqwest::Client,
    run: &Scenario,
    lease_id: &str,
    report: &Value,
) {
    let drawn = balance(run).await;
    let replay = post(http, run, "/v1/runners/me/reports", report).await;
    assert_eq!(
        replay.status().as_u16(),
        409,
        "the lease is no longer active, so the second delivery cannot claim it — \
         a conflict, which is terminal for the run and tells the runner to discard \
         rather than to back off and retry"
    );
    assert_eq!(
        ledger_rows(run).await,
        2,
        "and it stays at two rows: nothing claimed, so nothing charged"
    );
    assert_eq!(
        balance(run).await,
        drawn,
        "a fenced replay charges nothing at all"
    );
    assert_eq!(
        lease_column(run, lease_id, "status").await.as_deref(),
        Some("reported"),
        "and mutates nothing — the lease reads exactly as the first report left it"
    );
}

/// Dimension 2.3 — an exhausted tenant is refused at issue, and nothing is
/// written.
///
/// The gate that makes every other money assertion matter: without it a fleet
/// whose tenant went to zero would keep leasing work nobody is paying for. It
/// belongs beside the loop above rather than in `afd_fleet`'s suites because
/// the credits gate is only reached on the PULL path — the store verbs those
/// suites drive skip it entirely — and reaching it needs all six preconditions
/// `e2e_seed.rs` documents.
///
/// The event is not merely left unleased. It is ENDED: a refusal a runner can
/// do nothing about must not sit on the stream being re-delivered forever, so
/// the poll marks it terminal and answers no-work.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn test_lease_money_gate_refusal() {
    let mut supervisor = Supervisor::new();
    let run = scenario(&mut supervisor).await;
    let http = reqwest::Client::new();

    // Drained AFTER the seed, so the wallet row exists and holds zero — the
    // case the gate refuses, as distinct from the absent row it admits.
    run.drain_wallet().await;

    let beat = post(&http, &run, "/v1/runners/me/heartbeats", &capable_beat()).await;
    assert_eq!(
        beat.status().as_u16(),
        200,
        "the runner proves its capabilities, so the poll below is refused for \
         MONEY rather than for a degraded verdict"
    );

    let polled = post(&http, &run, "/v1/runners/me/leases", &json!({})).await;
    assert_eq!(
        polled.status().as_u16(),
        200,
        "a refused event is still an answered poll — work and no-work are the \
         same status on this verb, and an exhausted tenant is no-work"
    );
    assert_eq!(
        field(&json(polled).await, "lease"),
        &json!(null),
        "and it carries no lease"
    );

    assert_eq!(
        lease_rows(&run).await,
        0,
        "no partial write: the gate refuses BEFORE the lease row, so an \
         exhausted tenant leaves nothing for the reclaim sweep to find"
    );
    assert_eq!(
        balance(&run).await,
        Some(0),
        "and nothing was charged against a wallet that had nothing to give"
    );
}
