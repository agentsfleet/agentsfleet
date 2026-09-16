//! The runner half of §7: what a stock runner sends, and how it reads answers.
//!
//! Split from the suites by concern rather than by size (RULE FLL): these are
//! the REQUEST-side helpers — the bodies a runner puts on the wire and the
//! accessors that read one back. Nothing here touches a row; the row half is
//! `e2e_reads.rs` and the fixture half is `e2e.rs`.
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use afd_dragonfly::ready::READY_PARTITIONS;
use serde_json::{Value, json};

use crate::e2e::Scenario;

/// How many rotations of the readiness index a poll is given to reach a
/// specific fleet. See [`poll_for_seeded_lease`].
const ROTATIONS: u16 = 8;

/// A runner credential belonging to no row: well-formed, sixty-four hex
/// characters after the marker, so the refusal comes from the DIRECTORY rather
/// than from the shape check in front of it.
pub(crate) const UNKNOWN_TOKEN: &str =
    "agt_rffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";

/// The memory this run remembers, and the key the shape assertion reads back.
pub(crate) const MEMORY_KEY: &str = "e2e-observation";
pub(crate) const MEMORY_CONTENT: &str = "the runner completed a lease against the Rust daemon";
pub(crate) const MEMORY_CATEGORY: &str = "core";

/// Cumulative token counts the report settles against.
///
/// Large enough that the charge is non-zero at the seeded catalogue rates — a
/// settle that drew nothing would make the wallet assertion pass for the wrong
/// reason.
const INPUT_TOKENS: u32 = 12_000;
const CACHED_INPUT_TOKENS: u32 = 3_000;
pub(crate) const OUTPUT_TOKENS: u32 = 4_000;

/// The beat a stock runner sends once its startup probe has answered.
///
/// The capability report is NOT optional decoration. A freshly enrolled row
/// carries no probe result, so its verdict reads degraded and `Plane::lease`
/// answers no-work with `"the runner's verdict is degraded or unreadable"` —
/// fail-closed, and correct: a host that has not proven it can enforce the
/// assigned sandbox must not be handed work. A runner therefore beats its
/// capabilities BEFORE its first poll, and a §7 suite that skipped the beat
/// would be asserting against a state no runner is ever in.
pub(crate) fn capable_beat() -> Value {
    json!({
        "capability_report": {
            "landlock": true,
            "seccomp": true,
            "cgroup_controllers": ["cpu", "memory", "pids"],
            "bubblewrap": true,
            "egress_enforcement": true,
        },
        "selftest": null,
    })
}

/// A lease payload's identifier and fence, which every verb after the poll
/// echoes.
///
/// Read together because they are only ever used together, and a helper that
/// answered one of them would be a second place for a caller to pair a lease
/// with the wrong fence.
pub(crate) fn claim(lease: &Value) -> (String, u64) {
    (
        field(lease, "lease_id")
            .as_str()
            .expect("a lease carries its identifier")
            .to_owned(),
        field(lease, "fencing_token")
            .as_u64()
            .expect("a lease carries its fence"),
    )
}

/// Polls until the seeded event is handed over, and claims it.
///
/// # Why one poll is not enough
///
/// Readiness is sixteen partitions and a poll reads ONE of them, the next in a
/// rotation the process keeps. A single request therefore reaches a given
/// fleet about one time in sixteen, so every suite that polled once and
/// expected work was a lottery it usually lost — and it failed with
/// `lease: null`, which is a documented, entirely valid answer, so the
/// failures read as "the fleet was not leasable" rather than "the poll never
/// looked there".
///
/// # Why it also checks WHICH event came back
///
/// Even with the index swept, the assertion is on this scenario's own event
/// rather than on whatever arrives: a fixture that appended twice has two
/// entries owed, and the oldest is the one these suites are written against.
pub(crate) async fn poll_for_seeded_lease(http: &reqwest::Client, run: &Scenario) -> (String, u64) {
    poll_for_lease(http, run, &run.event_id).await
}

/// [`poll_for_seeded_lease`], for any event the scenario has put on its fleet.
pub(crate) async fn poll_for_lease(
    http: &reqwest::Client,
    run: &Scenario,
    event_id: &str,
) -> (String, u64) {
    for _poll in 0..(READY_PARTITIONS * ROTATIONS) {
        let response = post(http, run, "/v1/runners/me/leases", &json!({})).await;
        // Asserted on EVERY turn, not once before the loop: a caller that
        // polled separately to check the status would consume the lease this
        // loop is looking for, and then never find it.
        let status = response.status().as_u16();
        let body = json(response).await;
        assert_eq!(
            status, 200,
            "work and no-work share a status on this verb: {body}"
        );
        let Some(lease) = body.get("lease").filter(|value| !value.is_null()) else {
            continue;
        };
        if field(field(lease, "event"), "event_id") == &json!(event_id) {
            return claim(lease);
        }
    }
    panic!(
        "the event {event_id} was never offered in {ROTATIONS} rotations of the \
         readiness index.\n\
         \n\
         Every poll answered 200 with `lease: null`, so the daemon did not fail: \
         it found nothing it would hand this runner. Read the daemon's own log \
         before theorising — `AFD_TEST_LOG=1` is the switch; `RUST_LOG` alone \
         writes to a sink — and find the poll that DID reach the fleet's \
         partition, one in {READY_PARTITIONS}: the reason it declined is on \
         that line. The last time this fired, the line was \
         `assign_entry_undecodable_dropped`: the seed had appended the \
         pre-ledger field set, the reader refused the entry for want of \
         `event_id`, dropped it so the fleet stayed leasable, and every later \
         poll correctly found the stream empty. The seed now admits through the \
         ledger (`e2e_event.rs`).\n\
         \n\
         Also worth ruling out: a fleet in the lane database whose `config_json` \
         will not parse — the pull path resolves one and refuses the runner \
         with UZ-INTERNAL-003 — and a runner already holding a lease, since a \
         runner holds one lease and a poll that lands on residue takes the slot."
    )
}

/// Polls a full rotation budget while `ended` says the work under test is
/// still open, and answers whether it closed.
///
/// For the arms where the RIGHT answer is no work — an unsupported type the
/// daemon must end, a gate that must block — and where that answer is also
/// what a poll that never reached the fleet's partition says. The poll's
/// answer cannot tell the two apart, so the exit condition is the caller's
/// own row read, and every answer on the way is still checked: a 200 carrying
/// a lease would mean the daemon handed out the very work it must refuse.
pub(crate) async fn poll_until<F, Fut>(http: &reqwest::Client, run: &Scenario, mut ended: F) -> bool
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    for _poll in 0..(READY_PARTITIONS * ROTATIONS) {
        let response = post(http, run, "/v1/runners/me/leases", &json!({})).await;
        assert_eq!(
            response.status().as_u16(),
            200,
            "no-work is a 200 on this verb"
        );
        assert_no_lease_for_fleet_under_test(run, &json(response).await);
        if ended().await {
            return true;
        }
    }
    false
}

/// Asserts the answer carries no lease for the fleet under test.
///
/// Scoped to that fleet rather than asserting `lease: null`, because a lease
/// for ANOTHER fleet is the shared lane rather than a defect: this database
/// carries every earlier scenario's fleets, and the reclaim sweeper re-marks
/// any whose stream still holds work. Nothing in the daemon stops one runner
/// holding leases on several fleets — every `fleet.runner_leases` statement is
/// scoped by lease id AND runner id for OWNERSHIP, and none asks whether the
/// runner already holds one — so a lease taken here costs the caller nothing
/// and is left alone. Handing it back would mean reporting an event this suite
/// does not own, to a fleet another crate's suite may be waiting on.
pub(crate) fn assert_no_lease_for_fleet_under_test(run: &Scenario, body: &Value) {
    let Some(lease) = body.get("lease").filter(|value| !value.is_null()) else {
        return;
    };
    assert_ne!(
        field(field(lease, "event"), "fleet_id"),
        &json!(run.fleet),
        "the daemon must not hand out the work under test"
    );
}

/// The report one completed run sends.
///
/// A builder because both tests send the identical body and the replay case
/// sends it TWICE — three literals of eleven fields is three chances for the
/// cumulative counts to disagree and for the dedup assertion to be measuring a
/// different report rather than the same one.
pub(crate) fn report_body(lease_id: &str, event_id: &str, fence: u64) -> Value {
    json!({
        "lease_id": lease_id,
        "event_id": event_id,
        "fencing_token": fence,
        "outcome": "processed",
        "failure_reason": null,
        "failure_detail": "",
        "response_text": "the fixture run produced this",
        "tokens": u64::from(INPUT_TOKENS + CACHED_INPUT_TOKENS + OUTPUT_TOKENS),
        "input_tokens": INPUT_TOKENS,
        "cached_input_tokens": CACHED_INPUT_TOKENS,
        "output_tokens": OUTPUT_TOKENS,
        "telemetry": {"time_to_first_token_ms": 42, "wall_ms": 1_500},
        "checkpoint": {"last_event_id": event_id, "last_response": "the fixture run produced this"},
    })
}

/// A `GET` carrying the scenario's runner credential.
pub(crate) async fn get(http: &reqwest::Client, run: &Scenario, path: &str) -> reqwest::Response {
    http.get(format!("{}{path}", run.base))
        .bearer_auth(&run.token)
        .send()
        .await
        .expect("the booted daemon answers")
}

/// A `POST` carrying the scenario's runner credential and a JSON body.
///
/// Serialized here rather than through `reqwest`'s `json` helper: this
/// workspace takes `reqwest` with `default-features = false` and only the two
/// features the mint path needs, and turning `json` on for the whole graph to
/// save one line in a test is a dependency-surface change made for a test's
/// convenience. `serde_json` is already here.
pub(crate) async fn post(
    http: &reqwest::Client,
    run: &Scenario,
    path: &str,
    body: &Value,
) -> reqwest::Response {
    http.post(format!("{}{path}", run.base))
        .bearer_auth(&run.token)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(serde_json::to_vec(body).expect("the fixture body serializes"))
        .send()
        .await
        .expect("the booted daemon answers")
}

/// One field of a JSON value, by name.
///
/// `Value`'s own `Index` answers `Null` for a missing key, so an assertion
/// written as `body["id"] == …` reports "expected X, got Null" and leaves the
/// reader to work out whether the field was renamed, moved, or never sent.
/// This says WHICH key was absent, which is the whole difference when a wire
/// shape drifts.
pub(crate) fn field<'a>(value: &'a Value, key: &str) -> &'a Value {
    value
        .get(key)
        .unwrap_or_else(|| panic!("the answer carries no `{key}` field: {value}"))
}

/// One response's body, as JSON.
pub(crate) async fn json(response: reqwest::Response) -> Value {
    let bytes = response.bytes().await.expect("the body is readable");
    serde_json::from_slice(&bytes).unwrap_or_else(|_malformed| {
        panic!(
            "the daemon answered a body that is not JSON: {}",
            String::from_utf8_lossy(&bytes)
        )
    })
}
