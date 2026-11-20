//! An operator reads one runner's runs and its history through the admin
//! routes, after the runner has served and reported a real run.

use super::{TestResult, fixture::Fixture, signals};
use crate::wire;
use af_stream::Transport;
use af_wire::protocol::StreamKind;
use serde_json::{Value, json};

// Value: protects=operators see each runner's runs and the history rows the
// run wrote; fails_when=either route stops answering, or a run or its
// acquired and released rows go missing from the page;
// why_new=no test read a served run back through the two admin routes;
// seam=two production daemons, authenticated HTTP, real Postgres, raw native runner.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn the_operator_reads_a_runners_runs_and_history() -> TestResult {
    let fixture = Fixture::start().await?;
    let mut native = fixture.native(Transport::H2).await;
    native.heartbeat().await;
    native.ready().await;
    let held = signals::assign(&mut native).await?;
    native
        .send(
            StreamKind::Reports,
            &json!({"type":"report","request_id":22,
                    "request":wire::report_body(&held.run, &held.event, held.fence)}),
        )
        .await;
    let ack = native.expect(StreamKind::Reports, "report_ack").await;
    assert_eq!(wire::pointer(&ack, "/answer/status"), "ok");

    let runner = format!("/v1/admin/runners/{}", fixture.scene.runner_id);
    let (status, runs) = fixture
        .operator
        .get(&fixture.scene, &format!("{runner}/runs"))
        .await?;
    assert_eq!(status, reqwest::StatusCode::OK, "{runs}");
    let served: Vec<(&str, &str, i64)> = items(&runs)?
        .iter()
        .map(|run| {
            (
                run.pointer("/id")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                run.pointer("/turn_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                run.pointer("/fencing_token")
                    .and_then(Value::as_i64)
                    .unwrap_or_default(),
            )
        })
        .collect();
    let fence = i64::try_from(held.fence)?;
    assert_eq!(
        served,
        vec![(held.run.as_str(), held.event.as_str(), fence)],
        "{runs}"
    );

    let (status, history) = fixture
        .operator
        .get(
            &fixture.scene,
            &format!("{runner}/history?event_type=run_acquired,run_released"),
        )
        .await?;
    assert_eq!(status, reqwest::StatusCode::OK, "{history}");
    let mut written: Vec<(&str, &str, &str)> = items(&history)?
        .iter()
        .map(|row| {
            (
                row.pointer("/event_type")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                row.pointer("/metadata/run_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                row.pointer("/metadata/turn_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            )
        })
        .collect();
    written.sort_unstable();
    assert_eq!(
        written,
        vec![
            ("run_acquired", held.run.as_str(), held.event.as_str()),
            ("run_released", held.run.as_str(), held.event.as_str()),
        ],
        "{history}"
    );
    native.close().await;
    fixture.finish().await
}

/// A page's `items`.
fn items(page: &Value) -> TestResult<&Vec<Value>> {
    Ok(page
        .get("items")
        .and_then(Value::as_array)
        .ok_or("a page carries items")?)
}
