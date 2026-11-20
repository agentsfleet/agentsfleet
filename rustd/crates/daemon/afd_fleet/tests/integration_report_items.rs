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

/// A post of one finished call per number in `calls`, under `fence`.
fn post(fence: u64, calls: &[u64]) -> String {
    let calls: Vec<String> = calls
        .iter()
        .map(|number| {
            format!(
                r#"{{"call_number":{number},"arguments":{{}},"truncated_arguments":false,"output":"ok","output_line_count":1,"truncated":false}}"#
            )
        })
        .collect();
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
    let first = post(dead, &[1, 2]);
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
    let last = post(live, &[1]);
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
