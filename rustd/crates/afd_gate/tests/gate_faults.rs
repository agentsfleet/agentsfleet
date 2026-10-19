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

use afd_core::id::Uuid7;
use afd_core::test_util::trace::Capture;
use afd_crypto::entropy::Entropy;
use afd_dragonfly::Dragonfly;
use afd_dragonfly::config::{DragonflyConfig, DragonflyRole};
use afd_gate::gate::{Check, Claim, Gates, Park, Parked, Stated, Trigger, Verdict, Waiting};

use self::fixture::config_gates;

/// A queue address nothing listens on.
const NOWHERE: &str = "redis://127.0.0.1:1";

/// A rule that stops the fleet on every chat, and a threshold to count it
/// against, so one check reaches the counter and then the pause.
const KILL_AND_COUNT: &str = r#"{"rules":[{"tool":"chat","action":"user:fixture","behavior":"auto_kill"}],"anomaly_rules":[{"pattern":"same_action","threshold_count":2,"threshold_window_s":60}]}"#;

/// How long a parked question would wait for its answer.
const PARK_TIMEOUT: std::time::Duration = std::time::Duration::from_hours(1);

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
    assert!(
        logged
            .field("error_code")
            .is_some_and(|code| !code.is_empty()),
        "{event} names no error_code"
    );
    logged.assert_uncoded_reason();
    logged.field("reason").unwrap_or_default().to_owned()
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
            jiff::Timestamp::UNIX_EPOCH,
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

/// A rule that asks a human about every chat.
const ASK_EVERY_CHAT: &str =
    r#"{"rules":[{"tool":"chat","action":"user:fixture","behavior":"approve"}]}"#;

/// A gate that could not be raised.
const EVENT_PARK_UNAVAILABLE: &str = "gate_park_unavailable";

#[tokio::test]
async fn a_gated_event_whose_lookup_failed_waits_instead_of_asking_again() {
    let capture = Capture::install();
    let gates = unreachable_gates();
    let fleet = Uuid7::parse(FLEET).expect("a canonical identifier");
    let config = config_gates(ASK_EVERY_CHAT);

    let verdict = gates
        .check(
            Check {
                fleet_id: &fleet,
                workspace_id: &fleet,
                event_id: "event-ask-unreadable",
                event_type: "chat",
                actor: "user:fixture",
                request_json: "{}",
                config: &config,
            },
            jiff::Timestamp::UNIX_EPOCH,
        )
        .await;

    assert_eq!(
        verdict,
        Verdict::Await(Waiting::Unreadable),
        "the event may already hold a card; a second one is worse than a poll"
    );
    told_reason(&capture, EVENT_LOOKUP_UNREADABLE);
    let events = capture.events();
    assert!(
        events
            .iter()
            .all(|event| event.field("event") != Some(EVENT_PARK_UNAVAILABLE)),
        "no park was attempted: {events:?}"
    );
}

#[tokio::test]
async fn a_park_whose_row_cannot_be_written_fails_closed_naming_the_row() {
    let capture = Capture::install();
    let gates = unreachable_gates();
    let fleet = Uuid7::parse(FLEET).expect("a canonical identifier");

    let parked = gates
        .park(
            Park {
                fleet_id: &fleet,
                workspace_id: &fleet,
                event_id: "event-row-unwritable",
                stated: Stated::of(
                    "chat",
                    "user:fixture",
                    "event-row-unwritable",
                    None,
                    PARK_TIMEOUT,
                ),
                claim: &Claim::of(None),
            },
            jiff::Timestamp::UNIX_EPOCH,
        )
        .await;

    assert_eq!(parked, Parked::Unavailable, "no row, so no card and no run");
    let logged = capture.only(EVENT_PARK_UNAVAILABLE);
    assert_eq!(logged.field("write"), Some("durable_row"));
    assert_eq!(logged.field("fleet_id"), Some(FLEET));
}
