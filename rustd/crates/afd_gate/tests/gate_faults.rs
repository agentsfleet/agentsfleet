//! What the gate pass says when neither datastore answers.
//!
//! No datastore, deliberately: the pool points at a reserved port and the queue
//! at a loopback port nobody listens on, so every read and write fails at the
//! socket. One kill-rule check walks three failure arms in order — the lookup
//! that could not be read, the anomaly counter that could not be read, and the
//! pause that could not be written — and each says why in a `reason` an
//! operator can read without the registry code, which the line's own fields
//! already carry, or a captured backtrace.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "support/gate_fixture.rs"]
mod fixture;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_core::test_util::trace::Capture;
use afd_crypto::entropy::Entropy;
use afd_dragonfly::Dragonfly;
use afd_dragonfly::config::{DragonflyConfig, DragonflyRole};
use afd_gate::gate::{Check, Gates, Trigger, Verdict};

use self::fixture::config_gates;

/// A queue address nothing listens on.
const NOWHERE: &str = "redis://127.0.0.1:1";

/// A rule that stops the fleet on every chat, and a threshold to count it
/// against, so one check reaches the counter and then the pause.
const KILL_AND_COUNT: &str = r#"{"rules":[{"tool":"chat","action":"user:fixture","behavior":"auto_kill"}],"anomaly_rules":[{"pattern":"same_action","threshold_count":2,"threshold_window_s":60}]}"#;

/// The fleet and workspace the check names; nothing answers, so any will do.
const FLEET: &str = "0190aaaa-bbbb-7ccc-8ddd-eeeeeeeeeee1";

/// The lookup that could not tell whether the event already holds a gate.
const EVENT_LOOKUP_UNREADABLE: &str = "gate_ref_lookup_failed";

/// The anomaly counter that could not be read.
const EVENT_COUNTER_UNAVAILABLE: &str = "gate_anomaly_counter_unavailable";

/// The pause that could not be written.
const EVENT_PAUSE_FAILED: &str = "gate_pause_failed";

/// Gates over a pool and a queue that both refuse.
fn unreachable_gates() -> Gates {
    let queue = Dragonfly::unreachable(&DragonflyConfig::from_url(
        DragonflyRole::Default,
        NOWHERE.to_owned(),
    ))
    .expect("a lazy handle opens no socket and cannot fail");
    Gates::new(afd_db::test_util::unreachable_db(), queue, Entropy::new())
}

/// The `reason` an event carried, proven free of a code prefix and of a
/// backtrace, which begins on a line of its own.
fn told_reason(capture: &Capture, event: &str) -> String {
    let logged = capture.only(event);
    let reason = logged
        .field("reason")
        .expect("a failure line says why")
        .to_owned();
    assert!(!reason.is_empty(), "{event} says nothing");
    assert!(
        !reason.contains("[UZ-"),
        "{event} repeats its code: {reason}"
    );
    assert!(
        !reason.contains('\n'),
        "{event} carries a backtrace: {reason}"
    );
    reason
}

#[tokio::test]
async fn every_failed_gate_read_and_write_reason_carries_no_code_prefix() {
    let capture = Capture::install();
    let gates = unreachable_gates();
    let fleet = Uuid7::parse(FLEET).expect("a canonical identifier");
    let config = config_gates(KILL_AND_COUNT);

    let verdict = gates
        .check(
            Check {
                fleet_id: &fleet,
                workspace_id: &fleet,
                event_id: "event-unreachable",
                event_type: "chat",
                actor: "user:fixture",
                request_json: "{}",
                config: &config,
            },
            UnixMillis::from_millis(0),
        )
        .await;

    assert_eq!(
        verdict,
        Verdict::Killed(Trigger::Policy),
        "a failed pause does not change what the gate decided"
    );
    told_reason(&capture, EVENT_LOOKUP_UNREADABLE);
    told_reason(&capture, EVENT_COUNTER_UNAVAILABLE);
    told_reason(&capture, EVENT_PAUSE_FAILED);
}
