//! Policy, secret handling and bounded availability at the runner's search boundary.

#![expect(
    clippy::expect_used,
    reason = "a fixture returning the wrong result must fail the test"
)]

use std::sync::atomic::Ordering;
use std::time::Duration;

use af_wire::policy::HttpEgress;

use super::{CALL, Egressed, RunFixture, START, UNKNOWN, Verbs};
use crate::egress::READ_DEADLINE_EXCEEDED;
use crate::runtime::ToolErrorCode;

#[tokio::test]
async fn a_search_checks_the_run_before_dispatch_and_returns_only_displayable_results() {
    let verbs = Verbs::new(Ok(()));
    let mut fixture = RunFixture::new();
    fixture.policy.network_policy.http_egress = HttpEgress::Public;
    let run = fixture.run_reaching(&verbs);
    let web = Egressed::new(None, &run);
    let missing = web
        .search(CALL, "migration")
        .await
        .expect_err("unopened call");
    assert_eq!(missing.code, ToolErrorCode::AgentsfleetdRefused.into());
    run.calls.open(CALL, 8);
    let private = af_egress::fixture::GRAFANA_TOKEN;
    let refusal = web
        .search(CALL, private)
        .await
        .expect_err("protected query");
    assert_eq!(refusal.code, ToolErrorCode::InvalidArguments.into());
    assert!(!refusal.detail.contains(private));
    assert_eq!(verbs.calls.load(Ordering::SeqCst), 0);

    let results = web
        .search(CALL, "migration")
        .await
        .expect("search succeeds");
    let [row] = results.as_slice() else {
        unreachable!("only the public page can be displayed");
    };
    assert_eq!(row.url, UNKNOWN);
    assert_eq!(row.title.as_deref(), Some("Migration Guide"));
    assert_eq!(row.excerpts, ["Guide «secret:grafana.token»"]);
    assert_eq!(verbs.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn a_stalled_search_expires_after_one_dispatch() {
    let mut fixture = RunFixture::new();
    fixture.policy.network_policy.http_egress = HttpEgress::Public;
    let mut verbs = Verbs::new(Ok(()));
    verbs.delay = Duration::from_secs(30);
    let run = fixture.run_reaching(&verbs);
    run.calls.open(CALL, 8);
    let start = tokio::time::Instant::now();
    let refused = Egressed::new(None, &run)
        .search(CALL, "migration")
        .await
        .expect_err("search deadline");
    assert_eq!(refused.detail, READ_DEADLINE_EXCEEDED);
    assert_eq!(start.elapsed(), run.verbs.web_timeout());
    assert_eq!(verbs.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_runner_without_a_control_plane_or_transport_refuses_web_calls() {
    let mut fixture = RunFixture::new();
    fixture.policy.network_policy.http_egress = HttpEgress::Public;
    let run = fixture.run();
    run.calls.open(CALL, 8);
    let web = Egressed::new(None, &run);
    let search = web.search(CALL, "migration").await.expect_err("no daemon");
    assert_eq!(search.code, ToolErrorCode::AgentsfleetdRefused.into());
    let remote = web.fetch(CALL, UNKNOWN).await.expect_err("no extractor");
    assert_eq!(remote.code, ToolErrorCode::AgentsfleetdRefused.into());
    let local = web
        .fetch(CALL, START)
        .await
        .expect_err("no local transport");
    assert_eq!(local.code, ToolErrorCode::UpstreamUnreachable.into());
}
