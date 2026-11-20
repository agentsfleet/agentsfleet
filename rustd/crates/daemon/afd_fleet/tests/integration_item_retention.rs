//! An item lives as long as its turn: deleting its settled run leaves the
//! run's items, naming no run, and the item detail still reads them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_core::id::Uuid7;
use af_wire::tool_detail::{ItemKind, ToolCallRecordsRequest};
use afd_turns::view::{CallAddress, History};

use crate::report_seed::held;

/// The run's one edit, as a runner posts it under `fence` for `turn`.
fn edit_post(fence: u64, turn: &str) -> String {
    format!(
        r#"{{"fencing_token":{fence},"calls":[{{"call_number":1,"arguments":{{"patch":"*** Begin Patch"}},"truncated_arguments":false,"output":"Success.","output_line_count":1,"truncated":false,"turn_id":"{turn}","kind":"edit","tool":"apply_patch","status":"succeeded","changes":[{{"path":"src/a.rs","added":24,"removed":8}}]}}]}}"#
    )
}

// Value: protects=a turn's item history survives its run's deletion; fails_when=the
// items foreign key cascades again, or the detail read still requires the run row;
// why_new=items were erased with their run; seam=the runner row's deletion, which
// cascades to its runs, and the item detail read over live Postgres.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn deleting_a_settled_run_keeps_its_items_and_their_detail() {
    let held = held().await;
    let run = held.issued.run_id.clone();
    let body = edit_post(held.fence.as_u64(), held.turn_id.as_str());
    let request: ToolCallRecordsRequest<'_> =
        serde_json::from_str(&body).expect("a well-formed post");
    held.fixtures
        .plane()
        .record_tool_calls(&held.runner, run.as_str(), &request, held.now)
        .await
        .expect("the live run records its edit");

    let mut connection = held
        .fixtures
        .database
        .acquire()
        .await
        .expect("a connection");
    let settled = sqlx::query("UPDATE thread.runs SET status = $2 WHERE id = $1")
        .bind(&run)
        .bind(afd_state::sql::RUN_STATUS_REPORTED)
        .execute(&mut *connection)
        .await
        .expect("the run settles");
    assert_eq!(settled.rows_affected(), 1);
    // `thread.runs.runner_id` cascades: the runner's deletion takes its runs.
    held.fixtures.delete_runner(&held.runner).await;
    let runs: i64 = sqlx::query_scalar("SELECT count(*) FROM thread.runs WHERE id = $1")
        .bind(&run)
        .fetch_one(&mut *connection)
        .await
        .expect("the run count");
    assert_eq!(runs, 0, "the run went with its runner");

    let kept: Vec<(Option<Uuid7>, Uuid7, i64, String)> = sqlx::query_as(
        "SELECT run_id, turn_id, sequence, kind FROM thread.items WHERE fleet_id = $1",
    )
    .bind(&held.fleet)
    .fetch_all(&mut *connection)
    .await
    .expect("the fleet's items");
    drop(connection);
    assert_eq!(
        kept,
        vec![(
            None,
            held.turn_id.clone(),
            1,
            ItemKind::Edit.as_str().to_owned()
        )],
        "the item stays with its turn, naming no run"
    );

    let detail = History::new(held.fixtures.database.clone())
        .tool_call(
            &held.acquired.workspace_id,
            &held.fleet,
            &held.turn_id,
            CallAddress {
                fence: i64::try_from(held.fence.as_u64()).expect("a fence fits i64"),
                call_number: 1,
            },
        )
        .await
        .expect("the detail read answers")
        .expect("the item still reads through its turn");
    assert_eq!(detail.kind, ItemKind::Edit);
    assert_eq!(detail.tool.as_deref(), Some("apply_patch"));

    // Another fence names no item of this turn: the turn's fence is what found it.
    let other = History::new(held.fixtures.database.clone())
        .tool_call(
            &held.acquired.workspace_id,
            &held.fleet,
            &held.turn_id,
            CallAddress {
                fence: i64::try_from(held.fence.as_u64()).expect("a fence fits i64") + 1,
                call_number: 1,
            },
        )
        .await
        .expect("the detail read answers");
    assert!(other.is_none(), "{other:?}");
    held.fixtures.cleanup().await;
}
