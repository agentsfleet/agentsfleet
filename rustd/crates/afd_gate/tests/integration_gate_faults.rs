//! The gate failures that need one datastore answering and the other gone.
//!
//! A recorded gate whose decision cannot be read needs a live queue to hold
//! the reference and a dead pool behind the mirror; a paused fleet whose
//! readiness mark cannot be cleared needs a live pool to take the pause and a
//! dead queue to refuse the clear. Each line says why in a `reason` free of
//! the registry code and of a captured backtrace.
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
use afd_gate::gate::{
    Check, Claim, GateRef, Gates, Park, Parked, Stated, Trigger, Verdict, Waiting,
};

use self::fixture::{Fixture, NOW, config, config_gates, connect_redis};

/// A queue address nothing listens on.
const NOWHERE: &str = "redis://127.0.0.1:1";

/// The timeout the hand-recorded reference is kept alive for.
const REFERENCE_TIMEOUT: std::time::Duration = std::time::Duration::from_hours(1);

/// A rule that stops the fleet on every chat.
const KILL_EVERY_CHAT: &str =
    r#"{"rules":[{"tool":"chat","action":"user:fixture","behavior":"auto_kill"}]}"#;

/// A recorded gate's decision that could not be read.
const EVENT_DECISION_UNREADABLE: &str = "gate_decision_read_failed";

/// A paused fleet that kept its readiness mark.
const EVENT_READY_STALE: &str = "gate_ready_clear_failed";

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
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn an_unreadable_gate_decision_reason_carries_no_code_prefix() {
    let gates = Gates::new(
        afd_db::test_util::unreachable_db(),
        connect_redis().await,
        Entropy::new(),
    );
    let fleet = Uuid7::parse(&afd_db::test_util::mint_id()).expect("a canonical identifier");
    let action = Uuid7::parse(&afd_db::test_util::mint_id()).expect("a canonical identifier");
    let event_id = "event-decision-unreadable";
    gates
        .record(&fleet, event_id, &GateRef::new(action), REFERENCE_TIMEOUT)
        .await
        .expect("the live queue holds the reference");
    let config = config(false);

    let capture = Capture::install();
    let verdict = gates
        .check(
            Check {
                fleet_id: &fleet,
                workspace_id: &fleet,
                event_id,
                event_type: "chat",
                actor: "user:fixture",
                request_json: "{}",
                config: &config,
            },
            NOW,
        )
        .await;

    assert_eq!(
        verdict,
        Verdict::Await(Waiting::Unreadable),
        "a decision nobody could read waits rather than denying an approval"
    );
    told_reason(&capture, EVENT_DECISION_UNREADABLE);
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_stale_ready_mark_reason_carries_no_code_prefix() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let dead_queue = Dragonfly::unreachable(&DragonflyConfig::from_url(
        DragonflyRole::Default,
        NOWHERE.to_owned(),
    ))
    .expect("a lazy handle opens no socket and cannot fail");
    let gates = Gates::new(fixture.database.clone(), dead_queue, Entropy::new());
    let policy_kill = config_gates(KILL_EVERY_CHAT);

    let capture = Capture::install();
    let verdict = gates
        .check(fixture.check("event-ready-stale", &policy_kill), NOW)
        .await;

    assert_eq!(verdict, Verdict::Killed(Trigger::Policy));
    told_reason(&capture, EVENT_READY_STALE);
    drop(capture);
    fixture.cleanup().await;
}

/// The mirror's spelling of an approval, as the resolver writes it.
const MIRRORED_APPROVE: &str = "approve";

/// How long the hand-written mirror is kept, in seconds.
const MIRROR_TTL_SECONDS: i64 = 3_600;

/// A gate that could not be raised.
const EVENT_PARK_UNAVAILABLE: &str = "gate_park_unavailable";

/// Gates over `database` and a queue that refuses every command.
fn gates_without_a_queue(database: afd_db::Db) -> Gates {
    let dead_queue = Dragonfly::unreachable(&DragonflyConfig::from_url(
        DragonflyRole::Default,
        NOWHERE.to_owned(),
    ))
    .expect("a lazy handle opens no socket and cannot fail");
    Gates::new(database, dead_queue, Entropy::new())
}

#[tokio::test]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn a_mirrored_answer_decides_without_reading_the_durable_row() {
    // The pool refuses: an answer that arrives proves the mirror alone gave it.
    let queue = connect_redis().await;
    let gates = Gates::new(
        afd_db::test_util::unreachable_db(),
        queue.clone(),
        Entropy::new(),
    );
    let fleet = Uuid7::parse(&afd_db::test_util::mint_id()).expect("a canonical identifier");
    let action = Uuid7::parse(&afd_db::test_util::mint_id()).expect("a canonical identifier");
    let event_id = "event-mirror-answers";
    gates
        .record(
            &fleet,
            event_id,
            &GateRef::new(action.clone()),
            REFERENCE_TIMEOUT,
        )
        .await
        .expect("the live queue holds the reference");
    queue
        .set_for(
            &afd_gate::gate::key::response(action.as_str()),
            MIRRORED_APPROVE,
            MIRROR_TTL_SECONDS,
        )
        .await
        .expect("the live queue holds the mirrored answer");
    let config = config(false);

    let verdict = gates
        .check(
            Check {
                fleet_id: &fleet,
                workspace_id: &fleet,
                event_id,
                event_type: "chat",
                actor: "user:fixture",
                request_json: "{}",
                config: &config,
            },
            NOW,
        )
        .await;

    assert_eq!(
        verdict,
        Verdict::Pass,
        "the human's mirrored yes releases the event"
    );
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_reference_whose_row_is_gone_keeps_the_event_waiting() {
    let fixture = Fixture::create().await;
    let gates = Gates::new(
        fixture.database.clone(),
        connect_redis().await,
        Entropy::new(),
    );
    let action = Uuid7::parse(&afd_db::test_util::mint_id()).expect("a canonical identifier");
    let event_id = "event-row-gone";
    gates
        .record(
            &fixture.fleet,
            event_id,
            &GateRef::new(action),
            REFERENCE_TIMEOUT,
        )
        .await
        .expect("the live queue holds the reference");
    let config = config(false);

    let verdict = gates.check(fixture.check(event_id, &config), NOW).await;

    assert_eq!(
        verdict,
        Verdict::Await(Waiting::Pending),
        "no row means no deadline to lapse; the key's own lifetime ends the wait"
    );
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_park_whose_reference_cannot_be_written_fails_closed_after_the_row() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let gates = gates_without_a_queue(fixture.database.clone());
    let event_id = "event-reference-unwritable";
    let config = config(false);
    let scope = fixture.check(event_id, &config);
    let capture = Capture::install();

    let parked = gates
        .park(
            Park {
                fleet_id: scope.fleet_id,
                workspace_id: scope.workspace_id,
                event_id,
                stated: Stated::of("chat", "user:fixture", event_id, None, REFERENCE_TIMEOUT),
                claim: &Claim::of(None),
            },
            NOW,
        )
        .await;

    assert_eq!(
        parked,
        Parked::Unavailable,
        "nothing points at the row, so nobody is asked"
    );
    let logged = capture.only(EVENT_PARK_UNAVAILABLE);
    assert_eq!(logged.field("write"), Some("event_reference"));
    drop(capture);
    assert_eq!(
        fixture.card_count().await,
        1,
        "the row went down first; the next poll finds no reference and parks again"
    );
    fixture.cleanup().await;
}
