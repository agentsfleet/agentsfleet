//! The reviewing bundle, end to end: `github-pr-reviewer` installed from the
//! corpus into a real daemon and leased by the Rust runner's real loop, its
//! calls sent through the production egress to an HTTPS GitHub.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_wire::event::EventType;
use afr_tools::ToolErrorCode;
use agentsfleetd::supervisor::Supervisor;
use hyper::Method;
use serde_json::json;

use crate::bundle_install::install_bundle;
use crate::bundle_repair::{GITHUB, github_auth};
use crate::bundle_run::{posts, run_event};
use crate::e2e::REQUEST_JSON;
use crate::fake_model::{FakeModel, http, say};
use crate::https::{Reply, Route, Upstream};

/// The Pull Request the reviewer is pointed at.
const PULL: &str = "/repos/agentsfleet/linkwarden/pulls/7";
const REVIEWS: &str = "/repos/agentsfleet/linkwarden/pulls/7/reviews";

/// Dimension 6.3. The reviewer reads the diff in the media type it names and
/// posts its findings as one `COMMENT` review; an operator steer reads and
/// posts nothing. No write rule admits a review post yet, so today the post
/// is refused before it leaves and the refusal is what the model reads.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_pr_reviewer_posts_one_review() {
    let mut supervisor = Supervisor::new();
    let run = install_bundle(&mut supervisor, "github-pr-reviewer", &[], None).await;
    let routes = || {
        vec![
            Route::new(
                GITHUB,
                Method::GET,
                PULL,
                vec![Reply::json(
                    200,
                    &json!("diff --git a/src/a.ts b/src/a.ts\n+let id = user.id;\n"),
                )],
            ),
            Route::new(
                GITHUB,
                Method::POST,
                REVIEWS,
                vec![Reply::json(200, &json!({"id": 1}))],
            ),
        ]
    };
    let upstream = Upstream::serve(routes()).await;
    let review = json!({"event": "COMMENT", "body": "Two findings.", "comments": [
        {"path": "src/a.ts", "line": 1, "body": "`user` may be undefined here."},
        {"path": "src/a.ts", "line": 1, "body": "No test covers the missing user."}]});
    let (model, transcript) = FakeModel::new(vec![
        vec![http(
            "diff",
            json!({"url": format!("https://{GITHUB}{PULL}"), "headers": {
            "Authorization": "Bearer ${secrets.github.token}",
            "Accept": "application/vnd.github.diff"}}),
        )],
        vec![http(
            "review",
            json!({"url": format!("https://{GITHUB}{REVIEWS}"), "method": "POST",
            "headers": github_auth(), "body": review.to_string()}),
        )],
        vec![say("Posted one review with two findings.")],
    ]);

    let settled = run_event(&run, &run.event_id, &upstream, model).await;

    assert_eq!(settled.status, "processed");
    let seen = upstream.seen();
    let accept = seen
        .iter()
        .find(|request| request.path == PULL)
        .and_then(|request| request.headers.get("accept"))
        .and_then(|value| value.to_str().ok());
    assert_eq!(accept, Some("application/vnd.github.diff"));
    assert!(
        posts(&seen).is_empty(),
        "no review reached GitHub: {:?}",
        posts(&seen)
    );
    let refused = format!("[{}]", ToolErrorCode::RequestPolicyNotAllowed.as_str());
    let results = &transcript
        .asked()
        .last()
        .expect("the model was asked")
        .results
        .clone();
    assert!(
        results
            .last()
            .is_some_and(|result| result.starts_with(&refused)),
        "{results:#?}"
    );

    // A steer: the event's own text is what the model is asked, and the
    // script, following the SKILL.md, reaches for no upstream.
    let steer = run.enqueue_event(EventType::Chat).await;
    let quiet = Upstream::serve(routes()).await;
    let (model, transcript) = FakeModel::new(vec![vec![say("Noted; no review for a steer.")]]);
    let settled = run_event(&run, &steer, &quiet, model).await;
    assert_eq!(settled.status, "processed");
    let steer_text = serde_json::from_str::<serde_json::Value>(REQUEST_JSON)
        .ok()
        .and_then(|body| body.get("prompt")?.as_str().map(str::to_owned))
        .expect("the seeded steer carries a prompt");
    let asked = transcript.asked();
    assert!(
        asked.first().is_some_and(|turn| turn
            .user
            .iter()
            .any(|message| message.contains(&steer_text))),
        "the steer's text reached the model as its question: {asked:#?}"
    );
    assert!(quiet.seen().is_empty(), "a steer reads and posts nothing");

    supervisor.shutdown().await;
    run.cleanup().await;
}
