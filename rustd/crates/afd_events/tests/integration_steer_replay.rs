//! A steer's operation id is the caller's name for one message on one fleet.
//!
//! `integration_steer_retry` proves the retry itself admits once. This proves
//! the three edges around it that a caller meets when things go wrong:
//!
//! - the id is scoped to the fleet, so one client reusing ids across fleets
//!   never gets another fleet's event back;
//! - a repeat is answered despite the fleet budget, so a message already
//!   admitted is not refused as undelivered when the fleet fills up behind it;
//! - the same id with a different message is refused, and nothing is admitted,
//!   rather than answered with the first message's event.
//!
//! Marked `#[ignore]` so `make test-unit-all` still compiles and lints this
//! without a datastore; `make test-integration-rustd` runs it.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_admission::Budgets;
use afd_core::error_code;
use afd_db::test_util::mint_id;
use afd_dragonfly::streams::FleetStreams;
use afd_events::{ACTOR_MACHINE, Steer};
use tracing::Level;

use crate::integration_steer_retry::{REQUEST_JSON, admissions_for, append_with, clean};
use crate::recorder::Recorder;
use crate::support::EventsLane;

/// The consumer name the test reads the stream back under; distinct from the
/// sibling suites' so no read consumes another suite's entry.
const CONSUMER: &str = "steer-replay-integration-reader";

/// One operation id, used on two fleets and against a changed message.
pub(crate) const OPERATION: &str = "019feca5-bc9b-72e8-b71f-e2714f6b0a01";

/// A second operation id, for the new work a spent budget must still refuse.
const FRESH_OPERATION: &str = "019feca5-bc9b-72e8-b71f-e2714f6b0a02";

/// The same operation id's message, changed after the first send.
pub(crate) const CHANGED_JSON: &str = r#"{"message":"redeploy production"}"#;

/// A fleet budget one entry deep: the first admission spends it.
const ONE_ENTRY: u64 = 1;

/// The warn a refused reuse leaves, and the fields it must carry.
const CONFLICT_EVENT: &str = "steer_operation_conflict";
const FIELD_EVENT: &str = "event";
const FIELD_ERROR_CODE: &str = "error_code";

/// The same id on two fleets is two operations, each answering its own fleet.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_operation_ids_are_scoped_to_the_fleet() {
    let first = EventsLane::open().await;
    let second = EventsLane::open().await;

    let on_first = append_with(&Steer::new(first.admissions()), &first, Some(OPERATION)).await;
    let on_second = append_with(&Steer::new(second.admissions()), &second, Some(OPERATION)).await;

    assert_ne!(
        on_first, on_second,
        "one id on two fleets must not answer with the other fleet's event -- \
         the unique key is global, so the fleet has to be inside it"
    );
    assert_eq!(admissions_for(&first, OPERATION).await, 1);
    assert_eq!(admissions_for(&second, OPERATION).await, 1);

    clean(&first, &FleetStreams::new(first.queue.clone())).await;
    clean(&second, &FleetStreams::new(second.queue.clone())).await;
}

/// A repeat is answered despite a spent fleet budget; new work is still refused.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_replay_bypasses_fleet_budget() {
    let lane = EventsLane::open().await;
    let streams = FleetStreams::new(lane.queue.clone());
    streams
        .ensure_group(&lane.fleet)
        .await
        .expect("the consumer group is created");
    let steer = Steer::new(lane.admissions().with_budgets(Budgets {
        fleet_backlog: ONE_ENTRY,
        replay_backlog: u64::MAX,
    }));

    let admitted = append_with(&steer, &lane, Some(OPERATION)).await;
    // The budget is now spent. The retry is the same message, already
    // accepted, and must be answered as such.
    let retried = append_with(&steer, &lane, Some(OPERATION)).await;
    assert_eq!(
        admitted, retried,
        "a retry of an admitted message must not be refused by the budget it \
         already spent -- the sender would read it as never delivered"
    );
    let replayed = steer
        .replayed(
            &lane.fleet,
            &lane.workspace,
            ACTOR_MACHINE,
            REQUEST_JSON,
            OPERATION,
        )
        .await
        .expect("the ledger answers");
    assert_eq!(replayed.as_deref(), Some(admitted.as_str()));

    // New work under a spent budget is still refused, with the capacity class.
    let refused = steer
        .append(
            &lane.fleet,
            &lane.workspace,
            ACTOR_MACHINE,
            REQUEST_JSON,
            Some(FRESH_OPERATION),
        )
        .await
        .expect_err("the fleet holds its budget");
    assert!(
        refused.is_datastore_unavailable(),
        "a spent budget invites the retry an outage does: {refused}"
    );
    assert_eq!(admissions_for(&lane, FRESH_OPERATION).await, 0);
    // A steer without an id names no admission to repeat, so the capacity
    // refusal stands.
    let keyless = steer
        .append(
            &lane.fleet,
            &lane.workspace,
            ACTOR_MACHINE,
            REQUEST_JSON,
            None,
        )
        .await
        .expect_err("the fleet holds its budget");
    assert!(
        keyless.is_datastore_unavailable(),
        "a keyless steer over budget is refused like new work: {keyless}"
    );
    assert!(
        steer
            .replayed(
                &lane.fleet,
                &lane.workspace,
                ACTOR_MACHINE,
                REQUEST_JSON,
                FRESH_OPERATION
            )
            .await
            .expect("the ledger answers")
            .is_none(),
        "an id never admitted replays nothing"
    );

    clean(&lane, &streams).await;
}

/// Under a spent budget, the same id with a changed message is still a
/// conflict: the capacity refusal the sender would retry must not stand in for
/// the 409 that tells it to stop.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_drift_under_spent_budget_is_a_conflict() {
    let lane = EventsLane::open().await;
    let streams = FleetStreams::new(lane.queue.clone());
    streams
        .ensure_group(&lane.fleet)
        .await
        .expect("the consumer group is created");
    let steer = Steer::new(lane.admissions().with_budgets(Budgets {
        fleet_backlog: ONE_ENTRY,
        replay_backlog: u64::MAX,
    }));

    append_with(&steer, &lane, Some(OPERATION)).await;
    let refused = steer
        .append(
            &lane.fleet,
            &lane.workspace,
            ACTOR_MACHINE,
            CHANGED_JSON,
            Some(OPERATION),
        )
        .await
        .expect_err("a changed message under a reused id is refused");
    assert!(refused.is_operation_conflict(), "{refused}");
    assert_eq!(admissions_for(&lane, OPERATION).await, 1);

    clean(&lane, &streams).await;
}

/// A row the ledger will not take is the caller's error: neither a capacity
/// refusal to retry nor a repeat to answer.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_an_admission_the_ledger_refuses_is_an_error() {
    let lane = EventsLane::open().await;
    let streams = FleetStreams::new(lane.queue.clone());
    let steer = Steer::new(lane.admissions());

    // No fleet row holds this id, so the admission's foreign key refuses it.
    let unheld = mint_id();
    let refused = steer
        .append(&unheld, &lane.workspace, ACTOR_MACHINE, REQUEST_JSON, None)
        .await
        .expect_err("an admission for a fleet the ledger does not hold is refused");
    assert!(!refused.is_operation_conflict(), "{refused}");
    assert!(
        !refused.is_datastore_unavailable(),
        "not a retryable capacity refusal: {refused}"
    );

    clean(&lane, &streams).await;
}

/// The same id with a changed message is refused and admits nothing.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_payload_drift_is_refused() {
    let lane = EventsLane::open().await;
    let streams = FleetStreams::new(lane.queue.clone());
    streams
        .ensure_group(&lane.fleet)
        .await
        .expect("the consumer group is created");
    let steer = Steer::new(lane.admissions());

    let admitted = append_with(&steer, &lane, Some(OPERATION)).await;
    let logs = Recorder::install();
    let refused = steer
        .append(
            &lane.fleet,
            &lane.workspace,
            ACTOR_MACHINE,
            CHANGED_JSON,
            Some(OPERATION),
        )
        .await
        .expect_err("a reused id with another message is refused");
    // One conflict warn, carrying the code, and neither the message nor the
    // key.
    let conflicts: Vec<_> = logs
        .events()
        .into_iter()
        .filter(|record| record.fields.get(FIELD_EVENT).map(String::as_str) == Some(CONFLICT_EVENT))
        .collect();
    assert_eq!(conflicts.len(), 1, "one conflict warn: {conflicts:?}");
    for conflict in &conflicts {
        assert_eq!(conflict.level, Level::WARN);
        assert_eq!(
            conflict.fields.get(FIELD_ERROR_CODE).map(String::as_str),
            Some(error_code::AGENTSFLEET_OPERATION_CONFLICT.as_str())
        );
        for value in conflict.fields.values() {
            assert!(
                !value.contains("redeploy"),
                "the warn leaks the message: {value}"
            );
            assert!(
                !value.contains(OPERATION),
                "the warn leaks the key: {value}"
            );
        }
    }
    drop(logs);
    assert!(
        refused.is_operation_conflict(),
        "not the conflict: {refused}"
    );
    assert_eq!(refused.code(), error_code::AGENTSFLEET_OPERATION_CONFLICT);
    assert!(
        steer
            .replayed(
                &lane.fleet,
                &lane.workspace,
                ACTOR_MACHINE,
                CHANGED_JSON,
                OPERATION
            )
            .await
            .expect_err("the lookup refuses the same way")
            .is_operation_conflict()
    );
    assert_eq!(
        admissions_for(&lane, OPERATION).await,
        1,
        "the refusal admits nothing: one row, the first message's"
    );

    let leased = streams
        .read_new(&lane.fleet, CONSUMER)
        .await
        .expect("the read reaches the queue")
        .expect("the first message left an entry");
    assert_eq!(
        leased.field(afd_wire::event::field::EVENT_ID),
        Some(admitted.as_str())
    );
    assert!(
        streams
            .read_new(&lane.fleet, CONSUMER)
            .await
            .expect("the read reaches the queue")
            .is_none(),
        "the changed message appended no second entry"
    );

    clean(&lane, &streams).await;
}
