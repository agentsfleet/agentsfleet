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

use af_core::error_code;
use afd_admission::Budgets;
use afd_db::test_util::mint_id;
use afd_events::disposition::{Placed, SteerDisposition};
use afd_events::{ACTOR_MACHINE, Steer};
use tracing::Level;

use crate::integration_steer_retry::{REQUEST_JSON, admissions_for, append_with};
use crate::support::EventsLane;
use af_core::test_util::trace::Capture;

/// One operation id, used on two fleets and against a changed message.
pub(crate) const OPERATION: &str = "019feca5-bc9b-72e8-b71f-e2714f6b0a01";

/// A second operation id, for the new work a spent budget must still refuse.
const FRESH_OPERATION: &str = "019feca5-bc9b-72e8-b71f-e2714f6b0a02";

/// The same operation id's message, changed after the first send.
pub(crate) const CHANGED_JSON: &str = r#"{"message":"redeploy production"}"#;

/// The warn a refused reuse leaves, and the fields it must carry.
const CONFLICT_EVENT: &str = "steer_operation_conflict";
const FIELD_EVENT: &str = "event";
const FIELD_ERROR_CODE: &str = "error_code";
const FIELD_CAUSE: &str = "cause";

/// The cause a changed message under a reused id names: the payload differed.
pub(crate) const CAUSE_PAYLOAD: &str = "payload";

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
}

/// A ledger whose deployment ceiling is spent: nothing new is admitted.
///
/// A ceiling of zero rather than a small count, because the estimate samples
/// the whole lane's rows and a parallel suite could cross a count first.
fn spent(lane: &EventsLane) -> Steer {
    Steer::new(lane.admissions().with_budgets(Budgets {
        deployment_backlog: 0,
    }))
}

/// A repeat is answered despite a spent deployment ceiling; new work is still
/// refused.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_replay_bypasses_the_deployment_ceiling() {
    let lane = EventsLane::open().await;
    let admitted = append_with(&Steer::new(lane.admissions()), &lane, Some(OPERATION)).await;
    let steer = spent(&lane);
    // The ceiling is now spent. The retry is the same message, already
    // accepted, and must be answered as such: the first event, marked a replay.
    let retried = steer
        .append(
            &lane.fleet,
            &lane.workspace,
            ACTOR_MACHINE,
            REQUEST_JSON,
            Some(OPERATION),
        )
        .await
        .expect("a retry of an admitted message is answered despite the budget");
    assert_eq!(
        (retried.event_id.as_str(), retried.admitted_at),
        (admitted.as_str(), None),
        "a retry of an admitted message must not be refused by the budget it \
         already spent -- the sender would read it as never delivered"
    );

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
}

/// Under a spent ceiling, the same id with a changed message is still a
/// conflict: the capacity refusal the sender would retry must not stand in for
/// the 409 that tells it to stop.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_drift_under_spent_budget_is_a_conflict() {
    let lane = EventsLane::open().await;
    append_with(&Steer::new(lane.admissions()), &lane, Some(OPERATION)).await;
    let steer = spent(&lane);
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
}

/// A row the ledger will not take is the caller's error: neither a capacity
/// refusal to retry nor a repeat to answer.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_an_admission_the_ledger_refuses_is_an_error() {
    let lane = EventsLane::open().await;
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
}

/// The same id with a changed message is refused and admits nothing.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_payload_drift_is_refused() {
    let lane = EventsLane::open().await;
    let steer = Steer::new(lane.admissions());

    let admitted = append_with(&steer, &lane, Some(OPERATION)).await;
    let logs = Capture::install();
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
    // One conflict warn, carrying the code and the half that differed, and
    // neither the message nor the key.
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
        assert_eq!(
            conflict.fields.get(FIELD_CAUSE).map(String::as_str),
            Some(CAUSE_PAYLOAD)
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
    assert_eq!(
        admissions_for(&lane, OPERATION).await,
        1,
        "the refusal admits nothing: one row, the first message's"
    );

    assert_eq!(
        crate::integration_steer_retry::queued_ids(&lane).await,
        [admitted]
    );
}

/// A send whose twin committed while it waited on the fleet lock, with the
/// deployment ceiling spent, is still answered as the twin's replay: its insert
/// never ran to meet the first row, so `repeat_despite` reads the key after the
/// capacity refusal. The insert never ran, so the ledger reported no live run
/// and the answer waits its turn.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_twin_committed_behind_the_fleet_lock_is_answered_under_a_spent_ceiling() {
    use sqlx::Connection as _;
    let lane = EventsLane::open().await;
    let operation = "019feca5-bc9b-72e8-b71f-e2714f6b0d01";
    let mut connection = lane.connection().await;
    let holder: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *connection)
        .await
        .expect("holder backend");
    let mut hold = connection.begin().await.expect("open the holder");
    sqlx::query("SELECT 1 FROM fleet.fleets WHERE id = $1::uuid FOR UPDATE")
        .bind(&lane.fleet)
        .execute(&mut *hold)
        .await
        .expect("hold the fleet row");
    let lane = std::sync::Arc::new(lane);
    let waiting = tokio::spawn({
        let lane = std::sync::Arc::clone(&lane);
        let operation = operation.to_owned();
        async move {
            spent(&lane)
                .append(
                    &lane.fleet,
                    &lane.workspace,
                    ACTOR_MACHINE,
                    REQUEST_JSON,
                    Some(&operation),
                )
                .await
        }
    });
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let blocked: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pg_stat_activity WHERE $1 = ANY(pg_blocking_pids(pid))",
        )
        .bind(holder)
        .fetch_one(&mut *lane.connection().await)
        .await
        .expect("blocked sessions");
        if blocked > 0 {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the send never waited"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    // The twin lands through the holder's own transaction, then the lock goes.
    let key = [lane.fleet.as_str(), operation].join(crate::integration_steer_retry::KEY_SEPARATOR);
    let admissions = lane.admissions();
    let prepared = admissions
        .prepare(afd_admission::Admission {
            producer: afd_admission::Producer::Steer,
            key: afd_admission::Key::Repeated(&key),
            fleet: &lane.fleet,
            workspace: &lane.workspace,
            actor: ACTOR_MACHINE,
            event_type: af_wire::event::EventType::Chat,
            request_json: REQUEST_JSON,
            reply: afd_admission::Reply::None,
        })
        .await
        .expect("prepare the twin");
    let twin = admissions
        .stage(&mut hold, prepared)
        .await
        .expect("stage the twin");
    hold.commit().await.expect("the twin commits");
    drop(connection);
    let answered = waiting
        .await
        .expect("the waiting send's task")
        .expect("the waiting send is answered, not refused");
    assert_eq!(
        answered.event_id, twin.stored.id,
        "the twin's event answers the waiter"
    );
    assert_eq!(
        answered.placed,
        Placed {
            disposition: SteerDisposition::Queued,
            folded_into_event_id: None,
        }
    );
    assert_eq!(admissions_for(&lane, operation).await, 1);
}
