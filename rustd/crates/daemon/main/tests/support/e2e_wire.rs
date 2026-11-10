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

use af_core::id::Uuid7;
use serde_json::{Value, json};

use crate::e2e::Scenario;

const OFFER_LIMIT: usize = 8;

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
        "holds": [],
        "closing": false,
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

/// Read the pushed offer for this scenario's event through the production runner downlink.
pub(crate) async fn wait_for_seeded_offer(run: &Scenario) -> (String, u64) {
    wait_for_offer(run, &run.event_id).await
}

/// Wait for the requested event without issuing another lease request.
pub(crate) async fn wait_for_offer(run: &Scenario, event_id: &Uuid7) -> (String, u64) {
    let client = run.native_session().await;
    let reading = async {
        let mut elsewhere = Vec::new();
        loop {
            let frame = client.offer().await.expect("the native owner remains live");
            let message: af_wire::protocol::Down<'_> = frame.decode().expect("the offer decodes");
            let af_wire::protocol::Down::Offer {
                lease: af_wire::lease::TaggedLease::Fleet(lease),
            } = message
            else {
                panic!("this fleet fixture received an authoring offer")
            };
            if lease.event.event_id == event_id.as_str() {
                return (lease.lease_id.to_string(), lease.fencing_token);
            }
            elsewhere.push(lease.event.fleet_id.to_string());
            assert!(
                elsewhere.len() <= OFFER_LIMIT,
                "{}",
                never_offered(&elsewhere)
            );
        }
    };
    tokio::time::timeout(std::time::Duration::from_secs(10), reading)
        .await
        .expect("the seeded event is pushed")
}

/// Record capability and policy through the native heartbeat command.
pub(crate) async fn heartbeat(run: &Scenario, body: &Value) -> Value {
    let bytes = serde_json::to_vec(body).expect("the heartbeat fixture encodes");
    let request = serde_json::from_slice(&bytes).expect("the heartbeat fixture is current");
    run.native_session()
        .await
        .heartbeat(&request)
        .await
        .expect("the native heartbeat is acknowledged")
}

/// Preserve the native report response or the daemon's coded refusal.
pub(crate) async fn report(run: &Scenario, body: &Value) -> afr_supervisor::Result<Value> {
    let bytes = serde_json::to_vec(body).expect("the report fixture encodes");
    let response = run.native_session().await.report(&bytes).await?;
    Ok(serde_json::to_value(response).expect("the report response encodes"))
}

/// How many unrelated fleet identifiers a missing-offer diagnostic names.
const NAMED_ELSEWHERE: usize = 8;

/// Why [`wait_for_offer`] came back empty-handed, from what it was handed.
fn never_offered(elsewhere: &[String]) -> String {
    if elsewhere.is_empty() {
        return "No offer arrived. Enable AFD_TEST_LOG=1 to inspect the fleet's gate and placement decisions.".to_owned();
    }
    let named = elsewhere
        .iter()
        .take(NAMED_ELSEWHERE)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "{} offers named other events on fleets [{named}]; check scenario isolation and retirement",
        elsewhere.len()
    )
}

/// Observe a durable refusal while asserting the runner receives no offer for this fleet.
pub(crate) async fn wait_without_offer<F, Fut>(run: &Scenario, mut ended: F) -> bool
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    let client = run.native_session().await;
    let waiting = async {
        loop {
            if ended().await {
                return true;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    };
    tokio::select! {
        biased;
        frame = client.offer() => {
            let frame = frame.expect("the native owner remains live");
            let message: af_wire::protocol::Down<'_> = frame.decode().expect("the offer decodes");
            if let af_wire::protocol::Down::Offer { lease: af_wire::lease::TaggedLease::Fleet(lease) } = message {
                assert_ne!(lease.event.fleet_id, run.fleet.as_str(), "a refused fleet must not be issued");
            }
            false
        },
        result = tokio::time::timeout(std::time::Duration::from_secs(2), waiting) => result.unwrap_or(false),
    }
}

/// The report one completed run sends.
///
/// A builder because both tests send the identical body and the replay case
/// sends it TWICE — three literals of eleven fields is three chances for the
/// cumulative counts to disagree and for the dedup assertion to be measuring a
/// different report rather than the same one.
pub(crate) fn report_body(lease_id: &str, event_id: &Uuid7, fence: u64) -> Value {
    json!({
        "lease_id": lease_id,
        "event_id": event_id,
        "fencing_token": fence,
        "outcome": "processed",
        "failure_reason": null,
        "failure_detail": "",
        "response_text": "the fixture run produced this",
        "tokens": u64::from(INPUT_TOKENS + CACHED_INPUT_TOKENS + OUTPUT_TOKENS),
        "requests": [{
            "seq": 1, "at": af_core::clock::now().to_string(),
            "input_tokens": INPUT_TOKENS, "cached_input_tokens": CACHED_INPUT_TOKENS,
            "cache_write_tokens": 0, "cache_write_5m_tokens": 0, "cache_write_1h_tokens": 0,
            "output_tokens": OUTPUT_TOKENS,
        }],
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

/// Read an expected native response path; missing fields must fail the fixture.
pub(crate) fn pointer<'a>(value: &'a Value, path: &str) -> &'a Value {
    value
        .pointer(path)
        .unwrap_or_else(|| panic!("native response lacks {path}"))
}
