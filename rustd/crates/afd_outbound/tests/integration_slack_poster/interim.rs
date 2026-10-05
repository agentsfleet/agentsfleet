//! An interim line goes out through the answer's poster, under its own marker
//! part, and a repeat of it looks for that part rather than for the answer.

use afd_connector::Provider;
use afd_outbound::{Interim, Interjector};

use super::*;

/// The line a run says before it answers.
const LINE: &str = "fix pushed as a draft";

/// The line the fixture job's thread is owed, numbered `part`.
fn interim(fixture: &Fixture, part: u32) -> Interim {
    let job = fixture.job();
    Interim {
        provider: Provider::Slack,
        destination: job.destination,
        workspace_id: job.workspace_id,
        fleet_id: job.fleet_id,
        event_id: job.event_id,
        text: LINE.to_owned(),
        part,
    }
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_interim_line_posts_under_its_own_part() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    fixture.seal_grant(BOT_TOKEN).await;
    let slack = slack_answering(200, r#"{"ok":true}"#).await;

    let delivered = Interjector::new(fixture.poster(&slack.api_base()))
        .interject(interim(&fixture, 2))
        .await;

    assert!(delivered, "a Slack that accepted has the line");
    let sent = received(&slack);
    assert_eq!(sent.field("text"), Some(LINE));
    assert_eq!(sent.field("thread_ts"), Some(THREAD));
    assert_eq!(sent.body["metadata"]["event_payload"]["part"], 2);
    assert_eq!(slack.reads(), 0, "a first attempt reads no thread");
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_refused_line_is_undelivered_without_a_retry() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    fixture.seal_grant(BOT_TOKEN).await;
    let slack = slack_answering(200, r#"{"ok":false,"error":"channel_not_found"}"#).await;

    let delivered = Interjector::new(fixture.poster(&slack.api_base()))
        .interject(interim(&fixture, 1))
        .await;

    assert!(!delivered);
    assert_eq!(
        slack.requests().len(),
        1,
        "a refusal is permanent: one post"
    );
    fixture.cleanup().await;
}

/// A vendor error is retried, and the retry asks the thread for THIS line's
/// marker before it posts again.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_retried_line_checks_the_thread_for_its_own_part() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    fixture.seal_grant(BOT_TOKEN).await;
    let slack = slack_answering(503, r#"{"ok":false}"#).await;

    let delivered = Interjector::new(fixture.poster(&slack.api_base()))
        .interject(interim(&fixture, 3))
        .await;

    assert!(
        !delivered,
        "a vendor down through every attempt delivers nothing"
    );
    assert!(slack.reads() >= 1, "a repeat looks before it posts");
    let posts = slack
        .requests()
        .into_iter()
        .filter(Request::is_post)
        .count();
    assert!(
        posts >= 2,
        "the first attempt and at least one repeat posted: {posts}"
    );
    fixture.cleanup().await;
}

/// Slack took the line and the acknowledgement was lost: the retry finds the
/// line by its own part in the thread and posts nothing more, so the thread
/// holds it exactly once.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_line_whose_acknowledgement_was_lost_is_not_posted_twice() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    fixture.seal_grant(BOT_TOKEN).await;
    let slack = slack_answering(503, r#"{"ok":false}"#).await;
    let landed = interim(&fixture, 4);
    let marker = afd_connector::slack::AnswerMarker {
        fleet_id: landed.fleet_id.clone(),
        event_id: landed.event_id.clone(),
        part: Some(4),
    };
    let stamp = serde_json::to_string(&marker.metadata()).expect("a stamp serializes");
    slack.answer(
        THREAD,
        200,
        &format!(
            r#"{{"ok":true,"messages":[{{"ts":"{THREAD}","user":"U01","text":"why?"}},
                {{"ts":"1712345678.000901","user":"{BOT_USER}","text":"{LINE}","metadata":{stamp}}}]}}"#
        ),
    );

    let delivered = Interjector::new(fixture.poster(&slack.api_base()))
        .interject(landed)
        .await;

    assert!(delivered, "the thread already holds the line");
    let posts = slack
        .requests()
        .into_iter()
        .filter(Request::is_post)
        .count();
    assert_eq!(posts, 1, "only the attempt whose answer was lost posted");
    fixture.cleanup().await;
}
