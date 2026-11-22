//! An interim line goes out through the answer's poster, under its own marker
//! part, and every attempt at it, the first too, looks for that part rather
//! than for the answer.

use af_core::id::Uuid7;
use afd_connector::Provider;
use afd_outbound::{Interim, Interjector};

use super::*;

/// The line a run says before it answers.
const LINE: &str = "fix pushed as a draft";

/// The run the fixture line was said under.
const RUN: &str = "0195b4ba-8d3a-7a11-8abc-0000000000aa";

/// The line the fixture job's thread is owed, said by call `part`.
fn interim(fixture: &Fixture, part: u64) -> Interim {
    let job = fixture.job();
    Interim {
        provider: Provider::Slack,
        destination: job.destination,
        workspace_id: job.workspace_id,
        fleet_id: job.fleet_id,
        turn_id: job.turn_id,
        run_id: Uuid7::parse(RUN).expect("the fixture run is canonical"),
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
    let sent = (slack.requests().into_iter())
        .find(Request::is_post)
        .expect("the interjection posted after its thread read");
    assert_eq!(sent.field("text"), Some(LINE));
    assert_eq!(sent.field("thread_ts"), Some(THREAD));
    let stamped = |pointer: &str| sent.body.pointer(pointer).cloned();
    assert_eq!(
        stamped("/metadata/event_type"),
        Some(afd_connector::slack::INTERIM_EVENT_TYPE.into())
    );
    assert_eq!(
        stamped("/metadata/event_payload/part"),
        Some(serde_json::json!({"run_id": RUN, "line": 2}))
    );
    assert_eq!(
        slack.reads(),
        1,
        "the first attempt looks for its part: the call may be a retry"
    );
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
    let posts = (slack.requests().into_iter())
        .filter(Request::is_post)
        .count();
    assert_eq!(posts, 1, "a refusal is permanent: one post");
    assert_eq!(slack.reads(), 1, "the one attempt read its thread first");
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

/// The call posted its line, then its record was lost, so the runner calls
/// again with the same call number: the second interjection finds the line by
/// its part in the thread and posts nothing, so the thread holds it once.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_line_reposted_after_its_record_was_lost_is_not_posted_twice() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    fixture.seal_grant(BOT_TOKEN).await;
    let slack = slack_answering(200, r#"{"ok":true}"#).await;
    let interjector = Interjector::new(fixture.poster(&slack.api_base()));
    let first = interjector.interject(interim(&fixture, 4)).await;
    let landed = interim(&fixture, 4);
    let marker = afd_connector::slack::AnswerMarker {
        fleet_id: landed.fleet_id.clone().into(),
        turn_id: landed.turn_id.clone().into(),
        part: Some(afd_connector::slack::Part {
            run_id: RUN.to_owned(),
            line: 4,
        }),
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

    let retried = interjector.interject(landed).await;

    assert!(first && retried, "the thread holds the line both times");
    let posts = slack
        .requests()
        .into_iter()
        .filter(Request::is_post)
        .count();
    assert_eq!(posts, 1, "only the call's first interjection posted");
    assert_eq!(slack.reads(), 2, "each interjection looked first");
    fixture.cleanup().await;
}

/// A line that names the channel reaches the thread as text Slack shows
/// literally, so a steered fleet cannot page the channel.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_interim_line_naming_the_channel_posts_as_text() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    fixture.seal_grant(BOT_TOKEN).await;
    let slack = slack_answering(200, r#"{"ok":true}"#).await;
    let paging = Interim {
        text: "<!channel> look".to_owned(),
        ..interim(&fixture, 1)
    };

    let delivered = Interjector::new(fixture.poster(&slack.api_base()))
        .interject(paging)
        .await;

    assert!(delivered);
    let sent = (slack.requests().into_iter())
        .find(Request::is_post)
        .expect("the interjection posted after its thread read");
    assert_eq!(sent.field("text"), Some("&lt;!channel&gt; look"));
    fixture.cleanup().await;
}
