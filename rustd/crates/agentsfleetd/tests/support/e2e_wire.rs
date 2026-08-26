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

use serde_json::{Value, json};

use crate::e2e::Scenario;

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
