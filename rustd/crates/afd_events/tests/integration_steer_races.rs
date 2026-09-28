//! Two sends with one operation id at once, and ids that name someone else's row.
//!
//! Split from `integration_steer_replay.rs` at the length cap. The read-back
//! that answers a repeat is not atomic with the insert, so two sends can both
//! reach the insert; these prove the ledger's unique key, and the read-back the
//! steer layer runs when the insert meets a row, still give one admission and
//! one honest answer each. The last two prove a key never answers with an
//! event that belongs to another caller or another fleet.
//!
//! Marked `#[ignore]` so `make test-unit-all` still compiles and lints this
//! without a datastore; `make test-integration-rustd` runs it.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_admission::{Admission, Key, Producer, Reply};
use afd_dragonfly::streams::FleetStreams;
use afd_events::{ACTOR_MACHINE, Steer};
use afd_wire::event::EventType;

use crate::integration_steer_replay::{CHANGED_JSON, OPERATION};
use crate::integration_steer_retry::{REQUEST_JSON, admissions_for, append_with, clean};
use crate::support::EventsLane;

/// Rounds of two concurrent sends per race test: enough that the two reach
/// the insert together on some of them, whichever way the scheduler leans.
const RACE_ROUNDS: usize = 16;

/// Another person in the same workspace, reusing an id someone else sent.
const OTHER_ACTOR: &str = "steer:user_other";

/// Joins a fleet to an operation id the way the steer layer keys it.
const KEY_SEPARATOR: &str = ":";

/// Two sends with one id and one payload racing to the insert admit once, and
/// both are answered with that one event.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_racing_repeats_admit_once() {
    let lane = EventsLane::open().await;
    let streams = FleetStreams::new(lane.queue.clone());
    let steer = Steer::new(lane.admissions());
    for round in 0..RACE_ROUNDS {
        let operation = format!("{OPERATION}-same-{round}");
        let (first, second) = tokio::join!(
            append_with(&steer, &lane, Some(&operation)),
            append_with(&steer, &lane, Some(&operation)),
        );
        assert_eq!(first, second, "round {round}: one id, one event");
        assert_eq!(admissions_for(&lane, &operation).await, 1, "round {round}");
    }
    clean(&lane, &streams).await;
}

/// Two sends with one id and different messages racing to the insert: one is
/// admitted and the other refused, never both answered with one event.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_racing_drift_is_refused() {
    let lane = EventsLane::open().await;
    let streams = FleetStreams::new(lane.queue.clone());
    let steer = Steer::new(lane.admissions());
    for round in 0..RACE_ROUNDS {
        let operation = format!("{OPERATION}-drift-{round}");
        let (first, second) = tokio::join!(
            steer.append(
                &lane.fleet,
                &lane.workspace,
                ACTOR_MACHINE,
                REQUEST_JSON,
                Some(&operation)
            ),
            steer.append(
                &lane.fleet,
                &lane.workspace,
                ACTOR_MACHINE,
                CHANGED_JSON,
                Some(&operation)
            ),
        );
        let outcomes = [first, second];
        let admitted = outcomes.iter().filter(|outcome| outcome.is_ok()).count();
        let refused = outcomes
            .iter()
            .filter(|outcome| {
                outcome
                    .as_ref()
                    .is_err_and(afd_events::Error::is_operation_conflict)
            })
            .count();
        assert_eq!((admitted, refused), (1, 1), "round {round}: {outcomes:?}");
        assert_eq!(admissions_for(&lane, &operation).await, 1, "round {round}");
    }
    clean(&lane, &streams).await;
}

/// Another person reusing an id someone in the workspace already sent, with the
/// very same message, is refused: the answer would be somebody else's event.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_reused_id_by_another_caller_is_refused() {
    let lane = EventsLane::open().await;
    let streams = FleetStreams::new(lane.queue.clone());
    let steer = Steer::new(lane.admissions());
    append_with(&steer, &lane, Some(OPERATION)).await;
    let refused = steer
        .append(
            &lane.fleet,
            &lane.workspace,
            OTHER_ACTOR,
            REQUEST_JSON,
            Some(OPERATION),
        )
        .await
        .expect_err("another caller's reuse is refused");
    assert!(refused.is_operation_conflict(), "{refused}");
    assert!(
        steer
            .replayed(
                &lane.fleet,
                &lane.workspace,
                OTHER_ACTOR,
                REQUEST_JSON,
                OPERATION
            )
            .await
            .expect_err("the lookup refuses it too")
            .is_operation_conflict()
    );
    assert_eq!(admissions_for(&lane, OPERATION).await, 1);
    clean(&lane, &streams).await;
}

/// A row some other fleet holds under the key a steer composes is refused, not
/// answered: the key string alone never decides whose event comes back.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_key_another_fleet_holds_is_refused() {
    let holder = EventsLane::open().await;
    let steered = EventsLane::open().await;
    // A row admitted for one fleet whose key spells the other fleet's.
    let foreign_key = [steered.fleet.as_str(), OPERATION].join(KEY_SEPARATOR);
    holder
        .admissions()
        .admit(Admission {
            producer: Producer::Steer,
            key: Key::Repeated(&foreign_key),
            fleet: &holder.fleet,
            workspace: &steered.workspace,
            actor: ACTOR_MACHINE,
            event_type: EventType::Chat,
            request_json: REQUEST_JSON,
            reply: Reply::None,
        })
        .await
        .expect("the holder's row is admitted");
    let refused = Steer::new(steered.admissions())
        .append(
            &steered.fleet,
            &steered.workspace,
            ACTOR_MACHINE,
            REQUEST_JSON,
            Some(OPERATION),
        )
        .await
        .expect_err("a row another fleet holds is refused");
    assert!(refused.is_operation_conflict(), "{refused}");
    clean(&holder, &FleetStreams::new(holder.queue.clone())).await;
    clean(&steered, &FleetStreams::new(steered.queue.clone())).await;
}
