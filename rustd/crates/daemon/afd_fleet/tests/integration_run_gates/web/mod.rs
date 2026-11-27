//! Public policy and committed accounting observed by physical HTTP vendors.

mod extract;
mod scenario;
mod vendor;

use af_core::error_code;
use axum::http::StatusCode;
use serde_json::{Value, json};
use std::time::Duration;
use tokio::time::Instant;

use crate::support::Fixtures;
use scenario::Scenario;
use vendor::{Reply, key};

const BOTH: &[&str] = &["parallel", "exa"];
fn success() -> Reply {
    Reply::new(
        StatusCode::OK,
        json!({"results":[{"url":"https://example.org/","title":"Guide","excerpts":["Parallel passage"],"highlights":["Exa passage"]}]}),
    )
}
fn failed() -> Reply {
    Reply::new(StatusCode::INTERNAL_SERVER_ERROR, Value::Null)
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_vendor_call_is_counted_before_it_leaves() {
    let f = Fixtures::create().await;
    let s = Scenario::new(&f, BOTH, vec![success()]).await;
    assert_eq!(
        s.search().await.expect("fast vendor answers").vendor,
        "parallel"
    );
    let seen = s.vendor.seen();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].count, 1, "upstream saw committed reservation");
    assert_eq!(seen[0].body["mode"], "fast");
    f.delete_run(&s.live.run_id()).await;
    assert_eq!(s.reservations(&f).await, 0, "ledger follows run retention");
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_failed_vendor_falls_over_to_the_next() {
    let f = Fixtures::create().await;
    let s = Scenario::new(&f, BOTH, vec![failed(), success(), failed(), failed()]).await;
    let result = s.search().await.expect("fallback search");
    assert_eq!(result.vendor, "exa");
    assert_eq!(result.results[0].excerpts, ["Exa passage"]);
    let refusal = s.search().await.expect_err("both failed");
    assert_eq!(refusal.code(), error_code::WEB_VENDORS_FAILED);
    let detail = refusal.web_failure().expect("web failure").detail();
    assert!(detail.contains("parallel (HTTP 500)") && detail.contains("exa (HTTP 500)"));
    let seen = s.vendor.seen();
    assert_eq!(
        seen.iter()
            .map(|s| (s.name.as_str(), s.count))
            .collect::<Vec<_>>(),
        [("parallel", 1), ("exa", 1), ("parallel", 2), ("exa", 2)]
    );
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_cooling_vendor_is_skipped_until_its_named_wait_ends() {
    let f = Fixtures::create().await;
    let limited = Reply::new(StatusCode::TOO_MANY_REQUESTS, Value::Null);
    let s = Scenario::new(
        &f,
        BOTH,
        vec![
            limited.clone().cooling(30),
            limited.clone().cooling(5),
            success(),
            limited,
            success(),
            success(),
        ],
    )
    .await;
    assert_eq!(
        s.search()
            .await
            .expect_err("both rate limited initially")
            .code(),
        error_code::WEB_VENDORS_FAILED
    );
    let refusal = s.search().await.expect_err("cooldown");
    assert_eq!(refusal.code(), error_code::WEB_VENDORS_COOLING);
    assert_eq!(
        refusal.web_failure().expect("cooldown").retry_after(),
        Some(Duration::from_secs(5))
    );
    assert_eq!(
        s.reservations(&f).await,
        2,
        "cooldown makes no reservations"
    );
    s.clock.advance(Duration::from_secs(6));
    assert_eq!(
        s.search().await.expect("only Exa is available").vendor,
        "exa"
    );
    s.clock.advance(Duration::from_secs(25));
    assert_eq!(
        s.search()
            .await
            .expect("Parallel returns 429 without wait")
            .vendor,
        "exa"
    );
    assert_eq!(
        s.search().await.expect("no invented wait").vendor,
        "parallel"
    );
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_vendor_without_a_vault_key_is_skipped() {
    let f = Fixtures::create().await;
    let s = Scenario::new(&f, &["exa"], vec![success()]).await;
    assert_eq!(
        s.search().await.expect("only configured vendor").vendor,
        "exa"
    );
    assert_eq!(s.vendor.seen().len(), 1);
    drop(s);
    let empty = Scenario::new(&f, &[], vec![]).await;
    assert_eq!(
        empty
            .search()
            .await
            .expect_err("missing platform bags")
            .code(),
        error_code::WEB_VENDOR_UNCONFIGURED
    );
    assert_eq!(empty.reservations(&f).await, 0);
    assert!(empty.vendor.seen().is_empty());
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn no_vendor_key_reaches_a_runner_or_a_log() {
    let f = Fixtures::create().await;
    let s = Scenario::new(&f, BOTH, vec![failed(), success(), failed(), failed()]).await;
    let capture = af_core::test_util::trace::Capture::install();
    let result = s.search().await.expect("fallback answers");
    let error = s.search().await.expect_err("both vendors refuse");
    let observed = format!(
        "{} {} {} {:?}",
        s.live.payload,
        serde_json::to_string(&result).expect("verb JSON"),
        error,
        error
    );
    let events = capture.events();
    assert_eq!(
        events
            .iter()
            .filter(|event| event.field("event") == Some("web_call"))
            .count(),
        4
    );
    for name in BOTH {
        let sentinel = key(&s.live.run_id(), name);
        assert!(!observed.contains(&sentinel));
        assert!(!format!("{events:?}").contains(&sentinel));
    }
    assert!(s.vendor.seen().iter().all(|s| s.right_key));
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_stalled_vendor_is_dispatched_once_and_answers_inside_the_deadline() {
    let f = Fixtures::create().await;
    let mut stalled = success();
    stalled.delay = Duration::from_secs(6);
    let s = Scenario::new(&f, BOTH, vec![stalled, success()]).await;
    let deadline = Instant::now() + Duration::from_secs(3);
    assert_eq!(
        s.search_until(deadline)
            .await
            .expect("second vendor gets its share")
            .vendor,
        "exa"
    );
    assert!(Instant::now() < deadline);
    assert_eq!(s.reservations(&f).await, 2);
    assert_eq!(s.vendor.seen().len(), 2);
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_stale_fence_before_fallback_leaves_the_second_vendor_untouched() {
    let f = Fixtures::create().await;
    let mut move_holder = failed();
    move_holder.move_fence = true;
    let s = Scenario::new(&f, BOTH, vec![move_holder]).await;
    assert_eq!(
        s.search().await.expect_err("run superseded").code(),
        error_code::RUN_STALE_FENCING_TOKEN
    );
    assert_eq!(s.reservations(&f).await, 1);
    assert_eq!(s.vendor.seen().len(), 1);
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn stopping_after_reservation_keeps_one_count_and_sends_nothing() {
    let f = Fixtures::create().await;
    let mut s = Scenario::new(&f, BOTH, vec![]).await;
    let committed = std::sync::Arc::new(tokio::sync::Notify::new());
    s.plane.runs = s.plane.runs.stop_after_web_reservation(committed.clone());
    tokio::select! {
        result=s.search()=> panic!("tool finished before the commit interruption: {result:?}"),
        ()=committed.notified()=> {}
    }
    assert_eq!(s.reservations(&f).await, 1);
    assert!(s.vendor.seen().is_empty());
    f.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn concurrent_tool_calls_keep_both_reservations() {
    let f = Fixtures::create().await;
    let s = Scenario::new(&f, BOTH, vec![success(), success()]).await;
    let (first, second) = tokio::join!(
        s.search_until(Instant::now() + af_wire::web::WEB_DEADLINE),
        s.search_until(Instant::now() + af_wire::web::WEB_DEADLINE)
    );
    assert!(first.is_ok() && second.is_ok());
    assert_eq!(s.reservations(&f).await, 2);
    let seen = s.vendor.seen();
    assert_eq!(seen.len(), 2);
    assert!(seen.iter().all(|call| call.count > 0));
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn the_first_vendor_gets_its_share_under_the_minimum_runner_timeout() {
    let f = Fixtures::create().await;
    let share = af_wire::web::WEB_DEADLINE / 2;
    // Enough separation for the real database reservation and the loopback
    // scheduler; both calls still run at the deployment's actual timeouts.
    let margin = Duration::from_millis(600);
    let mut before = success();
    before.delay = share - margin;
    let mut after = success();
    after.delay = share + margin;
    let s = Scenario::new(&f, BOTH, vec![before, after, success()]).await;
    for expected in ["parallel", "exa"] {
        let answer = tokio::time::timeout(Duration::from_secs(13), s.search())
            .await
            .expect("answers before the minimum runner timeout")
            .expect("one vendor answers");
        assert_eq!(answer.vendor, expected);
    }
    assert_eq!(s.reservations(&f).await, 3);
    assert_eq!(s.vendor.seen().len(), 3);
    f.cleanup().await;
}
