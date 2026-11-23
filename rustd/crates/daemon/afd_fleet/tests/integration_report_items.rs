//! A turn keeps only the items of the run that settles it: a run that died
//! leaves none of its items behind once a resumed run reports.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_wire::tool_detail::ToolCallRecordsRequest;
use afd_fleet::run::{Billed, Committed};

use crate::report_commit::{RESPONSE_ACCEPTED, report};
use crate::report_seed::{Held, held};
use crate::seed::{MODEL, POSTURE, PROVIDER, select_fleet};

/// Finished call `number` of `turn`, as a runner posts it.
fn call(number: u64, turn: &str) -> String {
    format!(
        r#"{{"call_number":{number},"arguments":{{}},"truncated_arguments":false,"output":"ok","output_line_count":1,"truncated":false,"turn_id":"{turn}","kind":"tool_call","tool":"shell","status":"succeeded"}}"#
    )
}

/// A post of one finished call of `turn` per number in `calls`, under
/// `fence`.
fn post(fence: u64, turn: &str, calls: &[u64]) -> String {
    let calls: Vec<String> = calls.iter().map(|number| call(*number, turn)).collect();
    format!(
        r#"{{"fencing_token":{fence},"calls":[{}]}}"#,
        calls.join(",")
    )
}

/// The held turn's items, as `(fence, sequence)`, in order.
async fn items(held: &Held) -> Vec<(i64, i64)> {
    sqlx::query_as(
        "SELECT fencing_token, sequence FROM thread.items WHERE turn_id = $1 \
         ORDER BY fencing_token, sequence",
    )
    .bind(&held.turn_id)
    .fetch_all(
        &mut *held
            .fixtures
            .database
            .acquire()
            .await
            .expect("a connection"),
    )
    .await
    .expect("the turn's items")
}

/// Run 1 records two items and dies; run 2 resumes the turn, records one and
/// settles it. Only run 2's item remains.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_settling_resumed_run_drops_the_dead_runs_items() {
    let held = held().await;
    let plane = held.fixtures.plane();
    let dead = held.fence.as_u64();
    let first = post(dead, held.turn_id.as_str(), &[1, 2]);
    let first: ToolCallRecordsRequest<'_> =
        serde_json::from_str(&first).expect("a well-formed post");
    let kept = plane
        .record_tool_calls(&held.runner, held.issued.run_id.as_str(), &first, held.now)
        .await
        .expect("run 1 records its items");
    assert_eq!(kept.stored_count, 2);
    let dead = i64::try_from(dead).expect("a fence fits");
    assert_eq!(items(&held).await, vec![(dead, 1), (dead, 2)]);

    held.fixtures
        .lapse_holder(&held.issued.run_id, &held.fleet)
        .await;
    let resumed = select_fleet(&held.runs, &held.spare, &held.fleet)
        .await
        .expect("the spare is assigned the dead run's turn");
    assert_eq!(resumed.turn_id, held.turn_id);
    let second = held
        .runs
        .issue(
            &held.spare,
            &resumed,
            Billed {
                tenant_id: &held.tenant,
                posture: POSTURE,
                provider: PROVIDER,
                model: MODEL,
            },
            held.now,
        )
        .await
        .expect("the run row must be written")
        .expect("the assignment is still held")
        .run_id;
    let live = resumed.fence.as_u64();
    let last = post(live, held.turn_id.as_str(), &[1]);
    let last: ToolCallRecordsRequest<'_> = serde_json::from_str(&last).expect("a well-formed post");
    let kept = plane
        .record_tool_calls(&held.spare, second.as_str(), &last, held.now)
        .await
        .expect("run 2 records its item");
    assert_eq!(kept.stored_count, 1);

    let run = held
        .runs
        .load_for_report(&second, &held.spare)
        .await
        .expect("the run load must reach the datastore")
        .expect("run 2 belongs to the spare");
    let pricing = held.fixtures.pricing(&second).await;
    let now = held.fixtures.metered_since(&held.fleet).await;
    let committed = held
        .runs
        .commit_report(report(
            &second,
            &held.spare,
            &run,
            (&second, &pricing),
            RESPONSE_ACCEPTED,
            now,
        ))
        .await
        .expect("run 2 reports");
    assert!(
        matches!(committed, Committed::Settled { .. }),
        "{committed:?}"
    );
    let live = i64::try_from(live).expect("a fence fits");
    assert_eq!(items(&held).await, vec![(live, 1)], "one item remains");
    held.fixtures.cleanup().await;
}

/// One item row, as the record verb stored it.
#[derive(Debug, PartialEq, Eq, sqlx::FromRow)]
struct Item {
    run_id: af_core::id::Uuid7,
    turn_id: af_core::id::Uuid7,
    sequence: i64,
    kind: String,
    tool: Option<String>,
    status: String,
    changes: Option<serde_json::Value>,
}

/// Every item the held fleet keeps, in order.
async fn stored(held: &Held) -> Vec<Item> {
    sqlx::query_as(
        "SELECT run_id, turn_id, sequence, kind, tool, status, changes FROM thread.items \
         WHERE fleet_id = $1 ORDER BY run_id, sequence",
    )
    .bind(&held.fleet)
    .fetch_all(
        &mut *held
            .fixtures
            .database
            .acquire()
            .await
            .expect("a connection"),
    )
    .await
    .expect("the fleet's items")
}

/// A run's `apply_patch` changes `src/a.rs` by +24 −8: the post carries the
/// counts the runner's file tool reported, and the daemon keeps exactly one
/// `edit` item holding them, under the run and its turn.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn an_edit_item_records_path_and_line_counts() {
    let held = held().await;
    let plane = held.fixtures.plane();
    let turn = held.turn_id.as_str();
    let body = format!(
        r#"{{"fencing_token":{},"calls":[{{"call_number":1,"arguments":{{"patch":"*** Begin Patch"}},"truncated_arguments":false,"output":"Success. Updated the following files:\nM src/a.rs\nLines changed:\n\"src/a.rs\" +24 -8","output_line_count":4,"truncated":false,"turn_id":"{turn}","kind":"edit","tool":"apply_patch","status":"succeeded","changes":[{{"path":"src/a.rs","added":24,"removed":8}}]}}]}}"#,
        held.fence.as_u64()
    );
    let request: ToolCallRecordsRequest<'_> =
        serde_json::from_str(&body).expect("a well-formed post");

    let kept = plane
        .record_tool_calls(
            &held.runner,
            held.issued.run_id.as_str(),
            &request,
            held.now,
        )
        .await
        .expect("the live run records its edit");

    assert_eq!((kept.stored_count, kept.skipped_count), (1, 0));
    assert_eq!(
        stored(&held).await,
        vec![Item {
            run_id: held.issued.run_id.clone(),
            turn_id: held.turn_id.clone(),
            sequence: 1,
            kind: af_wire::tool_detail::ItemKind::Edit.as_str().to_owned(),
            tool: Some("apply_patch".to_owned()),
            status: af_wire::tool_detail::ItemStatus::Succeeded
                .as_str()
                .to_owned(),
            changes: Some(serde_json::json!([{"path": "src/a.rs", "added": 24, "removed": 8}])),
        }],
        "exactly one edit item, with its path and both counts"
    );
    held.fixtures.cleanup().await;
}

/// A call that changed nothing is a `tool_call` item with no changes.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_call_that_changed_nothing_records_no_changes() {
    let held = held().await;
    let plane = held.fixtures.plane();
    let body = post(held.fence.as_u64(), held.turn_id.as_str(), &[1]);
    let request: ToolCallRecordsRequest<'_> =
        serde_json::from_str(&body).expect("a well-formed post");

    plane
        .record_tool_calls(
            &held.runner,
            held.issued.run_id.as_str(),
            &request,
            held.now,
        )
        .await
        .expect("the live run records its call");

    let items = stored(&held).await;
    assert_eq!(items.len(), 1);
    assert_eq!(
        items
            .first()
            .map(|item| (item.kind.as_str(), item.changes.clone())),
        Some((af_wire::tool_detail::ItemKind::ToolCall.as_str(), None))
    );
    held.fixtures.cleanup().await;
}

/// A record naming a turn the run does not serve is refused: skipped and
/// counted, nothing stored, while the post's other record is kept.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_record_naming_a_turn_the_run_does_not_serve_is_refused() {
    let held = held().await;
    let plane = held.fixtures.plane();
    let stranger = afd_db::test_util::mint_id();
    let body = format!(
        r#"{{"fencing_token":{},"calls":[{},{}]}}"#,
        held.fence.as_u64(),
        call(1, held.turn_id.as_str()),
        call(2, stranger.as_str())
    );
    let request: ToolCallRecordsRequest<'_> =
        serde_json::from_str(&body).expect("a well-formed post");

    let kept = plane
        .record_tool_calls(
            &held.runner,
            held.issued.run_id.as_str(),
            &request,
            held.now,
        )
        .await
        .expect("a post with one unserved record is not refused whole");

    assert_eq!((kept.stored_count, kept.skipped_count), (1, 1));
    let items = stored(&held).await;
    assert_eq!(
        items
            .iter()
            .map(|item| (item.sequence, item.turn_id.clone()))
            .collect::<Vec<_>>(),
        vec![(1, held.turn_id.clone())],
        "only the served turn's record is stored"
    );
    held.fixtures.cleanup().await;
}
